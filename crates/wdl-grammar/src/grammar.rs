//! Module for the WDL grammar functions.

use std::str::FromStr;

use super::Diagnostic;
use super::Span;
use super::SupportedVersion;
use super::lexer::PreambleToken;
use super::parser::Event;
use super::parser::Parser;
use super::tree::SyntaxKind;
use crate::lexer::VersionStatementToken;

pub mod v1;

/// Helper macros for the parser implementation.
mod macros {
    /// A macro for expecting the next token be a particular token.
    ///
    /// Returns a diagnostic if the token is not the specified token.
    macro_rules! expected {
        ($parser:ident, $marker:ident, $token:expr_2021) => {
            if let Err(e) = $parser.expect($token) {
                return Err(($marker, e));
            }
        };
        ($parser:ident, $marker:ident, $token:expr_2021, $name:literal) => {
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
pub fn document(
    mut parser: PreambleParser<'_>,
    fallback_version: Option<SupportedVersion>,
) -> (Vec<Event>, Vec<Diagnostic>) {
    let root = parser.start();
    // Look for a starting `version` keyword token
    // If this fails, an error is emitted and we'll skip parsing the remainder of
    // the file.
    let (mut parser, diagnostic) = match parser.peek() {
        Some((PreambleToken::VersionKeyword, _)) => {
            match version_statement(parser, fallback_version) {
                (parser, None) => {
                    // A version statement was successfully parsed; continue on with parsing the
                    // rest of the document.
                    let mut parser = parser.morph();
                    v1::items(&mut parser);
                    root.complete(&mut parser, SyntaxKind::RootNode);
                    let output = parser.finish();
                    return (output.events, output.diagnostics);
                }
                (parser, Some(diag)) => (parser, diag),
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

/// Creates an "unsupported version" diagnostic.
fn unsupported_version(version: &str, span: Span) -> Diagnostic {
    Diagnostic::error(format!("unsupported WDL version `{version}`"))
        .with_label("this version of WDL is not supported", span)
        .with_fix("either use a supported WDL version or configure a fallback version")
}

/// Parses the version statement of a WDL source file.
///
/// Returns a diagnostic upon failure.
fn version_statement(
    mut parser: Parser<'_, PreambleToken>,
    fallback_version: Option<SupportedVersion>,
) -> (Parser<'_, PreambleToken>, Option<Diagnostic>) {
    let marker = parser.start();
    parser.require(PreambleToken::VersionKeyword);

    let mut parser: Parser<'_, VersionStatementToken> = parser.morph();
    match parser.expect(VersionStatementToken::Version) {
        Ok(span) => match SupportedVersion::from_str(parser.source(span)) {
            Ok(version) => parser.set_version(version),
            Err(e) => {
                if let Some(fallback) = fallback_version {
                    parser.set_version(fallback);
                } else {
                    parser.diagnostic(unsupported_version(&e, span));
                }
            }
        },
        Err(e) => {
            marker.abandon(&mut parser);
            return (parser.morph(), Some(e));
        }
    }

    marker.complete(&mut parser, SyntaxKind::VersionStatementNode);
    (parser.morph(), None)
}
