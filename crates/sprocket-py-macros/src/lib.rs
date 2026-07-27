//! Macros that assist with generating [Sprocket's Python
//! bindings](https://github.com/stjude-rust-labs/sprocket/tree/main/python).
//!
//! This crate is unstable and is not intended to be consumed outside of
//! Sprocket's WDL crates. While it will follow [Semantic Versioning](https://semver.org/),
//! use at your own risk.

mod ast;
mod ast_methods;

use proc_macro::TokenStream;
use quote::quote;
use syn::Item;
use syn::ItemImpl;
use syn::parse::Nothing;
use syn::parse_macro_input;

/// Creates the Python equivalent of an AST element.
///
/// Given an AST node or token named `Foo` annotated with this macro, it will
/// generate a new struct named `PyFoo`. This new struct will contain a
/// `ThreadSafeSyntaxNode` or `ThreadSafeSyntaxToken` field and be annotated
/// with `pyo3::pyclass`, allowing it to be used in Python bindings. [`From`]
/// implementations will be created to allow easy conversion between `Foo` and
/// `PyFoo`. The doc comments from `Foo` will be copied over to `PyFoo`.
///
/// Additionally, this macro will implement `pyo3::conversion::IntoPyObject` for
/// the original `Foo` struct, letting it be returned directly from Python
/// methods without first being converted to `PyFoo`.
///
/// # Arguments
///
/// - `module = "sprocket_bio.ast.v1"`: The module the AST element is defined
///   in, from Python's perspective. This is forwarded to `#[pyclass(module =
///   ...)]`.
///
/// # Requirements
///
/// - The type generics must be either `<N: TreeNode = SyntaxNode>` or `<T:
///   TreeToken = SyntaxToken>`.
/// - The type must be a tuple struct with a single field for the node or token
///   generic type.
/// - The type must implement [`PartialEq`].
/// - This attribute can only be used within the `wdl-ast` crate.
/// - `pyo3` must be available for import with the `macros` feature enabled.
///
/// # Examples
///
/// ```ignore
/// # use sprocket_py_macros::ast;
/// #
/// /// Some documentation...
/// #[ast]
/// #[derive(PartialEq)]
/// struct Ast<N: TreeNode = SyntaxNode>(N);
/// ```
///
/// The above code roughly expands to:
///
/// ```ignore
/// /// Some documentation...
/// #[derive(PartialEq)]
/// struct Ast<N: TreeNode = SyntaxNode>(N);
///
/// /// Some documentation...
/// #[pyclass(module = "sprocket_bio.ast.v1", name = "Ast", extends = PyAstNode, frozen, skip_from_py_object, eq)]
/// #[derive(Clone, PartialEq)]
/// struct PyAst(ThreadSafeSyntaxNode);
///
/// impl IntoPyObject for Ast {
///     // ...
/// }
///
/// impl From<Ast> for PyAst {
///     // ...
/// }
///
/// impl From<PyAst> for Ast {
///     // ...
/// }
/// ```
#[proc_macro_attribute]
pub fn ast(args_stream: TokenStream, item_stream: TokenStream) -> TokenStream {
    let mut args = ast::Args::default();

    let args_parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("module") {
            args.module = meta.value()?.parse()?;
            return Ok(());
        }

        Err(meta.error("unknown `#[ast]` argument"))
    });

    parse_macro_input!(args_stream with args_parser);
    let item = parse_macro_input!(item_stream as Item);

    let expanded = match &item {
        Item::Struct(struct_) => ast::build(struct_, args),
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

    let expanded = ast_methods::build(&mut methods).unwrap_or_else(syn::Error::into_compile_error);

    quote! {
        #methods
        #expanded
    }
    .into()
}
