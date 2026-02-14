//! A lint rule for empty documentation comments.

use wdl_analysis::Diagnostics;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;

use crate::Rule;
use crate::Tag;
use crate::TagSet;
use crate::util::is_inline_comment;

/// The identifier for the empty doc comment rule.
const ID: &str = "EmptyDocComment";

/// Creates a diagnostic when an empty documentation comment is found.
fn empty_doc_comment(span: Span) -> Diagnostic {
    Diagnostic::note("empty documentation comment serves no purpose")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("add comment text after `##` or remove the empty comment")
}

/// Detects empty documentation comments.
#[derive(Default, Debug, Clone, Copy)]
pub struct EmptyDocCommentRule {
    /// Whether or not the visitor has exited the preamble of the document.
    exited_preamble: bool,
}

impl Rule for EmptyDocCommentRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that documentation comments are not empty."
    }

    fn explanation(&self) -> &'static str {
        "Empty documentation comments (starting with `##` but containing no meaningful text) serve \
         no purpose. Additionally, if a lint for missing documentation comments is added in the \
         future, these empty comments could be incorrectly used to silence it. Either add \
         meaningful text to the documentation comment or remove it entirely."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Clarity, Tag::Completeness])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        None
    }

    fn related_rules(&self) -> &[&'static str] {
        &["CommentWhitespace", "PreambleCommentPlacement"]
    }
}

impl Visitor for EmptyDocCommentRule {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn version_statement(
        &mut self,
        _: &mut Diagnostics,
        reason: VisitReason,
        _: &wdl_ast::VersionStatement,
    ) {
        if reason == VisitReason::Exit {
            self.exited_preamble = true;
        }
    }

    fn comment(&mut self, diagnostics: &mut Diagnostics, comment: &Comment) {
        // Skip inline comments - only check comments on their own line
        if is_inline_comment(comment) {
            return;
        }

        let text = comment.text();

        // Check if this is a documentation comment (starts with ##)
        if !text.starts_with("##") {
            return;
        }

        // Extract the content after ##
        let content = &text[2..];

        // Check if the content is empty or only whitespace
        if content.trim().is_empty() {
            diagnostics.exceptable_add(
                empty_doc_comment(comment.span()),
                SyntaxElement::from(comment.inner().clone()),
                &self.exceptable_nodes(),
            );
        }
    }
}
