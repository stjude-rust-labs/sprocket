//! Module for the WDL grammar functions.

use super::Diagnostic;
use super::Span;
use super::lexer::PreambleToken;
use super::parser::Event;
use super::parser::Marker;
use super::parser::Parser;
use super::tree::SyntaxKind;
use crate::SupportedVersion;
use crate::lexer::VersionStatementToken;

pub mod v1;

/// Helper macros for the parser implementation.
mod macros {
    /// A macro for expecting the next token be a particular token.
    ///
    /// Returns a diagnostic if the token is not the specified token.
    macro_rules! expected {
        ($parser:ident, $marker:ident, $token:expr) => {
            if let Err(e) = $parser.expect($token) {
                return Err(($marker, e));
            }
        };
        ($parser:ident, $marker:ident, $token:expr, $name:literal) => {
            if let Err(e) = $parser.expect_with_name($token, $name) {
                return Err(($marker, e));
            }
        };
    }

    /// A macro for expecting the next token be in the given token set.
    ///
    /// Returns an error if the token is not the specified token.
    macro_rules! expected_in {
        ($parser:ident, $marker:ident, $set:ident $(, $names:literal)+ $(,)?) => {
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
pub fn document(source: &str, mut parser: PreambleParser<'_>) -> (Vec<Event>, Vec<Diagnostic>) {
    let root = parser.start();
    // Look for a starting `version` keyword token
    // If this fails, an error is emitted and we'll skip parsing the remainder of
    // the file.
    let (mut parser, diagnostic) = match parser.peek() {
        Some((PreambleToken::VersionKeyword, _)) => {
            let marker = parser.start();
            let (mut parser, res) = version_statement(parser, marker);
            match res {
                Ok(span) => {
                    // A version statement was successfully parsed, check to see if the
                    // version is supported by this implementation
                    let version = &source[span.start()..span.end()];

                    match version.parse::<SupportedVersion>() {
                        Ok(_) => {
                            let mut parser = parser.morph();
                            v1::items(&mut parser);
                            root.complete(&mut parser, SyntaxKind::RootNode);
                            let output = parser.finish();
                            return (output.events, output.diagnostics);
                        }
                        _ => (
                            parser,
                            Diagnostic::error(format!("unsupported WDL version `{version}`"))
                                .with_label("this version of WDL is not supported", span),
                        ),
                    }
                }
                Err((marker, e)) => {
                    marker.abandon(&mut parser);
                    (parser, e)
                }
            }
        }
        found => {
            let mut diagnostic =
                Diagnostic::error("a WDL document must start with a version statement");

            if let Some((_, span)) = found {
                diagnostic =
                    diagnostic.with_label("a version statement must come before this", span);
            }

            (parser, diagnostic)
        }
    };

    // At this point, the parse cannot continue; but we still want the tree to cover
    // every span of the source, so we will insert a special "unparsed" token for
    // the remaining source.
    parser.diagnostic(diagnostic);
    parser.consume_remainder();
    root.complete(&mut parser, SyntaxKind::RootNode);
    let output = parser.finish();
    (output.events, output.diagnostics)
}

/// Parses the version statement of a WDL source file.
///
/// Returns the source span of the version token if present.
pub fn version_statement(
    mut parser: Parser<'_, PreambleToken>,
    marker: Marker,
) -> (
    Parser<'_, PreambleToken>,
    Result<Span, (Marker, Diagnostic)>,
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
