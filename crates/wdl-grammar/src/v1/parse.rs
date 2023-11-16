//! WDL 1.x parsing.
#![allow(missing_docs)]

use pest_derive::Parser;

/// A Pest [`pest::Parser`] for the WDL 1.x grammar.
///
/// **Note:** this [`Parser`] is not exposed directly to the user. Instead, you
/// should use the provided [`parse`] method, which performs additional
/// validation outside of the PEG grammar itself (the choice was made to do some
/// validation outside of the PEG grammar to give users better error messages in
/// some use cases).
#[derive(Debug, Parser)]
#[grammar = "v1/wdl.pest"]
pub(crate) struct Parser;
