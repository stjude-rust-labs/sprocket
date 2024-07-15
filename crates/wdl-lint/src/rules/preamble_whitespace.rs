//! A lint rule that checks for an incorrect preamble whitespace.

use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::ToSpan;
use wdl_ast::VersionStatement;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;
use wdl_ast::Whitespace;

use crate::util::lines_with_offset;
use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the preamble whitespace rule.
const ID: &str = "PreambleWhitespace";

/// Creates an "unnecessary whitespace" diagnostic.
fn unnecessary_whitespace(span: Span) -> Diagnostic {
    Diagnostic::note("unnecessary whitespace in document preamble")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("remove the unnecessary whitespace")
}

/// Creates an "expected a blank line before" diagnostic.
fn expected_blank_line_before(span: Span) -> Diagnostic {
    Diagnostic::note("expected a blank line before the version statement")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("add a blank line between the last preamble comment and the version statement")
}

/// Creates an "expected a blank line after" diagnostic.
fn expected_blank_line_after(span: Span) -> Diagnostic {
    Diagnostic::note("expected a blank line after the version statement")
        .with_rule(ID)
        .with_label("add a blank line before this", span)
        .with_fix("add a blank line immediately after the version statement")
}

/// Detects incorrect whitespace in a document preamble.
#[derive(Default, Debug, Clone, Copy)]
pub struct PreambleWhitespaceRule {
    /// Whether or not we've entered the version statement.
    entered_version: bool,
    /// Whether or not we've exited the version statement.
    exited_version: bool,
    /// Whether or not we've visited whitespace *after* the version statement.
    checked_blank_after: bool,
}

impl Rule for PreambleWhitespaceRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that documents have correct whitespace in the preamble."
    }

    fn explanation(&self) -> &'static str {
        "The document preamble is defined as anything before the version declaration statement and \
         the version declaration statement itself. Only comments and whitespace are permitted \
         before the version declaration. If there are no comments, the version declaration must be \
         the first line of the document. If there are comments, there must be exactly one blank \
         line between the last comment and the version declaration. No extraneous whitespace is \
         allowed. A blank line must come after the preamble."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Spacing, Tag::Style])
    }
}

impl Visitor for PreambleWhitespaceRule {
    type State = Diagnostics;

    fn document(
        &mut self,
        _: &mut Self::State,
        reason: VisitReason,
        _: &Document,
        _: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Reset the visitor upon document entry
        *self = Default::default();
    }

    fn version_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &VersionStatement,
    ) {
        if reason == VisitReason::Exit {
            self.exited_version = true;
            return;
        }

        // We're finished after the version statement
        self.entered_version = true;

        // If the previous token is whitespace and its previous token is a comment,
        // then the whitespace should just be two lines
        match stmt
            .syntax()
            .prev_sibling_or_token()
            .and_then(SyntaxElement::into_token)
        {
            Some(whitespace) if whitespace.kind() == SyntaxKind::Whitespace => {
                let range = whitespace.text_range();
                if whitespace
                    .prev_sibling_or_token()
                    .map(|s| s.kind() == SyntaxKind::Comment)
                    .unwrap_or(false)
                {
                    // There's a preceding comment, so it is expected that the whitespace is only
                    // two newlines in a row
                    let text = whitespace.text();
                    let mut count = 0;
                    for (line, start, next_start) in lines_with_offset(text) {
                        // If this is not a blank line, it's unnecessary whitespace
                        if !line.is_empty() {
                            state.add(unnecessary_whitespace(Span::new(
                                usize::from(range.start()) + start,
                                line.len(),
                            )));
                        }

                        count += 1;
                        if count == 2 {
                            // If the previous byte isn't a newline, then we are missing a blank
                            // line
                            if text.as_bytes()[next_start - 1] != b'\n' {
                                state.add(expected_blank_line_before(Span::new(
                                    usize::from(range.start()) + start,
                                    1,
                                )));
                            }

                            // We're at the second line, the remainder is unnecessary whitespace
                            if next_start < text.len() {
                                state.add(unnecessary_whitespace(Span::new(
                                    usize::from(range.start()) + next_start,
                                    text.len() - next_start,
                                )));
                            }

                            break;
                        }
                    }

                    // We expected two lines
                    if count < 2 {
                        state.add(expected_blank_line_before(range.to_span()));
                    }
                } else {
                    // Whitespace without a comment before it
                    state.add(unnecessary_whitespace(range.to_span()));
                }
            }
            _ => {
                // Previous token is not whitespace
            }
        }
    }

    fn whitespace(&mut self, state: &mut Self::State, whitespace: &Whitespace) {
        if self.exited_version {
            // Check to see if we've already checked for a blank line after the version
            // statement
            if self.checked_blank_after {
                return;
            }

            self.checked_blank_after = true;

            let mut count = 0;
            let text = whitespace.as_str();
            let span = whitespace.span();
            for (_, _, next_start) in lines_with_offset(text) {
                count += 1;
                if count == 2 {
                    // If the previous byte isn't a newline, then we are missing a blank
                    // line after the version statement
                    if text.as_bytes()[next_start - 1] != b'\n' {
                        state.add(expected_blank_line_after(Span::new(
                            span.start() + next_start,
                            1,
                        )));
                    }

                    break;
                }
            }

            // We expected two lines or one if the whitespace is the last in the file
            if count == 0
                || (count == 1
                    && (whitespace.syntax().parent().unwrap().kind() != SyntaxKind::RootNode
                        || whitespace.syntax().next_sibling_or_token().is_some()))
            {
                state.add(expected_blank_line_after(Span::new(
                    span.start() + span.len(),
                    1,
                )));
            }

            return;
        }

        // Ignore whitespace inside the version statement itself
        if self.entered_version {
            return;
        }

        // If the next sibling is the version statement, let the `version_statement`
        // callback handle this particular whitespace
        if whitespace
            .syntax()
            .next_sibling_or_token()
            .map(|s| s.kind() == SyntaxKind::VersionStatementNode)
            .unwrap_or(false)
        {
            return;
        }

        // Otherwise, the whitespace should have a comment token before it
        if whitespace
            .syntax()
            .prev_sibling_or_token()
            .map(|s| s.kind() == SyntaxKind::Comment)
            .unwrap_or(false)
        {
            // Previous sibling is a comment, check that this is a single newline
            let s = whitespace.as_str();
            if s == "\r\n" || s == "\n" {
                // Whitespace token is valid
                return;
            }

            // Don't include the newline separating the previous comment from the whitespace
            let offset = if s.starts_with("\r\n") {
                2
            } else if s.starts_with('\n') {
                1
            } else {
                0
            };

            let span = whitespace.span();
            state.add(unnecessary_whitespace(Span::new(
                span.start() + offset,
                span.len() - offset,
            )));
            return;
        }

        // At this point, the whitespace is entirely unnecessary
        state.add(unnecessary_whitespace(whitespace.span()));
    }
}
