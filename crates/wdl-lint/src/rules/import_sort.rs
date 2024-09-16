//! A lint rule for ensuring that imports are sorted lexicographically.

use wdl_ast::v1::ImportStatement;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_ast::ToSpan;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the import sort rule.
const ID: &str = "ImportSort";

/// Creates an import not sorted diagnostic.
fn import_not_sorted(span: Span) -> Diagnostic {
    Diagnostic::note("imports are not sorted lexicographically")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("sort the imports lexicographically")
}

/// Creates an improper comment diagnostic.
fn improper_comment(span: Span) -> Diagnostic {
    Diagnostic::note("comments are not allowed within an import statement")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("remove the comment from the import statement")
}

/// Detects imports that are not sorted lexicographically.
#[derive(Default, Debug, Clone, Copy)]
pub struct ImportSortRule;

impl Rule for ImportSortRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that imports are sorted lexicographically."
    }

    fn explanation(&self) -> &'static str {
        "Imports should be sorted lexicographically to make it easier to find specific imports. \
         This rule ensures that imports are sorted in a consistent manner. Specifically, the \
         desired sort can be acheived with a GNU compliant `sort` and `LC_COLLATE=C`. No comments \
         are permitted within an import statement."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Style, Tag::Clarity])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[SyntaxKind::VersionStatementNode])
    }
}

impl Visitor for ImportSortRule {
    type State = Diagnostics;

    fn document(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        doc: &Document,
        _: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Reset the visitor upon document entry
        *self = Default::default();

        let imports = doc
            .syntax()
            .children_with_tokens()
            .filter(|c| c.kind() == SyntaxKind::ImportStatementNode)
            .map(|c| c.into_node().unwrap());

        let mut prev_import: Option<SyntaxNode> = None;
        for import in imports {
            if let Some(prev) = prev_import {
                if import.text().to_string() < prev.text().to_string() {
                    // Since this rule can only be excepted in a document-wide fashion,
                    // if the rule is running we can directly add the diagnostic
                    // without checking for the exceptable nodes
                    state.add(import_not_sorted(import.text_range().to_span()));
                    return; // Only report one sorting diagnostic at a time.
                }
            }
            prev_import = Some(import);
        }
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

        // Check for comments inside this import statement.
        let internal_comments = stmt
            .syntax()
            .children_with_tokens()
            .filter(|c| c.kind() == SyntaxKind::Comment)
            .map(|c| c.into_token().unwrap());

        for comment in internal_comments {
            // Since this rule can only be excepted in a document-wide fashion,
            // if the rule is running we can directly add the diagnostic
            // without checking for the exceptable nodes
            state.add(improper_comment(comment.text_range().to_span()));
        }
    }
}
