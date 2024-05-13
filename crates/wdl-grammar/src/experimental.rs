//! Module for the experimental parser implementation.
//!
//! The new implementation is an infallible parser based
//! on the `logos` crate for lexing and the `rowan` crate for
//! Concrete Syntax Tree (CST) representation.
//!
//! The parser will output a list of parser events that can be used
//! to construct the CST. An event may be an "error" variant that
//! signifies an error encountered during the parse; errors will
//! be collated during tree construction so that the final output
//! will be a CST and a list of errors. The errors will be based on
//! `miette` diagnostics and contain all relevant spans from the
//! original input.
//!
//! Additionally, parsing may be done in a version-agnostic way via
//! convenience functions that will handle the parse based on what version
//! was detected in the WDL source file; an unsupported version will
//! result in an error. Users may inspect the resulting CST to determine
//! the version of the document. Users may also configure the parse to
//! only allow specific WDL versions.
//!
//! When it is ready, the `experimental` module will be removed and this
//! implementation will replace the existing `pest`-based parser; all
//! existing rules will be updated to use the new CST/AST representation
//! at that time.

pub mod lexer;
