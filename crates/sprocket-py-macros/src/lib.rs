//! Macros that assist with generating [Sprocket's Python
//! bindings](https://github.com/stjude-rust-labs/sprocket/tree/main/python).
//!
//! This crate is unstable and is not intended to be consumed outside of
//! Sprocket's WDL crates. While it will follow [Semantic Versioning](https://semver.org/),
//! use at your own risk.

mod ast;
mod ast_methods;

use proc_macro::TokenStream;
use syn::Error;

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
/// To make AST element methods available in Python, please see
/// [`#[ast_methods]`](ast_methods).
///
/// # Arguments
///
/// - `module = "my.python.submodule"`: The module the AST element is defined
///   in, from Python's perspective. This is forwarded to `#[pyclass]`, and
///   defaults to `module = "sprocket_bio.ast.v1"`.
/// - `str`: Implements `__str__` using the [`Display`](std::fmt::Display)
///   implementation of the underlying Rust datatype. This is forwarded to
///   `#[pyclass]`, and by default is omitted. Note that format strings (like
///   `str = "{format_str:?}"`) are not supported because this macro internally
///   uses `#[pyclass(name = ...)]`.
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
/// #[pyclass(module = "sprocket_bio.ast.v1", name = "Ast", extends = PyAstNode, frozen, from_py_object, eq)]
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
    ast::ast(args_stream.into(), item_stream.into())
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

/// Makes AST element methods available in Python.
///
/// When added to an `impl` block, this attribute will make all **public**
/// functions available in Python. If an public function is annotated with
/// `#[skip]`, it will not be processed by this macro. These functions must
/// return a type that implements `IntoPyObject`. If a function returns an `impl
/// Iterator<Item = T>` where `T: IntoPyObject`, it will be adapted by the
/// attribute to return a list in Python.
///
/// As a general rule of thumb, add this attribute to the main `impl` block for
/// any type annotated with [`#[ast]`](ast).
///
/// # Arguments
///
/// This attribute does not accept any arguments.
///
/// # Requirements
///
/// - The type these methods are implemented for must be annotated with
///   [`#[ast]`](ast).
/// - The `impl` block must have zero or one generic type that is bound by
///   either `TreeNode` or `TreeToken`. (Ex. `impl<N: TreeNode>` or `impl<T:
///   TreeToken>`).
/// - This attribute can only be used once per AST element.
/// - `pyo3` must be available for import with the `macros` feature enabled.
///
/// # Examples
///
/// ```ignore
/// #[ast_methods]
/// impl<N: TreeNode> EnumDefinition<N> {
///     /// Some documentation...
///     pub fn name(&self) -> Ident<N::Token> {
///         // ...
///     }
///
///     /// Other docs...
///     pub fn keyword(&self) -> EnumKeyword<N::Token> {
///         // ...
///     }
///
///     pub fn choices(&self) -> impl Iterator<Item = EnumChoice<N>> + use<'_, N> {
///         // ...
///     }
/// }
/// ```
///
/// The above code roughly expands to:
///
/// ```ignore
/// // The original `impl` is untouched.
/// impl<N: TreeNode> EnumDefinition<N> {
///     // ...
/// }
///
/// // A new `impl` is created for the Python version of `EnumDefinition`.
/// #[pymethods]
/// impl PyEnumDefinition {
///     /// Some documentation...
///     fn py_name(&self) -> Ident {
///         EnumDefinition::from(self.clone()).name()
///     }
///
///     /// Other docs...
///     fn py_keyword(&self) -> EnumKeyword {
///         EnumDefinition::from(self.clone()).keyword()
///     }
///
///     fn py_choices<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
///         PyList::new(py, EnumDefinition::from(self.clone()).choices())
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn ast_methods(args_stream: TokenStream, impl_stream: TokenStream) -> TokenStream {
    ast_methods::ast_methods(args_stream.into(), impl_stream.into())
        .unwrap_or_else(Error::into_compile_error)
        .into()
}
