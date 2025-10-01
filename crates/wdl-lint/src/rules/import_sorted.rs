//! A lint rule for ensuring that imports are sorted lexicographically.

use wdl_analysis::Diagnostics;
use wdl_analysis::Document;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::ImportStatement;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the import sort rule.
const ID: &str = "ImportSorted";

/// Creates an import not sorted diagnostic.
fn import_not_sorted(span: Span, sorted_imports: String) -> Diagnostic {
    Diagnostic::note("imports are not sorted lexicographically")
        .with_rule(ID)
        .with_label("imports must be sorted", span)
        .with_fix(format!(
            "sort the imports lexicographically:\n{sorted_imports}"
        ))
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
pub struct ImportSortedRule;

impl Rule for ImportSortedRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that imports are sorted lexicographically."
    }

    fn explanation(&self) -> &'static str {
        "Imports should be sorted lexicographically to make it easier to find specific imports. \
         This rule ensures that imports are sorted in a consistent manner. Specifically, the \
         desired sort can be achieved with a GNU compliant `sort` and `LC_COLLATE=C`. No comments \
         are permitted within an import statement."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Sorting])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[SyntaxKind::VersionStatementNode])
    }

    fn related_rules(&self) -> &[&'static str] {
        &[]
    }
}

impl Visitor for ImportSortedRule {
    fn reset(&mut self) {
        *self = Self;
    }

    fn document(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        doc: &Document,
        _: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        *self = Default::default();

        // Collect all import statements
        let imports: Vec<_> = doc
            .root()
            .inner()
            .children_with_tokens()
            .filter(|n| n.kind() == SyntaxKind::ImportStatementNode)
            .filter_map(|c| c.into_node())
            .collect();

        if imports.is_empty() {
            return;
        }

        // Clone imports for comparison
        let mut sorted_imports = imports.clone();
        sorted_imports.sort_by(|a, b| {
            let a_uri = ImportStatement::cast(a.clone())
                .expect("import statement")
                .uri()
                .text()
                .expect("import uri should not be interpolated");
            let b_uri = ImportStatement::cast(b.clone())
                .expect("import statement")
                .uri()
                .text()
                .expect("import uri should not be interpolated");
            a_uri.text().cmp(b_uri.text())
        });

        if imports != sorted_imports {
            let span = imports
                .first()
                .expect("there should be at least one import")
                .first_token()
                .expect("node should have a first token")
                .text_range()
                .into();
            diagnostics.add(import_not_sorted(
                span,
                sorted_imports
                    .iter()
                    .map(|i| i.text().to_string())
                    .collect::<Vec<_>>()
                    .join("\n"),
            ));
        }
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

        // Check for comments inside this import statement.
        let internal_comments = stmt
            .inner()
            .children_with_tokens()
            .filter(|c| c.kind() == SyntaxKind::Comment)
            .map(|c| c.into_token().unwrap());

        for comment in internal_comments {
            // Since this rule can only be excepted in a document-wide fashion,
            // if the rule is running we can directly add the diagnostic
            // without checking for the exceptable nodes
            diagnostics.add(improper_comment(comment.text_range().into()));
        }
    }
}
