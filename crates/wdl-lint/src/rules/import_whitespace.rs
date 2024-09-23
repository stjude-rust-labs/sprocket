//! A lint rule for preventing whitespace between imports.

use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::ToSpan;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;
use wdl_ast::v1::ImportStatement;

use crate::Rule;
use crate::Tag;
use crate::TagSet;
use crate::util::lines_with_offset;

/// The identifier for the import whitespace rule.
const ID: &str = "ImportWhitespace";

/// Creates a diagnostic for when there is a blank between
/// imports.
fn blank_between_imports(span: Span) -> Diagnostic {
    Diagnostic::note("blank lines are not allowed between imports")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("remove any blank lines between imports")
}

/// Creates a diagnostic for where there is improper
/// whitespace within an import statement.
fn improper_whitespace_within_import(span: Span) -> Diagnostic {
    Diagnostic::note("improper whitespace in import statement")
        .with_rule(ID)
        .with_label("this should be a singular space (` `)", span)
        .with_fix("use minimal whitespace within import statements")
}

/// Creates a diagnostic for where there is improper
/// whitespace before an import statement.
fn improper_whitespace_before_import(span: Span) -> Diagnostic {
    Diagnostic::note("improper whitespace before import statement")
        .with_rule(ID)
        .with_label("extraneous whitespace should not be there", span)
        .with_fix("use minimal whitespace before import statements")
}

/// Detects whitespace between imports.
#[derive(Default, Debug, Clone, Copy)]
pub struct ImportWhitespaceRule;

impl Rule for ImportWhitespaceRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that there is no extraneous whitespace between or within imports."
    }

    fn explanation(&self) -> &'static str {
        "Imports should be grouped together without any whitespace between them. Each import \
         statement should be contained to one line. No whitespace should come before the start of \
         each import statement. One literal space should be between the `import` keyword and the \
         import path. If the `alias` or `as` keywords are present, they should be separated from \
         the previous and next words by exactly one space. If separation between imports is \
         needed, it should be done with one or more comments labelling groups of imports. \
         Extraneous whitespace between and within imports makes code harder to parse and \
         understand."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Style, Tag::Clarity, Tag::Spacing])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::ImportStatementNode,
        ])
    }
}

impl Visitor for ImportWhitespaceRule {
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

    fn import_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &ImportStatement,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // First, check internal whitespace.
        let internal_whitespace = stmt
            .syntax()
            .children_with_tokens()
            .filter(|c| c.kind() == SyntaxKind::Whitespace)
            .map(|c| c.into_token().unwrap());

        for token in internal_whitespace {
            if token.text() != " " && token.prev_token().unwrap().kind() != SyntaxKind::Comment {
                state.exceptable_add(
                    improper_whitespace_within_import(token.text_range().to_span()),
                    SyntaxElement::from(token),
                    &self.exceptable_nodes(),
                );
            }
        }

        // Second, check for whitespace before the import.
        let prev_token = stmt
            .syntax()
            .prev_sibling_or_token()
            .and_then(SyntaxElement::into_token);

        if let Some(token) = prev_token {
            if token.kind() == SyntaxKind::Whitespace && !token.text().ends_with('\n') {
                // Find the span of just the leading whitespace
                let span = token.text_range().to_span();
                for (text, offset, _) in lines_with_offset(token.text()) {
                    if !text.is_empty() {
                        state.exceptable_add(
                            improper_whitespace_before_import(Span::new(
                                span.start() + offset,
                                span.len() - offset,
                            )),
                            SyntaxElement::from(token.clone()),
                            &self.exceptable_nodes(),
                        );
                    }
                }
            }
        }

        // Third, check for whitespace between imports.
        let between_imports = stmt
            .syntax()
            .prev_sibling()
            .map(|s| s.kind() == SyntaxKind::ImportStatementNode)
            .unwrap_or(false);
        if !between_imports {
            // Another rule will catch any whitespace here.
            return;
        }

        let mut prev_token = stmt
            .syntax()
            .prev_sibling_or_token()
            .and_then(SyntaxElement::into_token);

        while let Some(token) = prev_token {
            if token.kind() == SyntaxKind::Whitespace {
                let mut should_warn = false;
                let mut second_line_start = None;
                for (i, (text, _, next)) in lines_with_offset(token.text()).enumerate() {
                    if i == 0 {
                        second_line_start = Some(next);
                    } else if i == 1 && text.is_empty() {
                        should_warn = true;
                    } else if i == 2 {
                        should_warn = false;
                        break;
                    }
                }

                if should_warn {
                    let span = token.text_range().to_span();
                    state.exceptable_add(
                        blank_between_imports(Span::new(
                            span.start()
                                + second_line_start.expect("should have a second line start"),
                            span.len()
                                - second_line_start.expect("should have a second line start"),
                        )),
                        SyntaxElement::from(token.clone()),
                        &self.exceptable_nodes(),
                    );
                }
            } else if token.kind() != SyntaxKind::Comment {
                // We've backed into non-trivia, so we're done.
                break;
            }
            prev_token = token.prev_token();
        }
    }
}
