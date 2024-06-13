//! A lint rule that checks for an incorrect preamble whitespace.

use wdl_ast::v1::Visitor;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::ToSpan;
use wdl_ast::VersionStatement;
use wdl_ast::VisitReason;
use wdl_ast::Whitespace;

use super::Rule;
use crate::util::lines_with_offset;
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

/// Creates an "expected a blank line" diagnostic.
fn expected_blank_line(span: Span) -> Diagnostic {
    Diagnostic::note("expected a blank line before the version statement")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("add a blank line between the last preamble comment and the version statement")
}

/// Detects incorrect whitespace in a document preamble.
#[derive(Debug, Clone, Copy)]
pub struct PreambleWhitespaceRule;

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
         allowed."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Spacing, Tag::Style])
    }

    fn visitor(&self) -> Box<dyn Visitor<State = Diagnostics>> {
        Box::<PreambleWhitespaceVisitor>::default()
    }
}

/// Implements the visitor for the preamble whitespace rule.
#[derive(Default, Debug)]
struct PreambleWhitespaceVisitor {
    /// Whether or not the rule has finished.
    finished: bool,
}

impl Visitor for PreambleWhitespaceVisitor {
    type State = Diagnostics;

    fn version_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &VersionStatement,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // We're finished after the version statement
        self.finished = true;

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
                                state.add(expected_blank_line(Span::new(
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
                        state.add(expected_blank_line(range.to_span()));
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
        if self.finished {
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
