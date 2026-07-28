//! TODO

pub(crate) mod struct_;

use proc_macro2::Span;
use proc_macro2::TokenStream;
use quote::format_ident;
use syn::ItemStruct;
use syn::LitStr;
use syn::Result;
use syn::parse::Parser;

/// Arguments to the `#[ast]` attribute.
#[derive(PartialEq, Debug)]
pub(crate) struct Args {
    /// The module the type is defined in, from Python's perspective.
    ///
    /// This is forwarded to `#[pyclass(module = ...)]`.
    pub(crate) module: LitStr,
}

impl Args {
    /// Parses a token stream into structured arguments.
    pub(crate) fn parse(args_stream: TokenStream) -> Result<Self> {
        let mut args = Self::default();

        let args_parser = syn::meta::parser(|meta| {
            if meta.path.is_ident("module") {
                args.module = meta.value()?.parse()?;
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
        }
    }
}

/// Modifies the [`Ident`] of `py_struct` to its Python name.
fn build_ident(py_struct: &mut ItemStruct, original: &ItemStruct) {
    py_struct.ident = format_ident!("Py{}", original.ident);
}

#[cfg(test)]
mod tests {
    use quote::quote;
    use syn::parse_quote;

    use super::*;

    #[test]
    fn empty_args() {
        let args_stream = quote!();
        let args = Args::parse(args_stream).unwrap();

        assert_eq!(args, Args::default());
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

        build_ident(&mut py_struct, &original);

        assert_eq!(py_struct.ident.to_string(), "PyFoo");
    }
}
