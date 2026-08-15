//! The `#[ast]` implementation.

mod enum_;
mod struct_;

use proc_macro2::Span;
use proc_macro2::TokenStream;
use quote::format_ident;
use quote::quote;
use syn::Error;
use syn::Ident;
use syn::Item;
use syn::LitStr;
use syn::Result;
use syn::parse::Parser;

/// Arguments to the `#[ast]` attribute.
#[derive(PartialEq, Debug)]
struct Args {
    /// The module the type is defined in, from Python's perspective.
    ///
    /// This is forwarded to `#[pyclass]`, and defaults to `module =
    /// "sprocket_bio.ast.v1"`.
    module: LitStr,
    /// Implements `__eq__` using the `PartialEq` implementation of the original
    /// type.
    ///
    /// This is forwarded to `#[pyclass]`, and by default is omitted.
    eq: bool,
    /// Implements `__str__` using the `Display` implementation of the
    /// original type.
    ///
    /// This is forwarded to `#[pyclass]`, and by default is omitted.
    str_: bool,
}

impl Args {
    /// Parses a token stream into structured arguments.
    fn parse(args_stream: TokenStream) -> Result<Self> {
        let mut args = Self::default();

        let args_parser = syn::meta::parser(|meta| {
            if meta.path.is_ident("module") {
                args.module = meta.value()?.parse()?;
                return Ok(());
            }

            if meta.path.is_ident("eq") {
                args.eq = true;
                return Ok(());
            }

            if meta.path.is_ident("str") {
                args.str_ = true;
                return Ok(());
            }

            Err(meta.error("unknown `#[ast]` argument"))
        });

        args_parser.parse2(args_stream)?;

        Ok(args)
    }
}

impl Default for Args {
    fn default() -> Self {
        Self {
            module: LitStr::new("sprocket_bio.ast.v1", Span::call_site()),
            eq: false,
            str_: false,
        }
    }
}

/// See [`#[ast]`](super::ast).
pub(crate) fn ast(args_stream: TokenStream, item_stream: TokenStream) -> Result<TokenStream> {
    let args = Args::parse(args_stream)?;
    let item = syn::parse2::<Item>(item_stream)?;

    let expanded = match &item {
        Item::Struct(struct_) => struct_::build(struct_, args)?,
        Item::Enum(enum_) => enum_::build(enum_, args)?,
        unsupported => {
            return Err(Error::new_spanned(
                unsupported,
                "`#[ast]` only supports structs and enums",
            ));
        }
    };

    Ok(quote! {
        #item
        #expanded
    })
}

/// Makes the Python item's name from the original item's [`Ident`].
fn make_py_ident(original: &Ident) -> Ident {
    format_ident!("Py{}", original)
}

#[cfg(test)]
mod tests {
    use quote::quote;
    use syn::ItemStruct;
    use syn::parse_quote;

    use super::*;

    #[test]
    fn empty_args() {
        let args_stream = quote!();
        let args = Args::parse(args_stream).unwrap();

        assert_eq!(args, Args::default());
    }

    #[test]
    fn str_arg() {
        let str_default_stream = quote!(str);
        let str_default_args = Args::parse(str_default_stream).unwrap();

        assert!(str_default_args.str_);

        // This is supported by `#[pyclass]`, but is incompatible with `#[pyclass(name =
        // ...)]` so we don't allow it.
        let str_format_stream = quote!(str = "Hello, {name:?}!");
        let result = Args::parse(str_format_stream);

        assert!(
            result.is_err(),
            "should have errored on format string: {result:?}"
        );
    }

    #[test]
    fn module_arg() {
        let args_stream = quote!(module = "sprocket_bio.super_cool_module");
        let args = Args::parse(args_stream).unwrap();

        assert_eq!(args.module.value(), "sprocket_bio.super_cool_module");
    }

    #[test]
    fn unknown_arg() {
        let args_stream = quote!(spooky = "👻");
        let result = Args::parse(args_stream);

        assert!(
            result.is_err(),
            "did not error on unknown argument: {result:?}"
        );
    }

    #[test]
    fn ident() {
        let original: ItemStruct = parse_quote! { struct Foo; };
        let mut py_struct = original.clone();

        py_struct.ident = make_py_ident(&original.ident);

        assert_eq!(py_struct.ident.to_string(), "PyFoo");
    }
}
