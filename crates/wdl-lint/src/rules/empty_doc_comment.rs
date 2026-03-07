//! A lint rule for empty documentation comments.

use wdl_analysis::Diagnostics;
use wdl_analysis::Visitor;
use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::DOC_COMMENT_PREFIX;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the empty doc comment rule.
const ID: &str = "EmptyDocComment";

/// Creates a diagnostic when an empty documentation comment block is found.
fn empty_doc_comment(span: Span) -> Diagnostic {
    Diagnostic::note("empty doc comment block")
        .with_rule(ID)
        .with_highlight(span)
        .with_help("consider adding meaningful documentation text or removing the comment block")
}

/// Detects empty documentation comment blocks.
#[derive(Default, Debug, Clone, Copy)]
pub struct EmptyDocCommentRule {
    /// The number of comment tokens to skip.
    ///
    /// This is used to avoid processing comments that have already been
    /// handled as part of a block.
    skip_count: usize,
}

impl Rule for EmptyDocCommentRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that documentation comment blocks are not empty."
    }

    fn explanation(&self) -> &'static str {
        "Documentation comment blocks (consecutive lines starting with `##`) where all lines are \
         empty serve no purpose. Additionally, if a lint for missing documentation comments is \
         added in the future, these empty comment blocks could be incorrectly used to silence it. \
         Either add meaningful text to the documentation comment block or remove it entirely. \
         Note that blank lines within a doc block that contains non-empty lines are acceptable as \
         paragraph separators."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Clarity, Tag::Completeness])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        None
    }

    fn related_rules(&self) -> &[&'static str] {
        &["CommentWhitespace"]
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

        // Only process documentation comments
        if !comment.is_doc_comment() {
            return;
        }

        // Collect information about the entire contiguous doc comment block
        let first_span = comment.span();
        let mut last_span = first_span;
        let mut all_empty = {
            let text = comment.text();
            let content = text.strip_prefix(DOC_COMMENT_PREFIX).unwrap_or(text);
            content.trim().is_empty()
        };

        let mut current = comment.inner().next_sibling_or_token();

        while let Some(sibling) = current {
            match sibling.kind() {
                SyntaxKind::Comment => {
                    if let Some(c) = Comment::cast(sibling.as_token().unwrap().clone()) {
                        if c.is_doc_comment() {
                            // Check if this comment is empty
                            let text = c.text();
                            let content = text.strip_prefix(DOC_COMMENT_PREFIX).unwrap_or(text);
                            if !content.trim().is_empty() {
                                all_empty = false;
                            }

                            last_span = c.span();
                            self.skip_count += 1;
                        } else {
                            // Hit a non-doc comment, stop collecting
                            break;
                        }
                    } else {
                        break;
                    }
                }
                SyntaxKind::Whitespace => {
                    // Continue through whitespace to find more doc comments
                }
                _ => {
                    // Hit a non-comment, non-whitespace element, stop
                    break;
                }
            }

            current = sibling.next_sibling_or_token();
        }

        // Only flag if all comments in the block are empty
        if all_empty {
            // Calculate the span for the entire block
            let span = Span::new(first_span.start(), last_span.end() - first_span.start());

            diagnostics.exceptable_add(
                empty_doc_comment(span),
                SyntaxElement::from(comment.inner().clone()),
                &self.exceptable_nodes(),
            );
        }
    }
}
