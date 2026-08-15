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

/// Creates a duplicate of the annotated AST type that can be used in Python
/// bindings.
///
/// This attribute is a specialized version of `#[pyclass]` intended for use by
/// `wdl-ast`. Instead of making the annotated type a Python object, it instead
/// creates a duplicate type, prefixing "Py" to the name, that becomes the
/// new Python object. For example, given a type named `Foo`, this attribute
/// will generate a new type named `PyFoo`.
///
/// This attribute behaves differently depending on whether it is applied to a
/// struct or an enum. For structs, it requires the annotated type to be an AST
/// node and token. For enums, it requires each variant to have zero or one
/// fields, and that field must be another type annotated with `#[ast]`. For
/// more complete examples of behavior, please see the [Struct](#struct) and
/// [Enum](#enum) sections.
///
/// This attribute additionally implements `IntoPyObject` for the type it
/// annotates, meaning you can return it directly from functions in an
/// [`#[ast_methods]`](ast_methods) block and it will be automatically converted
/// to its Python equivalent at the FFI boundary.
///
/// This macro will only work when used within the `wdl-ast` crate with the
/// `unstable-python` feature enabled, and will fail to compile otherwise.
///
/// # Struct
///
/// The structs `#[ast]` is applied to must be AST nodes and tokens (usually
/// implementing the `AstNode` or `AstToken` trait). They are required to be
/// tuple structs, with either `<N: TreeNode = SyntaxNode>` or `<T: TreeToken =
/// SyntaxToken>` as their generics, and they must have a single field for that
/// generic type. For example, the
/// following two structs are valid:
///
/// ```ignore
/// #[ast]
/// struct MyNode<N: TreeNode = SyntaxNode>(N);
///
/// #[ast]
/// struct MyToken<T: TreeToken = SyntaxToken>(T);
/// ```
///
/// This attribute will apply the following transformations on the duplicate
/// Python struct:
///
/// - Prefix the name with "Py"
/// - Remove generics
/// - Remove all original attributes except for doc comments
/// - Add `#[pyclass]` and other necessary derive attributes
/// - Replace the field with a `ThreadSafeSyntaxNode` or
///   `ThreadSafeSyntaxToken`, depending on whether the struct is an AST node or
///   token
/// - Implement [`From`] to allow converting from the original to the Python
///   struct and back
/// - Implement `IntoPyObject` for both the Python and the original struct
///
/// Using these transformations, the above code will approximately expand to the
/// following:
///
/// ```ignore
/// // Original struct.
/// struct MyNode<N: TreeNode = SyntaxNode>(N);
///
/// // Python struct.
/// #[pyclass(
///     module = "sprocket_bio.ast.v1",
///     name = "MyNode",
///     extends = crate::PyAstNode,
///     frozen,
///     from_py_object,
///     eq,
/// )]
/// #[derive(Clone, PartialEq)]
/// struct PyMyNode(crate::python::ThreadSafeSyntaxNode);
///
/// // Conversion between the original and Python struct.
/// impl From<MyNode> for PyMyNode { /* ... */ }
/// impl From<PyMyNode> for MyNode { /* ... */ }
///
/// // Converts the structs into Python objects.
/// impl<'py> IntoPyObject<'py> for PyMyNode { /* ... */ }
/// impl<'py> IntoPyObject<'py> for MyNode { /* ... */ }
///
/// // Original struct.
/// struct MyToken<T: TreeToken = SyntaxToken>(T);
///
/// // Python struct.
/// #[pyclass(
///     module = "sprocket_bio.ast.v1",
///     name = "MyToken",
///     extends = crate::PyAstToken,
///     frozen,
///     from_py_object,
///     eq,
/// )]
/// #[derive(Clone, PartialEq)]
/// struct PyMyToken(crate::python::ThreadSafeSyntaxToken);
///
/// // Conversion between the original and Python struct.
/// impl From<MyToken> for PyMyToken { /* ... */ }
/// impl From<PyMyToken> for MyToken { /* ... */ }
///
/// // Converts the structs into Python objects.
/// impl<'py> IntoPyObject<'py> for PyMyToken { /* ... */ }
/// impl<'py> IntoPyObject<'py> for MyToken { /* ... */ }
/// ```
///
/// # Enum
///
/// `#[ast]` supports two different forms of enums: AST unions and enums with
/// only unit variants.
///
/// First, `#[ast]` can be added to enums that represent a union of multiple AST
/// types. These enums may only have tuple and unit variants, and the tuple
/// variants may have up to one field. That field must be another type annotated
/// with `#[ast]`. For example, the following enum is valid:
///
/// ```ignore
/// #[ast]
/// enum MyElement<N: TreeNode = SyntaxNode> {
///     // Both `MyNode` and `MyToken` are annotated with `#[ast]`.
///     Node(MyNode<N>),
///     Token(MyToken<N::Token>),
///     // Unit variants may be used as well.
///     Other,
/// }
/// ```
///
/// This attribute will apply the following transformations on the duplicate
/// Python enum:
///
/// - Prefix the name with “Py”
/// - Remove generics
/// - Remove all original attributes except for doc comments
/// - Add `#[pyclass]` and other necessary derive attributes
/// - Convert unit variants to tuple variants with no fields
/// - Replace contained types with their Python equivalent
/// - Implement [`From`] to allow converting from the original to the Python
///   enum and back
/// - Implement `IntoPyObject` for both the Python and the original
///   enum[^into-py-object-enums]
///
/// [^into-py-object-enums]: Technically this attribute only implements `IntoPyObject` for the original enum, as `#[pyclass]` does that automatically for the Python equivalent.
///
/// Using these transformations, the above code will approximately expand to the
/// following:
///
/// ```ignore
/// // Original enum.
/// enum MyElement<N: TreeNode = SyntaxNode> {
///     Node(MyNode<N>),
///     Token(MyToken<N::Token>),
///     Other,
/// }
///
/// // Python enum.
/// #[pyclass(
///     module = "sprocket_bio.ast.v1",
///     name = "MyElement",
///     frozen,
///     from_py_object,
///     eq,
/// )]
/// #[derive(Clone, PartialEq)]
/// enum PyMyElement {
///     Node(PyMyNode),
///     Token(PyMyToken),
///     Other(),
/// }
///
/// // Conversion between the original and Python enum.
/// impl From<MyElement> for PyMyElement { /* ... */ }
/// impl From<PyMyElement> for MyElement { /* ... */ }
///
/// // Converts the enums into Python objects.
/// impl<'py> IntoPyObject<'py> for PyMyElement { /* ... */ }
/// impl<'py> IntoPyObject<'py> for MyElement { /* ... */ }
/// ```
///
/// Second, `#[ast]` can be added to enums with only unit variants. For example,
/// the following enum is valid:
///
/// ```ignore
/// #[ast]
/// enum PrimitiveTypeKind {
///     Boolean,
///     Integer,
///     Float,
///     String,
///     File,
///     Directory,
/// }
/// ```
///
/// This attribute will apply the following transformations on the duplicate
/// Python enum:
///
/// - Prefix the name with “Py”
/// - Remove generics
/// - Remove all original attributes except for doc comments
/// - Add `#[pyclass]` and other necessary derive attributes
/// - Implement [`From`] to allow converting from the original to the Python
///   enum and back
/// - Implement `IntoPyObject` for both the Python and the original
///   enum[^into-py-object-enums]
///
/// Using these transformations, the above code will approximately expand to the
/// following:
///
/// ```ignore
/// // Original enum.
/// enum PrimitiveTypeKind {
///     Boolean,
///     Integer,
///     Float,
///     String,
///     File,
///     Directory,
/// }
///
/// // Python enum.
/// #[pyclass(
///     module = "sprocket_bio.ast.v1",
///     name = "PrimitiveTypeKind",
///     frozen,
///     from_py_object,
///     eq,
///     rename_all = "SCREAMING_SNAKE_CASE",
/// )]
/// #[derive(Clone, PartialEq)]
/// enum PyPrimitiveTypeKind {
///     Boolean,
///     Integer,
///     Float,
///     String,
///     File,
///     Directory,
/// }
///
/// // Conversion between the original and Python enum.
/// impl From<PrimitiveTypeKind> for PyPrimitiveTypeKind { /* ... */ }
/// impl From<PyPrimitiveTypeKind> for PrimitiveTypeKind { /* ... */ }
///
/// // Converts the enums into Python objects.
/// impl<'py> IntoPyObject<'py> for PyPrimitiveTypeKind { /* ... */ }
/// impl<'py> IntoPyObject<'py> for PrimitiveTypeKind { /* ... */ }
/// ```
///
/// # Arguments
///
/// Arguments can be passed to the attribute to customize its generation, such
/// as `#[ast(str)]` for example.
///
/// |Name|Default|Description|
/// |-|-|-|
/// |`module`|`"sprocket_bio.ast.v1"`|The module the AST element is defined in, from Python's perspective.|
/// |`eq`|Omitted|Implements `__eq__` for the type using its [`PartialEq`] implementation.|
/// |`str`|Omitted|Implements `__str__` for the type using its [`Display`](std::fmt::Display) implementation. Note that unlike `#[pyclass]`, format strings (such as `str = "{var:?}"`) are not supported.|
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
