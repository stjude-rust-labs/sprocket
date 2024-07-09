//! Validation of string literals in an AST.

use wdl_grammar::lexer::v1::EscapeToken;
use wdl_grammar::lexer::v1::Logos;

use crate::v1::StringText;
use crate::AstToken;
use crate::Diagnostic;
use crate::Diagnostics;
use crate::Document;
use crate::Span;
use crate::VisitReason;
use crate::Visitor;

/// Creates an "unknown escape sequence" diagnostic
fn unknown_escape_sequence(sequence: &str, span: Span) -> Diagnostic {
    Diagnostic::error(format!("unknown escape sequence `{sequence}`"))
        .with_label("this is not a valid WDL escape sequence", span)
}

/// Creates an "invalid line continuation" diagnostic
fn invalid_line_continuation(span: Span) -> Diagnostic {
    Diagnostic::error("literal strings may not contain line continuations")
        .with_label("remove this line continuation", span)
}

/// Creates an "invalid octal escape" diagnostic
fn invalid_octal_escape(span: Span) -> Diagnostic {
    Diagnostic::error("invalid octal escape sequence").with_label(
        "expected a sequence of three octal digits to follow this",
        span,
    )
}

/// Creates an "invalid hex escape" diagnostic
fn invalid_hex_escape(span: Span) -> Diagnostic {
    Diagnostic::error("invalid hex escape sequence").with_label(
        "expected a sequence of two hexadecimal digits to follow this",
        span,
    )
}

/// Creates an "invalid short unicode escape" diagnostic
fn invalid_short_unicode_escape(span: Span) -> Diagnostic {
    Diagnostic::error("invalid unicode escape sequence").with_label(
        "expected a sequence of four hexadecimal digits to follow this",
        span,
    )
}

/// Creates an "invalid unicode escape" diagnostic
fn invalid_unicode_escape(span: Span) -> Diagnostic {
    Diagnostic::error("invalid unicode escape sequence").with_label(
        "expected a sequence of eight hexadecimal digits to follow this",
        span,
    )
}

/// Creates a "must escape newline" diagnostic
fn must_escape_newline(span: Span) -> Diagnostic {
    Diagnostic::error("literal strings cannot contain newline characters")
        .with_label("escape this newline with `\\n`", span)
}

/// Creates a "must escape tab" diagnostic
fn must_escape_tab(span: Span) -> Diagnostic {
    Diagnostic::error("literal strings cannot contain tab characters")
        .with_label("escape this tab with `\\t`", span)
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
            EscapeToken::InvalidOctal => {
                diagnostics.add(invalid_octal_escape(Span::new(start + span.start, 1)))
            }
            EscapeToken::InvalidHex => diagnostics.add(invalid_hex_escape(Span::new(
                start + span.start,
                span.len(),
            ))),
            EscapeToken::InvalidShortUnicode => diagnostics.add(invalid_short_unicode_escape(
                Span::new(start + span.start, span.len()),
            )),
            EscapeToken::InvalidUnicode => diagnostics.add(invalid_unicode_escape(Span::new(
                start + span.start,
                span.len(),
            ))),
            EscapeToken::Continuation => diagnostics.add(invalid_line_continuation(Span::new(
                start + span.start,
                span.len(),
            ))),
            EscapeToken::Newline => diagnostics.add(must_escape_newline(Span::new(
                start + span.start,
                span.len(),
            ))),
            EscapeToken::Tab => {
                diagnostics.add(must_escape_tab(Span::new(start + span.start, span.len())))
            }
            EscapeToken::Unknown => diagnostics.add(unknown_escape_sequence(
                &text[span.start..span.end],
                Span::new(start + span.start, span.len()),
            )),
        }
    }
}

/// A visitor of literal text within an AST.
///
/// Ensures that string text:
///
/// * Does not contain characters that must be escaped.
/// * Does not contain invalid escape sequences.
#[derive(Default, Debug)]
pub struct LiteralTextVisitor;

impl Visitor for LiteralTextVisitor {
    type State = Diagnostics;

    fn document(&mut self, _: &mut Self::State, reason: VisitReason, _: &Document) {
        if reason == VisitReason::Exit {
            return;
        }

        // Reset the visitor upon document entry
        *self = Default::default();
    }

    fn string_text(&mut self, state: &mut Self::State, text: &StringText) {
        check_text(
            state,
            text.syntax().text_range().start().into(),
            text.as_str(),
        );
    }
}
