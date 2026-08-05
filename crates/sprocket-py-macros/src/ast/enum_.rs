//! TODO

use proc_macro2::TokenStream;
use quote::ToTokens;
use quote::format_ident;
use syn::Error;
use syn::Fields;
use syn::Generics;
use syn::ItemEnum;
use syn::LitStr;
use syn::PathArguments;
use syn::Result;
use syn::Type;
use syn::parse_quote;

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
            Fields::Unit => continue,
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

    Ok(py_enum.to_token_stream())
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
