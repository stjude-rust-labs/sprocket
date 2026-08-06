//! TODO

use proc_macro2::TokenStream;
use quote::format_ident;
use quote::quote;
use syn::Error;
use syn::Fields;
use syn::FieldsUnnamed;
use syn::Generics;
use syn::ItemEnum;
use syn::LitStr;
use syn::PathArguments;
use syn::Result;
use syn::Type;
use syn::parse_quote;
use syn::punctuated::Punctuated;
use syn::token::Paren;

use super::Args;

/// Builds the Python binding equivalent of an AST enum.
pub(super) fn build(original: &ItemEnum, args: Args) -> Result<TokenStream> {
    let mut py_enum = original.clone();

    py_enum.ident = super::make_py_ident(&original.ident);

    // Remove generics.
    py_enum.generics = Generics::default();

    make_py_attrs(&mut py_enum, original, args);

    // Strip generic from all variants, add "Py" prefix
    for variant in py_enum.variants.iter_mut() {
        let fields = match variant.fields {
            Fields::Unnamed(ref mut fields) => fields,
            Fields::Unit => {
                // Convert unit variants into empty tuple variants so that PyO3 supports them.
                variant.fields = Fields::Unnamed(FieldsUnnamed {
                    paren_token: Paren::default(),
                    unnamed: Punctuated::new(),
                });
                continue;
            }
            _ => {
                return Err(Error::new_spanned(
                    &variant.fields,
                    "`#[ast]` does not support struct variants",
                ));
            }
        };

        if fields.unnamed.is_empty() {
            continue;
        }

        if fields.unnamed.len() > 1 {
            return Err(Error::new_spanned(
                fields,
                "`#[ast]` requires enums variants have zero or one fields",
            ));
        }

        if let Type::Path(ref mut type_path) = fields.unnamed.first_mut().unwrap().ty {
            let segment = type_path
                .path
                .segments
                .last_mut()
                .expect("paths are expected to have at least one segment");

            segment.ident = format_ident!("Py{}", segment.ident);
            segment.arguments = PathArguments::None
        }
    }

    // Used for quote formatting.
    let ident = &original.ident;
    let py_ident = &py_enum.ident;
    let from_match_arms = original.variants.iter().map(|variant| {
        let variant_ident = &variant.ident;

        match variant.fields {
            Fields::Unnamed(ref fields_unnamed) => {
                let field_names = (0..fields_unnamed.unnamed.len()).map(|i| format_ident!("_{i}"));
                let field_names2 = field_names.clone().map(|i| quote! { #i.into() });

                // This tends to look like `Foo::Bar(_0, _1) => PyFoo::Bar(_0.into(), _1.into())`.
                quote!(#ident::#variant_ident(#(#field_names),*) => #py_ident::#variant_ident(#(#field_names2),*))
            },
            // Convert unit variant to empty tuple variant.
            Fields::Unit => quote!(#ident::#variant_ident => #py_ident::#variant_ident()),
            Fields::Named(_) => unreachable!("struct variants were previously filtered out"),
        }
    });

    Ok(quote! {
        #py_enum

        // Converting from the original struct to the Python struct. This is used in the original
        // struct's `IntoPyObject` impl.
        impl ::std::convert::From<#ident> for #py_ident {
            fn from(value: #ident) -> Self {
                match value {
                    #(#from_match_arms),*
                }
            }
        }

        // Let the original enum be converted directly into a Python object. This lets the original
        // enum be returned directly from Python methods.
        impl<'py> ::pyo3::conversion::IntoPyObject<'py> for #ident {
            type Target = <#py_ident as ::pyo3::conversion::IntoPyObject<'py>>::Target;
            type Output = <#py_ident as ::pyo3::conversion::IntoPyObject<'py>>::Output;
            type Error = <#py_ident as ::pyo3::conversion::IntoPyObject<'py>>::Error;

            // We qualify `Self::Output` because this would be ambiguous for enums with a variant
            // named `Output`.
            fn into_pyobject(self, py: ::pyo3::marker::Python<'py>) -> Result<<Self as ::pyo3::conversion::IntoPyObject<'py>>::Output, Self::Error> {
                #py_ident::from(self).into_pyobject(py)
            }
        }
    })
}

/// Modifies the attributes of `py_enum`.
///
/// This first removes all of `py_enum`'s attributes except for its doc
/// comments. Then, it appends attributes like `#[pyclass]` that are necessary
/// for making it a Python type.
fn make_py_attrs(py_enum: &mut ItemEnum, original: &ItemEnum, args: Args) {
    // Only copy over doc comments, remove all other attributes.
    py_enum.attrs.retain(|attr| attr.path().is_ident("doc"));

    py_enum.attrs.extend_from_slice(&[
        // Make generated enum a pyclass.
        {
            let module = args.module;
            let class_name = LitStr::new(&original.ident.to_string(), original.ident.span());

            parse_quote!(#[::pyo3::pyclass(module = #module, name = #class_name, frozen, from_py_object, eq)])
        },
        // `Clone` is required by `from_py_object`, and `PartialEq` is for parity with the original
        // enum.
        parse_quote!(#[derive(Clone, PartialEq)]),
        // `Debug` is purposefully not implemented, silence the lint.
        parse_quote!(#[allow(missing_debug_implementations)]),
    ]);
}
