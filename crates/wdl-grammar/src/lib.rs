//! A crate for lexing and parsing the Workflow Description Language
//! (WDL) using [`pest`](https://pest.rs).

#![feature(let_chains)]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::missing_docs_in_private_items)]
#![warn(rustdoc::broken_intra_doc_links)]

use pest::RuleType;

pub mod core;
pub mod v1;
mod version;

pub use version::Version;

/// An error that can occur when parsing.
///
/// **Note:** the contents of these errors are all boxed because the have
/// relatively large struct sizes (and, thus, are unwieldy to pass around on the
/// stack). As such, they boxed so that only a pointer to the heap is stored.
#[derive(Debug)]
pub enum Error<R: RuleType> {
    /// An error occurred while linting a parse tree.
    Lint(Box<dyn std::error::Error>),

    /// An error occurred while Pest was parsing the parse tree.
    Parse(Box<pest::error::Error<R>>),

    /// An error occurred while validating a parse tree.
    Validation(Box<core::validation::Error>),
}

impl<R: RuleType> std::fmt::Display for Error<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Lint(err) => {
                write!(f, "lint error: {err}")
            }
            Error::Parse(err) => write!(f, "parse error:\n\n{err}"),
            Error::Validation(err) => write!(f, "validation error: {err}"),
        }
    }
}

impl<R: RuleType> std::error::Error for Error<R> {}

/// A [`Result`](std::result::Result) with an [`Error`].
pub type Result<T, R> = std::result::Result<T, Error<R>>;
