//! TODO

use proc_macro2::TokenStream;
use quote::format_ident;
use quote::quote;
use syn::Fields;
use syn::Generics;
use syn::ItemStruct;
use syn::LitStr;
use syn::parse_quote;

/// Gives the Python binding equivalent of an AST struct.
pub(crate) fn build(original: &ItemStruct) -> syn::Result<TokenStream> {
    let mut py_struct = original.clone();

    py_struct.ident = format_ident!("Py{}", original.ident);

    // Only copy over doc comments, remove all other attributes.
    py_struct.attrs.retain(|attr| attr.path().is_ident("doc"));

    py_struct.attrs.extend_from_slice(&[
        // Make generated struct a pyclass.
        {
            let class_name = LitStr::new(&original.ident.to_string(), original.ident.span());
            parse_quote!(#[::pyo3::pyclass(module = "sprocket_bio.ast.v1", name = #class_name, extends = crate::PyAstNode, frozen, skip_from_py_object, eq)])
        },
        // `#[pymethods]` relies on the Python struct being cloneable. We additionally derive
        // `PartialEq` for parity with the original struct.
        parse_quote!(#[derive(Clone, PartialEq)]),
        // `Debug` is purposefully not implemented, silence the lint.
        parse_quote!(#[allow(missing_debug_implementations)]),
    ]);

    // Verify generics.
    if original.generics != parse_quote!(<N: TreeNode = SyntaxNode>) {
        return Err(syn::Error::new_spanned(
            &original.generics,
            "`#[ast]` requires that struct generics be `<N: TreeNode = SyntaxNode>`",
        ));
    }

    // Remove generics.
    py_struct.generics = Generics::default();

    // Verify fields.
    if let Fields::Unnamed(ref fields) = original.fields {
        if fields.unnamed.len() != 1 {
            return Err(syn::Error::new_spanned(
                fields,
                "`#[ast]` only supports structs with a single field",
            ));
        }
    } else {
        return Err(syn::Error::new_spanned(
            &original.fields,
            "`#[ast]` only supports tuple structs",
        ));
    }

    // Replace generic `N: SyntaxNode` field with thread-safe variant.
    py_struct.fields = Fields::Unnamed(parse_quote!((crate::python::ThreadSafeSyntaxNode)));

    // Used for quote formatting.
    let ident = &original.ident;
    let py_ident = &py_struct.ident;

    Ok(quote! {
        #py_struct

        // Converting from the original struct to the Python struct. This is used in the original
        // struct's `IntoPyObject` impl.
        impl ::std::convert::From<#ident> for #py_ident {
            fn from(value: #ident) -> Self {
                Self(value.0.into())
            }
        }

        // Converting from the Python struct to the original struct. This is used by Python methods
        // to call their original counterparts.
        impl ::std::convert::From<#py_ident> for #ident {
            fn from(value: #py_ident) -> Self {
                Self(value.0.into())
            }
        }

        // Let the original struct be converted directly into a Python object. This lets the
        // original struct be returned directly from Python methods.
        impl<'py> ::pyo3::conversion::IntoPyObject<'py> for #ident {
            type Target = ::pyo3::types::PyAny;
            type Output = ::pyo3::Bound<'py, Self::Target>;
            type Error = ::pyo3::PyErr;

            fn into_pyobject(self, py: ::pyo3::marker::Python<'py>) -> Result<Self::Output, Self::Error> {
                use ::pyo3::prelude::*;

                // Convert `self` to its Python counterpart, make it a subclass of `PyAstNode`,
                // allocate it on Python's heap, then cast it to `PyAny`.
                Bound::new(py, PyClassInitializer::from(crate::PyAstNode).add_subclass(#py_ident::from(self)))
                    .map(Bound::into_any)
            }
        }
    })
}
