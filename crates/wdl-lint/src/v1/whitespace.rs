//! A lint rule for whitespace.

use wdl_ast::experimental::v1::Visitor;
use wdl_ast::experimental::AstToken;
use wdl_ast::experimental::Diagnostic;
use wdl_ast::experimental::Diagnostics;
use wdl_ast::experimental::Span;
use wdl_ast::experimental::SyntaxKind;
use wdl_ast::experimental::VersionStatement;
use wdl_ast::experimental::VisitReason;
use wdl_ast::experimental::Whitespace;

use super::Rule;
use crate::util::lines_with_offset;
use crate::Tag;
use crate::TagSet;

/// The identifier for the whitespace rule.
const ID: &str = "Whitespace";

/// Creates an "only whitespace" diagnostic.
fn only_whitespace(span: Span) -> Diagnostic {
    Diagnostic::warning("line contains only whitespace")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("remove the whitespace from this line")
}

/// Creates a "trailing whitespace" diagnostic.
fn trailing_whitespace(span: Span) -> Diagnostic {
    Diagnostic::warning("line contains trailing whitespace")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("remove the trailing whitespace from this line")
}

/// Creates a "more than one blank line" diagnostic.
fn more_than_one_blank_line(span: Span) -> Diagnostic {
    Diagnostic::warning("more than one blank line in a row")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("remove the unnecessary blank lines")
}

/// Detects undesired whitespace.
#[derive(Debug, Clone, Copy)]
pub struct WhitespaceRule;

impl Rule for WhitespaceRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that a document does not contain undesired whitespace."
    }

    fn explanation(&self) -> &'static str {
        "Whitespace should be used judiciously. Spurious whitespace can cause issues with parsing, \
         automation, and rendering. There should never be trailing whitespace at the end of lines \
         and blank lines should be completely empty with no whitespace characters between \
         newlines. There should be at most one empty line in a row."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Style, Tag::Spacing])
    }

    fn visitor(&self) -> Box<dyn Visitor<State = Diagnostics>> {
        Box::new(WhitespaceVisitor::default())
    }
}

/// Implements the visitor for whitespace rule.
#[derive(Default)]
struct WhitespaceVisitor {
    /// Whether or not the version statement has been encountered in the
    /// document.
    has_version: bool,
}

impl Visitor for WhitespaceVisitor {
    type State = Diagnostics;

    fn version_statement(
        &mut self,
        _: &mut Self::State,
        reason: VisitReason,
        _: &VersionStatement,
    ) {
        if reason != VisitReason::Exit {
            return;
        }

        self.has_version = true;
    }

    fn whitespace(&mut self, state: &mut Self::State, whitespace: &Whitespace) {
        // Only process whitespace after the version statement as
        // the preamble whitespace rule will otherwise validate it
        if !self.has_version {
            return;
        }

        // Check to see if this whitespace is the last token in the document (i.e. the
        // parent is the root and there is no sibling)
        let is_last = whitespace
            .syntax()
            .parent()
            .expect("should have a parent")
            .kind()
            == SyntaxKind::RootNode
            && whitespace.syntax().next_sibling_or_token().is_none();

        let text = whitespace.as_str();
        let span = whitespace.span();
        let mut blank_start = None;
        for (i, (line, start, next_start)) in lines_with_offset(text).enumerate() {
            let ends_with_newline = text.as_bytes().get(next_start - 1) == Some(&b'\n');

            // If the line isn't empty and either ends with a newline or is the last
            // element in the document, then it is invalid whitespace
            if !line.is_empty() && (ends_with_newline || is_last) {
                // If it's the first line, it's considered trailing
                // The remaining lines will be treated as "blank".
                if i == 0 {
                    state.add(trailing_whitespace(Span::new(
                        span.start() + start,
                        line.len(),
                    )));
                } else {
                    state.add(only_whitespace(Span::new(span.start() + start, line.len())));
                }
            }

            // At the third blank line that ends with a newline, record the start to report
            // on "too many blank lines"
            if i == 2 && ends_with_newline {
                blank_start = Some(start);
            }
        }

        // Only report on multiple blank lines if not at the end of the file
        // The "ending newline" rule will catch blank lines at the end of the file
        if !is_last {
            if let Some(start) = blank_start {
                state.add(more_than_one_blank_line(Span::new(
                    span.start() + start,
                    span.len() - start,
                )));
            }
        }
    }
}
