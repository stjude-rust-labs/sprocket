//! A crate for lexing and parsing the Workflow Description Language
//! (WDL) using [`pest`](https://pest.rs).

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::missing_docs_in_private_items)]
#![warn(rustdoc::broken_intra_doc_links)]

#[cfg(feature = "experimental")]
pub mod experimental;
pub mod v1;

/// An unrecoverable error.
///
/// **Note:** this is not a parse error, lint warning, or validation failure
/// (which are expected and recoverable). Instead, this struct represents an
/// unrecoverable error that occurred.
#[derive(Debug)]
pub enum Error {
    /// A WDL 1.x error.
    V1(v1::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::V1(err) => write!(f, "{err}"),
        }
    }
}

/// A [`Result`](std::result::Result) with an [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
