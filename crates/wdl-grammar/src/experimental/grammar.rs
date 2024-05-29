//! Module for the WDL grammar functions.

use miette::SourceSpan;

use super::lexer::PreambleToken;
use super::parser::Error;
use super::parser::Event;
use super::parser::Marker;
use super::parser::Parser;
use super::tree::SyntaxKind;
use crate::experimental::lexer::VersionStatementToken;

pub mod v1;

mod macros {
    /// A macro for expecting the next token be a particular token.
    ///
    /// Returns an error if the token is not the specified token.
    macro_rules! expected {
        ($parser:ident, $marker:ident, $token:expr) => {
            if let Err(e) = $parser.expect($token) {
                return Err(($marker, e));
            }
        };
    }

    /// A macro for expecting the next token be in the given token set.
    ///
    /// Returns an error if the token is not the specified token.
    macro_rules! expected_in {
        ($parser:ident, $marker:ident, $set:ident $(, $names:literal)+) => {
            if let Err(e) = $parser.expect_in($set, &[$($names),+]) {
                return Err(($marker, e));
            }
        };
    }

    /// A macro for expecting that a given function parses the next node.
    ///
    /// Returns an error if the given function returns an error.
    macro_rules! expected_fn {
        ($parser:ident, $marker:ident, $func:ident) => {
            let inner = $parser.start();
            if let Err((inner, e)) = $func($parser, inner) {
                inner.abandon($parser);
                return Err(($marker, e));
            }
        };
        ($parser:ident, $func:ident) => {
            let inner = $parser.start();
            if let Err((inner, e)) = $func($parser, inner) {
                inner.abandon($parser);
                return Err(e);
            }
        };
    }

    pub(crate) use expected;
    pub(crate) use expected_fn;
    pub(crate) use expected_in;
}

/// A parser type used for parsing the document preamble.
type PreambleParser<'a> = Parser<'a, PreambleToken>;

/// Parses a WDL document.
///
/// Returns the parser events that result from parsing the document.
pub fn document(source: &str, mut parser: PreambleParser<'_>) -> (Vec<Event>, Vec<Error>) {
    let root = parser.start();
    // Look for a starting `version` keyword token
    // If this fails, an error is emitted and we'll fallback to the latest version
    // of the grammar to parse the remainder of the source.
    let (mut parser, err) = match parser.peek() {
        Some((PreambleToken::VersionKeyword, _)) => {
            let marker = parser.start();
            let (mut parser, res) = version_statement(parser, marker);
            match res {
                Ok(span) => {
                    // A version statement was successfully parsed, check to see if the
                    // version is supported by this implementation
                    let version: &str = &source[span.offset()..span.offset() + span.len()];
                    match version {
                        "1.0" | "1.1" => {
                            let mut parser = parser.morph();
                            v1::items(&mut parser);
                            root.complete(&mut parser, SyntaxKind::RootNode);
                            let output = parser.finish();
                            return (output.events, output.errors);
                        }
                        _ => (
                            parser,
                            Error::UnsupportedVersion {
                                version: version.to_string(),
                                span,
                            },
                        ),
                    }
                }
                Err((marker, e)) => {
                    marker.abandon(&mut parser);
                    (parser, e)
                }
            }
        }
        found => (
            parser,
            Error::VersionRequired {
                span: found.map(|(_, s)| s),
            },
        ),
    };

    // Fallback to parsing with the latest supported version
    // This will attempt to parse as much as possible, despite maybe not being
    // correct for what's in the document
    parser.error(err);
    let mut parser = parser.morph();
    v1::items(&mut parser);
    root.complete(&mut parser, SyntaxKind::RootNode);
    let output = parser.finish();
    (output.events, output.errors)
}

/// Parses the version statement of a WDL source file.
///
/// Returns the source span of the version token if present.
pub fn version_statement(
    mut parser: Parser<'_, PreambleToken>,
    marker: Marker,
) -> (
    Parser<'_, PreambleToken>,
    Result<SourceSpan, (Marker, Error)>,
) {
    parser.require(PreambleToken::VersionKeyword);

    let mut parser: Parser<'_, VersionStatementToken> = parser.morph();
    let span = match parser.expect(VersionStatementToken::Version) {
        Ok(span) => span,
        Err(e) => return (parser.morph(), Err((marker, e))),
    };

    marker.complete(&mut parser, SyntaxKind::VersionStatementNode);
    (parser.morph(), Ok(span))
}
