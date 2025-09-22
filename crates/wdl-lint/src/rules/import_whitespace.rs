//! A lint rule for preventing whitespace between imports.

use wdl_analysis::Diagnostics;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
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
        .with_fix("remove blank lines between imports")
}

/// Creates a diagnostic for where there is improper
/// whitespace within an import statement.
fn improper_whitespace_within_import(span: Span) -> Diagnostic {
    Diagnostic::note("improper whitespace in import statement")
        .with_rule(ID)
        .with_label("this should be a singular space (` `)", span)
        .with_fix("replace the extraneous whitespace with a single space")
}

/// Creates a diagnostic for where there is improper
/// whitespace before an import statement.
fn improper_whitespace_before_import(span: Span) -> Diagnostic {
    Diagnostic::note("improper whitespace before import statement")
        .with_rule(ID)
        .with_label("extraneous whitespace should not be here", span)
        .with_fix("remove the extraneous whitespace")
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
        TagSet::new(&[Tag::Style, Tag::Spacing])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::ImportStatementNode,
        ])
    }

    fn related_rules(&self) -> &[&'static str] {
        &[]
    }
}

impl Visitor for ImportWhitespaceRule {
    fn reset(&mut self) {
        *self = Self;
    }

    fn import_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        stmt: &ImportStatement,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // First, check internal whitespace.
        let internal_whitespace = stmt
            .inner()
            .children_with_tokens()
            .filter(|c| c.kind() == SyntaxKind::Whitespace)
            .map(|c| c.into_token().unwrap());

        for token in internal_whitespace {
            if token.text() != " " && token.prev_token().unwrap().kind() != SyntaxKind::Comment {
                diagnostics.exceptable_add(
                    improper_whitespace_within_import(token.text_range().into()),
                    SyntaxElement::from(token),
                    &self.exceptable_nodes(),
                );
            }
        }

        // Second, check for whitespace before the import.
        let prev_token = stmt
            .inner()
            .prev_sibling_or_token()
            .and_then(SyntaxElement::into_token);

        if let Some(token) = prev_token
            && token.kind() == SyntaxKind::Whitespace
            && !token.text().ends_with('\n')
        {
            // Find the span of just the leading whitespace
            let span: Span = token.text_range().into();
            for (text, offset, _) in lines_with_offset(token.text()) {
                if !text.is_empty() {
                    diagnostics.exceptable_add(
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

        // Third, check for whitespace between imports.
        let between_imports = stmt
            .inner()
            .prev_sibling()
            .map(|s| s.kind() == SyntaxKind::ImportStatementNode)
            .unwrap_or(false);
        if !between_imports {
            // Another rule will catch any whitespace here.
            return;
        }

        let mut prev_token = stmt
            .inner()
            .prev_sibling_or_token()
            .and_then(SyntaxElement::into_token);

        while let Some(token) = prev_token {
            if token.kind() == SyntaxKind::Whitespace {
                let mut should_warn = false;
                for (i, (text, ..)) in lines_with_offset(token.text()).enumerate() {
                    if i == 1 && text.is_empty() {
                        should_warn = true;
                    } else if i == 2 {
                        should_warn = false;
                        break;
                    }
                }

                if should_warn {
                    let span = token.text_range().into();
                    diagnostics.exceptable_add(
                        blank_between_imports(span),
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
