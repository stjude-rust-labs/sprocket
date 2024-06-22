//! A lint rule for preventing whitespace between imports.

use wdl_ast::v1::ImportStatement;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::ToSpan;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;

use crate::util::lines_with_offset;
use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the import sort rule.
const ID: &str = "ImportWhitespace";

/// Creates a bad import whitespace diagnostic.
fn bad_import_whitespace(span: Span) -> Diagnostic {
    Diagnostic::note("blank lines are not allowed between imports")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("remove any blank lines between imports")
}

/// Detects whitespace between imports.
#[derive(Debug, Clone, Copy)]
pub struct ImportWhitespaceRule;

impl Rule for ImportWhitespaceRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that there is no extraneous whitespace between imports."
    }

    fn explanation(&self) -> &'static str {
        "Imports should be grouped together without any whitespace between them. _If_ separation \
         between imports is needed, it should be done with one or more comments labelling groups \
         of imports. Extraneous whitespace between imports makes code harder to parse and \
         understand."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Style, Tag::Clarity, Tag::Spacing])
    }

    fn visitor(&self) -> Box<dyn Visitor<State = Diagnostics>> {
        Box::new(ImportWhitespaceVisitor)
    }
}

/// Implements the visitor for the import whitespace rule.
#[derive(Debug, Default)]
struct ImportWhitespaceVisitor;

impl Visitor for ImportWhitespaceVisitor {
    type State = Diagnostics;

    fn import_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &ImportStatement,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

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
                for (i, (_, _, next)) in lines_with_offset(token.text()).enumerate() {
                    if i == 0 {
                        second_line_start = Some(next);
                    } else if i == 1 {
                        should_warn = true;
                    } else if i == 2 {
                        should_warn = false;
                        break;
                    }
                }

                if should_warn {
                    let span = token.text_range().to_span();
                    state.add(bad_import_whitespace(Span::new(
                        span.start() + second_line_start.expect("should have a second line start"),
                        span.len() - second_line_start.expect("should have a second line start"),
                    )));
                }
            } else if token.kind() != SyntaxKind::Comment {
                // We've backed into non-trivia, so we're done.
                break;
            }
            prev_token = token.prev_token();
        }
    }
}
