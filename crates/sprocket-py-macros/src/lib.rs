//! TODO

mod ast;

use proc_macro::TokenStream;
use quote::quote;
use syn::Item;
use syn::ItemImpl;
use syn::parse::Nothing;
use syn::parse_macro_input;

/// TODO
#[proc_macro_attribute]
pub fn ast(args: TokenStream, item: TokenStream) -> TokenStream {
    parse_macro_input!(args as Nothing);
    let item = parse_macro_input!(item as Item);

    let expanded = match &item {
        Item::Struct(struct_) => ast::struct_::build(struct_),
        Item::Enum(_enum_) => todo!(),
        unsupported => Err(syn::Error::new_spanned(
            unsupported,
            "`#[ast]` only supports structs and enums",
        )),
    }
    .unwrap_or_else(syn::Error::into_compile_error);

    quote! {
        #item
        #expanded
    }
    .into()
}

/// TODO
#[proc_macro_attribute]
pub fn ast_methods(args: TokenStream, item: TokenStream) -> TokenStream {
    parse_macro_input!(args as Nothing);
    let mut methods = parse_macro_input!(item as ItemImpl);

    let expanded = ast::methods::build(&mut methods).unwrap_or_else(syn::Error::into_compile_error);

    quote! {
        #methods
        #expanded
    }
    .into()
}
