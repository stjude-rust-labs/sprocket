//! A lint rule for flagging preamble comments which are outside the preamble.

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

/// The identifier for the rule.
const ID: &str = "PreambleCommentPlacement";

/// Creates a diagnostic for a comment outside the preamble.
fn preamble_comment_outside_preamble(span: Span) -> Diagnostic {
    Diagnostic::note("preamble comment after the version statement")
        .with_rule(ID)
        .with_highlight(span)
        .with_help("freestanding doc comments (`##`) are assumed to be preamble comments")
        .with_fix("move the preamble comment before the `version` statement")
}

/// A lint rule for flagging preamble comments which are outside the preamble.
#[derive(Default, Debug, Clone, Copy)]
pub struct PreambleCommentPlacementRule {
    /// Exited the preamble.
    exited_preamble: bool,
    /// The number of comment tokens to skip.
    ///
    /// This is used when consolidating multiple comments into a single
    /// diagnostic.
    skip_count: usize,
}

impl Rule for PreambleCommentPlacementRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that preamble comments are inside the preamble."
    }

    fn explanation(&self) -> &'static str {
        "Preamble comments should only appear in the preamble section of a WDL document. This rule \
         ensures that freestanding double-pound comments (`##`) are not used after the version \
         statement."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Clarity, Tag::Style, Tag::SprocketCompatibility])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        None
    }

    fn related_rules(&self) -> &[&'static str] {
        &["PreambleFormatted"]
    }
}

impl Visitor for PreambleCommentPlacementRule {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn version_statement(
        &mut self,
        _diagnostics: &mut Diagnostics,
        reason: VisitReason,
        _stmt: &wdl_ast::VersionStatement,
    ) {
        if reason == VisitReason::Exit {
            self.exited_preamble = true;
        }
    }

    fn comment(&mut self, diagnostics: &mut Diagnostics, comment: &Comment) {
        if !self.exited_preamble {
            return;
        }

        if self.skip_count > 0 {
            self.skip_count -= 1;
            return;
        }

        if !comment.text().starts_with("## ") {
            return;
        }

        let mut span = comment.span();
        let mut current = comment.inner().next_sibling_or_token();

        // Floating comments aren't allowed. For example:
        //
        // ```
        // ## I'm not documenting the struct!
        //
        // struct Foo {}
        // ```
        //
        // So any floating comments are assumed to be preamble comments
        let mut floating = false;
        while let Some(sibling) = current {
            match sibling.kind() {
                SyntaxKind::Comment => {
                    self.skip_count += 1;

                    if !sibling
                        .as_token()
                        .expect("expected a token")
                        .text()
                        .starts_with("## ")
                    {
                        // The sibling comment is valid, so we can break.
                        break;
                    }

                    // Not valid, update the span
                    span = Span::new(
                        span.start(),
                        usize::from(sibling.text_range().end()) - span.start(),
                    )
                }
                SyntaxKind::Whitespace => {
                    if let Some(token) = sibling.as_token() {
                        floating = token.text().chars().filter(|c| *c == '\n').count() > 1;
                    };
                }
                _ => break,
            }

            current = sibling.next_sibling_or_token();
        }

        if floating {
            diagnostics.exceptable_add(
                preamble_comment_outside_preamble(span),
                SyntaxElement::from(comment.inner().clone()),
                &self.exceptable_nodes(),
            );
        }
    }
}
