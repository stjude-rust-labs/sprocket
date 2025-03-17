//! A lint rule for flagging preamble comments which are outside the preamble.

use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the rule.
const ID: &str = "PreambleCommentAfterVersion";

/// Creates a diagnostic for a comment outside the preamble.
fn preamble_comment_outside_preamble(span: Span) -> Diagnostic {
    Diagnostic::error("preamble comment after the version statement")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("do not use `##` comments outside the preamble")
}

/// A lint rule for flagging preamble comments which are outside the preamble.
#[derive(Default, Debug, Clone, Copy)]
pub struct PreambleCommentAfterVersionRule {
    /// Exited the preamble.
    exited_preamble: bool,
    /// The number of comment tokens to skip.
    ///
    /// This is used when consolidating multiple comments into a single
    /// diagnositc.
    skip_count: usize,
}

impl Rule for PreambleCommentAfterVersionRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that preamble comments are inside the preamble."
    }

    fn explanation(&self) -> &'static str {
        "Preamble comments should be inside the preamble."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Clarity])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        None
    }
}

impl Visitor for PreambleCommentAfterVersionRule {
    type State = Diagnostics;

    fn document(
        &mut self,
        _: &mut Self::State,
        reason: VisitReason,
        _: &Document,
        _: wdl_ast::SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Reset the visitor upon document entry.
        *self = Default::default();
    }

    fn version_statement(
        &mut self,
        _state: &mut Self::State,
        reason: VisitReason,
        _stmt: &wdl_ast::VersionStatement,
    ) {
        if reason == VisitReason::Exit {
            self.exited_preamble = true;
        }
    }

    fn comment(&mut self, state: &mut Self::State, comment: &Comment) {
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
                    // Skip whitespace
                }
                _ => break,
            }

            current = sibling.next_sibling_or_token();
        }

        state.exceptable_add(
            preamble_comment_outside_preamble(span),
            SyntaxElement::from(comment.inner().clone()),
            &self.exceptable_nodes(),
        );
    }
}
