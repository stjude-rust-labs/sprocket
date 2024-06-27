//! A lint rule for ensuring that newlines are consistent.

use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Span;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;
use wdl_ast::Whitespace;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the inconsistent newlines rule.
const ID: &str = "InconsistentNewlines";

/// Creates an inconsistent newlines diagnostic.
fn inconsistent_newlines(span: Span) -> Diagnostic {
    Diagnostic::note("inconsistent newlines detected")
        .with_rule(ID)
        .with_label("the first occurrence of a mismatched newline is here", span)
        .with_fix("use either \"\\n\" or \"\\r\\n\" consistently in the file")
}

/// Detects imports that are not sorted lexicographically.
#[derive(Default, Debug, Clone, Copy)]
pub struct InconsistentNewlinesRule {
    /// The number of carriage returns in the file.
    carriage_return: u32,
    /// The number of newlines in the file.
    newline: u32,
    /// Location of first inconsistent newline.
    first_inconsistent: Option<Span>,
}

impl Rule for InconsistentNewlinesRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that newline usage is consistent."
    }

    fn explanation(&self) -> &'static str {
        "Files should not mix \\n and \\r\\n line breaks. Pick one and use it consistently in your \
         project."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Style, Tag::Clarity])
    }
}

impl Visitor for InconsistentNewlinesRule {
    type State = Diagnostics;

    fn document(&mut self, state: &mut Self::State, reason: VisitReason, _doc: &wdl_ast::Document) {
        if reason == VisitReason::Exit && self.newline > 0 && self.carriage_return > 0 {
            state.add(inconsistent_newlines(self.first_inconsistent.unwrap()));
        }
    }

    fn whitespace(&mut self, _state: &mut Self::State, whitespace: &Whitespace) {
        if let Some(pos) = whitespace.as_str().find("\r\n") {
            self.carriage_return += 1;
            if self.newline > 0 && self.first_inconsistent.is_none() {
                self.first_inconsistent = Some(Span::new(whitespace.span().start() + pos, 2));
            }
        } else if let Some(pos) = whitespace.as_str().find('\n') {
            self.newline += 1;
            if self.carriage_return > 0 && self.first_inconsistent.is_none() {
                self.first_inconsistent = Some(Span::new(whitespace.span().start() + pos, 1));
            }
        }
    }
}
