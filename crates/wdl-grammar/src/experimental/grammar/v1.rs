//! Module for the V1 grammar functions.

use crate::experimental::lexer;
use crate::experimental::parser;

/// The parser type for the V1 grammar.
pub type Parser<'a> = parser::Parser<'a, lexer::v1::Token>;

/// Parses the top-level items of a V1 document.
///
/// It is expected that the version statement has already been parsed.
pub fn items(_parser: &mut Parser<'_>) {
    // TODO: parse the top-level items
}
