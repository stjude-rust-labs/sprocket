//! Module for the experimental parser implementation.
//!
//! The new implementation is an infallible parser based
//! on the `logos` crate for lexing and the `rowan` crate for
//! concrete syntax tree (CST) representation.
//!
//! The parser will output a list of parser events that can be used
//! to construct the CST. The parser also keeps a list of [Diagnostic] emitted
//! during the parse that relate to spans from the original source.
//!
//! See [SyntaxTree::parse] for parsing WDL source;
//! users may inspect the resulting CST to determine the version of the
//! document that was parsed.
//!
//! When it is ready, the `experimental` module will be removed and this
//! implementation will replace the existing `pest`-based parser; all
//! existing rules will be updated to use the new CST/AST representation
//! at that time.

mod diagnostic;
pub mod grammar;
pub mod lexer;
pub mod parser;
mod tree;

pub use diagnostic::*;
pub use tree::*;
