//! TODO

use proc_macro::TokenStream;
use syn::{Item, parse_macro_input};

/// TODO
#[proc_macro_attribute]
pub fn ast(_args: TokenStream, item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as Item);

    match item {
        Item::Struct(struct_) => todo!(),
        Item::Enum(enum_) => todo!(),
        unsupported => {
            syn::Error::new_spanned(unsupported, "#[ast] only supports structs and enums")
                .into_compile_error()
                .into()
        }
    }
}
