//! TODO

use proc_macro2::Ident;
use proc_macro2::TokenStream;
use quote::format_ident;
use quote::quote;
use syn::Attribute;
use syn::Fields;
use syn::Generics;
use syn::ItemStruct;
use syn::LitStr;
use syn::Path;
use syn::parse_quote;

/// Gives the Python binding equivalent of an AST struct.
pub(crate) fn build(original: &ItemStruct) -> syn::Result<TokenStream> {
    verify_generics(&original.generics)?;
    verify_fields(&original.fields)?;

    let py_struct = py_struct(&original);
    let py_impl = py_impl(&original, &py_struct.ident);

    Ok(quote! {
        #py_struct
        #py_impl
    })
}

fn verify_generics(_generics: &Generics) -> syn::Result<()> {
    // TODO: Assert generics are just `<N: TreeNode = SyntaxNode>`.
    Ok(())
}

fn verify_fields(fields: &Fields) -> syn::Result<()> {
    let Fields::Unnamed(fields) = fields else {
        return Err(syn::Error::new_spanned(
            fields,
            "`#[ast]` only supports tuple structs",
        ));
    };

    if fields.unnamed.len() != 1 {
        return Err(syn::Error::new_spanned(
            fields,
            "`#[ast]` only supports structs with a single field",
        ));
    }

    Ok(())
}

fn py_attrs(original_attrs: &[Attribute], class_name: &Ident) -> Vec<Attribute> {
    let doc_path: Path = parse_quote!(doc);

    // Extract all `#[doc = "..."]` attributes from the original struct.
    let doc_attrs = original_attrs
        .iter()
        .filter(move |attr| *attr.path() == doc_path)
        .cloned();

    let class_name = LitStr::new(&class_name.to_string(), class_name.span());

    let additional_attrs = [
        // Make generated struct a pyclass.
        parse_quote!(#[::pyo3::pyclass(module = "sprocket_bio.ast.v1", name = #class_name, extends = crate::PyAstNode, frozen, skip_from_py_object, eq)]),
        // `#[pymethods]` relies on the Python struct being cloneable. We additionally derive
        // `PartialEq` for parity with the original struct.
        parse_quote!(#[derive(Clone, PartialEq)]),
        // `Debug` is purposefully not implemented, silence the lint.
        parse_quote!(#[allow(missing_debug_implementations)]),
    ];

    doc_attrs.chain(additional_attrs).collect()
}

fn py_struct(original: &ItemStruct) -> ItemStruct {
    let ItemStruct {
        attrs,
        vis,
        struct_token,
        ident,
        semi_token,
        ..
    } = original;

    // Only copy over doc comments, not other attributes.
    let py_attrs = py_attrs(attrs, ident);

    // Prepend "Py" to the beginning of the name.
    let py_ident = format_ident!("Py{ident}");

    ItemStruct {
        attrs: py_attrs,
        vis: vis.clone(),
        struct_token: *struct_token,
        ident: py_ident.clone(),
        // Generics are removed.
        generics: Generics::default(),
        // Replace generic `SyntaxNode` field with thread-safe variant.
        fields: Fields::Unnamed(parse_quote!((crate::python::ThreadSafeSyntaxNode))),
        semi_token: *semi_token,
    }
}

fn py_impl(original: &ItemStruct, py_ident: &Ident) -> TokenStream {
    let ItemStruct { ident, .. } = original;

    quote! {
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
    }
}
