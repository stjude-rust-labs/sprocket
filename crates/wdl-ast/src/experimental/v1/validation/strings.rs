//! Validation of string literals in a V1 AST.

use miette::Diagnostic;
use miette::SourceSpan;
use wdl_grammar::experimental::lexer::v1::EscapeToken;
use wdl_grammar::experimental::lexer::v1::Logos;

use crate::experimental::v1::StringText;
use crate::experimental::v1::Visitor;
use crate::experimental::AstToken;
use crate::experimental::Diagnostics;
use crate::experimental::VisitReason;

/// Represents a string validation error.
#[derive(thiserror::Error, Diagnostic, Debug, Clone, PartialEq, Eq)]
enum Error {
    /// An unknown escape sequence was encountered.
    #[error("unknown escape sequence `{sequence}`")]
    UnknownEscapeSequence {
        /// The invalid escape sequence.
        sequence: String,
        /// The span of the unknown escape sequence token.
        #[label(primary, "this is an unknown escape sequence")]
        span: SourceSpan,
    },
    /// An invalid line continuation was encountered.
    #[error("literal strings may not contain line continuations")]
    InvalidLineContinuation {
        /// The span of the invalid line continuation.
        #[label(primary, "remove this line continuation")]
        span: SourceSpan,
    },
    /// An invalid octal escape sequence was encountered.
    #[error("invalid octal escape sequence")]
    InvalidOctalEscape {
        /// The span of the invalid escape sequence.
        #[label(primary, "expected a sequence of three octal digits to follow this")]
        span: SourceSpan,
    },
    /// An invalid hex escape sequence was encountered.
    #[error("invalid hex escape sequence")]
    InvalidHexEscape {
        /// The span of the invalid escape sequence.
        #[label(
            primary,
            "expected a sequence of two hexadecimal digits to follow this"
        )]
        span: SourceSpan,
    },
    /// An invalid short unicode escape sequence was encountered.
    #[error("invalid unicode escape sequence")]
    InvalidShortUnicodeEscape {
        /// The span of the invalid escape sequence.
        #[label(
            primary,
            "expected a sequence of four hexadecimal digits to follow this"
        )]
        span: SourceSpan,
    },
    /// An invalid unicode escape sequence was encountered.
    #[error("invalid unicode escape sequence")]
    InvalidUnicodeEscape {
        /// The span of the invalid escape sequence.
        #[label(
            primary,
            "expected a sequence of eight hexadecimal digits to follow this"
        )]
        span: SourceSpan,
    },
    /// An unescaped newline was encountered.
    #[error("literal strings cannot contain newline characters")]
    MustEscapeNewline {
        /// The span of the unescaped newline.
        #[label(primary, "escape this newline with `\\n`")]
        span: SourceSpan,
    },
    /// An unescaped tab was encountered.
    #[error("literal strings cannot contain tab characters")]
    MustEscapeTab {
        /// The span of the unescaped tab.
        #[label(primary, "escape this tab with `\\t`")]
        span: SourceSpan,
    },
}

/// Used to check literal text in a string.
fn check_text(diagnostics: &mut Diagnostics, start: usize, text: &str) {
    let lexer = EscapeToken::lexer(text).spanned();
    for (token, span) in lexer {
        match token.expect("should lex") {
            EscapeToken::Valid
            | EscapeToken::ValidOctal
            | EscapeToken::ValidHex
            | EscapeToken::ValidUnicode
            | EscapeToken::Text => continue,
            EscapeToken::InvalidOctal => diagnostics.add(Error::InvalidOctalEscape {
                span: SourceSpan::new((start + span.start).into(), 1),
            }),
            EscapeToken::InvalidHex => diagnostics.add(Error::InvalidHexEscape {
                span: SourceSpan::new((start + span.start).into(), span.len()),
            }),
            EscapeToken::InvalidShortUnicode => diagnostics.add(Error::InvalidShortUnicodeEscape {
                span: SourceSpan::new((start + span.start).into(), span.len()),
            }),
            EscapeToken::InvalidUnicode => diagnostics.add(Error::InvalidUnicodeEscape {
                span: SourceSpan::new((start + span.start).into(), span.len()),
            }),
            EscapeToken::Continuation => {
                diagnostics.add(Error::InvalidLineContinuation {
                    span: SourceSpan::new((start + span.start).into(), span.len()),
                });
            }
            EscapeToken::Newline => {
                diagnostics.add(Error::MustEscapeNewline {
                    span: SourceSpan::new((start + span.start).into(), span.len()),
                });
            }
            EscapeToken::Tab => {
                diagnostics.add(Error::MustEscapeTab {
                    span: SourceSpan::new((start + span.start).into(), span.len()),
                });
            }
            EscapeToken::Unknown => diagnostics.add(Error::UnknownEscapeSequence {
                sequence: text[span.start..span.end].to_string(),
                span: SourceSpan::new((start + span.start).into(), span.len()),
            }),
        }
    }
}

/// A visitor of literal text within an AST.
///
/// Ensures that string text:
///
/// * Does not contain characters that must be escaped.
/// * Does not contain invalid escape sequences.
#[derive(Debug)]
pub struct LiteralTextVisitor;

impl Visitor for LiteralTextVisitor {
    type State = Diagnostics;

    fn string_text(&mut self, state: &mut Self::State, reason: VisitReason, text: &StringText) {
        if reason == VisitReason::Exit {
            return;
        }

        check_text(
            state,
            text.syntax().text_range().start().into(),
            text.as_str(),
        );
    }
}
