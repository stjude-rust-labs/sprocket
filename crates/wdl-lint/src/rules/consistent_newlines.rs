//! A lint rule for ensuring that newlines are consistent.

use wdl_analysis::Diagnostics;
use wdl_analysis::Document;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::Whitespace;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the inconsistent newlines rule.
const ID: &str = "ConsistentNewlines";

/// Creates an inconsistent newlines diagnostic.
fn inconsistent_newlines(span: Span) -> Diagnostic {
    Diagnostic::note("inconsistent line endings detected")
        .with_rule(ID)
        .with_label(
            "the first occurrence of a mismatched line ending is here",
            span,
        )
        .with_fix(
            "ensure that the same line endings (e.g., `\\n` or `\\r\\n`) are used throughout the \
             file",
        )
}

/// Detects imports that are not sorted lexicographically.
#[derive(Default, Debug, Clone, Copy)]
pub struct ConsistentNewlinesRule {
    /// The number of carriage returns in the file.
    carriage_return: u32,
    /// The number of newlines in the file.
    newline: u32,
    /// Location of first inconsistent newline.
    first_inconsistent: Option<Span>,
}

impl Rule for ConsistentNewlinesRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that newline usage is consistent."
    }

    fn explanation(&self) -> &'static str {
        "Files should not mix `\\n` and `\\r\\n` line breaks. Pick one and use it consistently in \
         your project."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Portability, Tag::Clarity, Tag::Spacing])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[SyntaxKind::VersionStatementNode])
    }

    fn related_rules(&self) -> &[&'static str] {
        &[]
    }
}

impl Visitor for ConsistentNewlinesRule {
    fn reset(&mut self) {
        *self = Default::default();
    }

    fn document(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        _doc: &Document,
        _: SupportedVersion,
    ) {
        if reason == VisitReason::Enter {
            return;
        }

        if self.newline > 0 && self.carriage_return > 0 {
            // Since this rule can only be excepted in a document-wide fashion,
            // if the rule is running we can directly add the diagnostic
            // without checking for the exceptable nodes
            diagnostics.add(inconsistent_newlines(self.first_inconsistent.unwrap()));
        }
    }

    fn whitespace(&mut self, _diagnostics: &mut Diagnostics, whitespace: &Whitespace) {
        if let Some(pos) = whitespace.text().find("\r\n") {
            self.carriage_return += 1;
            if self.newline > 0 && self.first_inconsistent.is_none() {
                self.first_inconsistent = Some(Span::new(whitespace.span().start() + pos, 2));
            }
        } else if let Some(pos) = whitespace.text().find('\n') {
            self.newline += 1;
            if self.carriage_return > 0 && self.first_inconsistent.is_none() {
                self.first_inconsistent = Some(Span::new(whitespace.span().start() + pos, 1));
            }
        }
    }
}
