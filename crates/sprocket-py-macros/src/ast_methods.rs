//! TODO

use proc_macro2::TokenStream;
use quote::quote;
use syn::ItemImpl;
use syn::Result;
use syn::parse::Nothing;
use syn::parse_quote;

pub(crate) fn ast_methods(
    args_stream: TokenStream,
    impl_stream: TokenStream,
) -> Result<TokenStream> {
    syn::parse2::<Nothing>(args_stream)?;
    let impl_ = syn::parse2::<ItemImpl>(impl_stream)?;

    let mut py_impl = impl_.clone();

    // Annotate the Python `impl` with `#[pymethods]`.
    py_impl.attrs.push(parse_quote!(#[::pyo3::pymethods]));

    Ok(quote! {
        #impl_
        #py_impl
    })
}
