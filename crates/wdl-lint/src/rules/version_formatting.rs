//! A lint rule that checks the formatting of the version statement.

use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::ToSpan;
use wdl_ast::VersionStatement;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;
use wdl_ast::Whitespace;

use crate::Rule;
use crate::Tag;
use crate::TagSet;
use crate::util::lines_with_offset;

/// The ID of the rule.
const ID: &str = "VersionFormatting";

/// Creates a diagnostic for an expected blank line before the version
/// statement.
fn expected_blank_line_before_version(span: Span) -> Diagnostic {
    Diagnostic::note(
        "expected exactly one blank line between the last comment and the version statement",
    )
    .with_rule(ID)
    .with_highlight(span)
}

/// Creates a diagnostic for an expected blank line after the version statement.
fn expected_blank_line_after_version(span: Span) -> Diagnostic {
    Diagnostic::note("expected exactly one blank line after the version statement")
        .with_rule(ID)
        .with_highlight(span)
}

/// Creates a diagnostic for unexpected whitespace before the version statement.
fn whitespace_before_version(span: Span) -> Diagnostic {
    Diagnostic::note("unexpected whitespace before the version statement")
        .with_rule(ID)
        .with_highlight(span)
}

/// Creates a diagnostic for a comment inside the version statement.
fn comment_inside_version(span: Span) -> Diagnostic {
    Diagnostic::note("unexpected comment inside the version statement")
        .with_rule(ID)
        .with_highlight(span)
}

/// Creates a diagnostic for unexpected whitespace inside the version statement.
fn unexpected_whitespace_inside_version(span: Span) -> Diagnostic {
    Diagnostic::note("expected exactly one space between 'version' and the version number")
        .with_rule(ID)
        .with_highlight(span)
}

/// Detects incorrect formatting of the version statement.
#[derive(Default, Debug, Clone, Copy)]
pub struct VersionFormattingRule;

impl Rule for VersionFormattingRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Checks the formatting of the version statement."
    }

    fn explanation(&self) -> &'static str {
        "The version statement should be formatted correctly. This rule checks that the version \
         statement is followed by a blank line and that there is exactly one space between \
         'version' and the version number. It also checks that if there are comments before the \
         version statement, they are separated by exactly one blank line. If there are no \
         comments, there should be no whitespace before the version statement."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Style])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[SyntaxKind::VersionStatementNode])
    }
}

impl Visitor for VersionFormattingRule {
    type State = Diagnostics;

    fn document(&mut self, _: &mut Self::State, _: VisitReason, _: &Document, _: SupportedVersion) {
        // This is intentionally empty, as this rule has no state.
    }

    fn version_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &VersionStatement,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // 1. Handle whitespace before the version statement
        // If there's a previous sibling or token, it must be whitespace
        // because only comments and whitespace may precede the version statement
        // and whitespace must come between the last comment and the version statement.
        if let Some(prev_ws) = stmt.syntax().prev_sibling_or_token() {
            let ws = prev_ws.as_token().expect("expected a token").text();
            // If there's a previous sibling or token, it must be a comment
            if let Some(_prev_comment) = prev_ws.prev_sibling_or_token() {
                if ws != "\n\n" && ws != "\r\n\r\n" {
                    // There's a special case where the blank line has extra whitespace
                    // but that doesn't appear in the printed diagnostic.
                    let mut diagnostic =
                        expected_blank_line_before_version(prev_ws.text_range().to_span());

                    if ws.chars().filter(|&c| c == '\n').count() == 2 {
                        for (line, start, end) in lines_with_offset(ws) {
                            if !line.is_empty() {
                                let end_offset = if ws.ends_with('\n') {
                                    1
                                } else if ws.ends_with("\r\n") {
                                    2
                                } else {
                                    0
                                };

                                diagnostic = diagnostic.with_highlight(Span::new(
                                    prev_ws.text_range().to_span().start() + start,
                                    end - start - end_offset,
                                ));
                            }
                        }
                    }
                    state.add(diagnostic);
                }
            } else {
                state.add(whitespace_before_version(prev_ws.text_range().to_span()));
            }
        }

        // 2. Handle internal whitespace and comments
        for child in stmt
            .syntax()
            .children_with_tokens()
            .filter(|c| c.kind() == SyntaxKind::Whitespace || c.kind() == SyntaxKind::Comment)
        {
            match child.kind() {
                SyntaxKind::Whitespace => {
                    if child.as_token().expect("expected a token").text() != " " {
                        state.add(unexpected_whitespace_inside_version(
                            child.text_range().to_span(),
                        ));
                    }
                }
                SyntaxKind::Comment => {
                    state.add(comment_inside_version(child.text_range().to_span()));
                }
                _ => unreachable!(),
            }
        }

        // 3. Handle whitespace after the version statement
        if let Some(next) = stmt.syntax().next_sibling_or_token() {
            if let Some(ws) = next.as_token().and_then(|s| Whitespace::cast(s.clone())) {
                let s = ws.as_str();
                // Don't add diagnostic if there's nothing but whitespace after the version
                // statement
                if s != "\n\n" && s != "\r\n\r\n" && next.next_sibling_or_token().is_some() {
                    state.add(expected_blank_line_after_version(ws.span()));
                }
            }
        } // else version is the last item in the document
    }
}
