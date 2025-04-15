//! Workflow Description Language (WDL) document parsing and linting.
//!
//! There are three top-level modules to this crate:
//!
//! * `grammar` - used to parse WDL source into a Concrete Syntax Tree (CST).
//! * `ast` - used to parse a WDL document into an Abstract Syntax Tree (AST).
//! * `lint` - provides additional lint rules that can be used in a validation
//!   pass over a document.
//!
//! The above are re-exports of the individual `wdl-grammar`, `wdl-ast`, and
//! `wdl-lint` crates, respectively.
//!
//! The CST is based on the `rowan` crate and represents an immutable red-green
//! tree. Mutations to the tree require creating a new tree where unaffected
//! nodes are shared between the old and new trees; the cost of editing a node
//! of the tree depends solely on the depth of the node, as it must update the
//! parent chain to produce a new tree root.
//!
//! Note that in this implementation, the AST is a facade over the CST; each AST
//! representation internally holds a CST node or token. As a result, the AST is
//! very cheaply constructed and may be cheaply cloned at any level.
//!
//! # Examples
//!
//! An example of parsing WDL source into a CST and printing the tree:
//!
//! ```rust
//! use wdl::grammar::SyntaxTree;
//!
//! let (tree, diagnostics) = SyntaxTree::parse("version 1.1");
//! assert!(diagnostics.is_empty());
//! println!("{tree:#?}");
//! ```
//!
//! An example of parsing a WDL document into an AST:
//!
//! ```rust
//! # let source = "version 1.1\nworkflow test {}";
//! use wdl::ast::Document;
//!
//! let (document, diagnostics) = Document::parse(source);
//! if !diagnostics.is_empty() {
//!     // Handle the failure to parse
//! }
//! ```

#![warn(missing_docs)]

#[cfg(feature = "analysis")]
#[doc(inline)]
pub use wdl_analysis as analysis;
#[cfg(feature = "ast")]
#[doc(inline)]
pub use wdl_ast as ast;
#[cfg(feature = "cli")]
#[doc(inline)]
pub use wdl_cli as cli;
#[cfg(feature = "doc")]
#[doc(inline)]
pub use wdl_doc as doc;
#[cfg(feature = "engine")]
#[doc(inline)]
pub use wdl_engine as engine;
#[cfg(feature = "format")]
#[doc(inline)]
pub use wdl_format as format;
#[cfg(feature = "grammar")]
#[doc(inline)]
pub use wdl_grammar as grammar;
#[cfg(feature = "lint")]
#[doc(inline)]
pub use wdl_lint as lint;
#[cfg(feature = "lsp")]
#[doc(inline)]
pub use wdl_lsp as lsp;
