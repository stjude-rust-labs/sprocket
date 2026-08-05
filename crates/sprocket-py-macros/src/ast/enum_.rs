//! TODO

use proc_macro2::TokenStream;
use quote::quote;
use syn::Generics;
use syn::ItemEnum;
use syn::LitStr;
use syn::Result;
use syn::parse_quote;

use super::Args;

pub(super) fn build(original: &ItemEnum, args: Args) -> Result<TokenStream> {
    let mut py_enum = original.clone();

    py_enum.ident = super::make_py_ident(&original.ident);

    // Remove generics.
    py_enum.generics = Generics::default();

    make_py_attrs(&mut py_enum, original, args);

    todo!();

    Ok(quote! {
        #py_enum
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

            parse_quote!(#[::pyo3::pyclass(module = #module, name = #class_name, frozen, skip_from_py_object, eq)])
        },
        // We derive `PartialEq` for parity with the original enum.
        parse_quote!(#[derive(PartialEq)]),
        // `Debug` is purposefully not implemented, silence the lint.
        parse_quote!(#[allow(missing_debug_implementations)]),
    ]);
}
