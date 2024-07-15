//! Lexing and parsing for Workflow Description Language (WDL) documents.
//!
//! This crate implements an infallible WDL parser based
//! on the `logos` crate for lexing and the `rowan` crate for
//! concrete syntax tree (CST) representation.
//!
//! The parser outputs a list of parser events that can be used
//! to construct the CST; the parser also keeps a list of [Diagnostic]s emitted
//! during the parse that relate to spans from the original source.
//!
//! See [SyntaxTree::parse] for parsing WDL source;
//! users may inspect the resulting CST to determine the version of the
//! document that was parsed.
//!
//! # Examples
//!
//! An example of parsing WDL source into a CST and printing the tree:
//!
//! ```rust
//! use wdl_grammar::SyntaxTree;
//!
//! let (tree, diagnostics) = SyntaxTree::parse("version 1.1");
//! assert!(diagnostics.is_empty());
//! println!("{tree:#?}");
//! ```

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::missing_docs_in_private_items)]
#![warn(rustdoc::broken_intra_doc_links)]

mod diagnostic;
pub mod grammar;
pub mod lexer;
pub mod parser;
mod tree;
pub mod version;

pub use diagnostic::*;
pub use tree::*;
pub use version::SupportedVersion;
