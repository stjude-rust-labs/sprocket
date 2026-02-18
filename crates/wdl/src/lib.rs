//! Workflow Description Language (WDL) document parsing and linting.
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
#[cfg(feature = "diagnostics")]
#[doc(inline)]
pub use wdl_diagnostics as diagnostics;
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
