//! A lint rule for empty documentation comments.

use wdl_analysis::Diagnostics;
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

/// The identifier for the empty doc comment rule.
const ID: &str = "EmptyDocComment";

/// Creates a diagnostic when an empty documentation comment is found.
fn empty_doc_comment(span: Span) -> Diagnostic {
    Diagnostic::note("empty doc comment")
        .with_rule(ID)
        .with_highlight(span)
        .with_help("consider adding a comment after the `##` or removing it")
}

/// Detects empty documentation comments.
#[derive(Default, Debug, Clone, Copy)]
pub struct EmptyDocCommentRule {
    /// The number of comment tokens to skip.
    ///
    /// This is used when consolidating multiple comments into a single
    /// diagnostic.
    skip_count: usize,
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

    fn comment(&mut self, diagnostics: &mut Diagnostics, comment: &Comment) {
        // Skip if we've already processed this comment as part of a block
        if self.skip_count > 0 {
            self.skip_count -= 1;
            return;
        }

        // Check if this is an inline comment (on the same line as code)
        // by looking at the previous sibling
        if let Some(prior) = comment.inner().prev_sibling_or_token() {
            let is_inline = prior.kind() != SyntaxKind::Whitespace
                || !prior
                    .as_token()
                    .expect("whitespace should be a token")
                    .text()
                    .contains('\n');

            if is_inline {
                return;
            }
        }

        let text = comment.text();

        // Check if this is a documentation comment (starts with ##)
        if !text.starts_with("##") {
            return;
        }

        // Extract the content after ##
        let content = &text[2..];

        // Check if the content is empty or only whitespace
        if !content.trim().is_empty() {
            return;
        }

        // We have an empty doc comment - now collect any consecutive empty doc comments
        let mut span = comment.span();
        let mut current = comment.inner().next_sibling_or_token();

        while let Some(sibling) = current {
            match sibling.kind() {
                SyntaxKind::Comment => {
                    let sibling_text = sibling.as_token().expect("expected a token").text();

                    // Check if this is also an empty doc comment
                    if sibling_text.starts_with("##") && sibling_text[2..].trim().is_empty() {
                        self.skip_count += 1;

                        // Extend the span to include this comment
                        span = Span::new(
                            span.start(),
                            usize::from(sibling.text_range().end()) - span.start(),
                        );
                    } else {
                        // Not an empty doc comment, stop collecting
                        break;
                    }
                }
                SyntaxKind::Whitespace => {
                    // Continue through whitespace to find more comments
                }
                _ => {
                    // Hit a non-comment, non-whitespace element, stop
                    break;
                }
            }

            current = sibling.next_sibling_or_token();
        }

        diagnostics.exceptable_add(
            empty_doc_comment(span),
            SyntaxElement::from(comment.inner().clone()),
            &self.exceptable_nodes(),
        );
    }
}
