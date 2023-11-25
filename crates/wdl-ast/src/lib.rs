//! An abstract syntax tree for Workflow Description Language documents.

#![feature(decl_macro)]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::missing_docs_in_private_items)]
#![warn(rustdoc::broken_intra_doc_links)]

pub mod v1;

/// An error related to an abstract syntax tree (AST).
#[derive(Debug)]
pub enum Error {
    /// An error occurred while linting a parse tree.
    ///
    /// **Note:** this is not a lint _warning_! A lint error is an unrecoverable
    /// error that occurs during the process of linting.
    Lint(Box<dyn std::error::Error>),

    /// An error occurred while parsing a WDL v1.x abstract syntax tree.
    ParseV1(Box<v1::Error>),

    /// An error occurred while validating an abstract syntax tree.
    Validation(Box<wdl_core::concern::validation::Failure>),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Lint(err) => {
                write!(f, "lint error: {err}")
            }
            Error::ParseV1(err) => write!(f, "parse error:\n\n{err}"),
            Error::Validation(err) => write!(f, "validation error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
