//! TODO

use proc_macro2::TokenStream;
use quote::quote;
use syn::ItemImpl;
use syn::Result;
use syn::parse::Nothing;

pub(crate) fn ast_methods(
    args_stream: TokenStream,
    impl_stream: TokenStream,
) -> Result<TokenStream> {
    syn::parse2::<Nothing>(args_stream)?;
    let mut impl_ = syn::parse2::<ItemImpl>(impl_stream)?;

    todo!();

    Ok(quote! {
        #impl_
    })
}
