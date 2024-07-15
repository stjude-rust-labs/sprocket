//! A lint rule that checks for an incorrect preamble comments.

use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::VersionStatement;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;

use crate::Rule;
use crate::Tag;
use crate::TagSet;
use crate::EXCEPT_COMMENT_PREFIX;

/// The identifier for the preamble comments rule.
const ID: &str = "PreambleComments";

/// Creates an "invalid preamble comment" diagnostic.
fn invalid_preamble_comment(span: Span) -> Diagnostic {
    Diagnostic::note("preamble comments must start with `##` followed by a space")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("change each preamble comment to start with `##` followed by a space")
}

/// Creates a "preamble comment after version" diagnostic.
fn preamble_comment_after_version(span: Span) -> Diagnostic {
    Diagnostic::note("preamble comments cannot come after the version statement")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("change each comment to start with `#` followed by a space")
}

/// Detects incorrect comments in a document preamble.
#[derive(Default, Debug, Clone, Copy)]
pub struct PreambleCommentsRule {
    /// Whether or not the preamble has finished.
    finished: bool,
    /// The number of comment tokens to skip.
    skip_count: usize,
}

impl Rule for PreambleCommentsRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that documents have correct comments in the preamble."
    }

    fn explanation(&self) -> &'static str {
        "Preamble comments are full line comments before the version declaration and they start \
         with a double pound sign. These comments are reserved for documentation that doesn't fit \
         within any of the WDL-defined documentation elements (such as `meta` and `parameter_meta` \
         sections). They may provide context for a collection of tasks or structs, or they may \
         provide a high-level overview of the workflow. Double-pound-sign comments are not allowed \
         after the version declaration. All comments before the version declaration should start \
         with a double pound sign (or if they are not suitable as preamble comments they should be \
         moved to _after_ the version declaration). Comments beginning with 3 or more pound signs \
         are permitted after the version declaration, as they are not considered preamble \
         comments. Comments beginning with 3 or more pound signs before the version declaration \
         are not permitted."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Spacing, Tag::Style, Tag::Clarity])
    }
}

impl Visitor for PreambleCommentsRule {
    type State = Diagnostics;

    fn document(
        &mut self,
        _: &mut Self::State,
        reason: VisitReason,
        _: &Document,
        _: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Reset the visitor upon document entry
        *self = Default::default();
    }

    fn version_statement(
        &mut self,
        _: &mut Self::State,
        reason: VisitReason,
        _: &VersionStatement,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // We're finished after the version statement
        self.finished = true;
    }

    fn comment(&mut self, state: &mut Self::State, comment: &Comment) {
        // Skip this comment if necessary; this occurs if we've consolidated multiple
        // comments in a row into a single diagnostic
        if self.skip_count > 0 {
            self.skip_count -= 1;
            return;
        }

        let check = |text: &str| {
            let double_pound = text == "##" || text.starts_with("## ");
            let except = text.starts_with(EXCEPT_COMMENT_PREFIX);
            (self.finished && !double_pound) || (!self.finished && (double_pound | except))
        };

        if check(comment.as_str()) {
            // The comment is valid, stop here
            return;
        }

        // Otherwise, look for the next siblings that might also be invalid;
        // if so, consolidate them into a single diagnostic
        let mut span = comment.span();
        let mut current = comment.syntax().next_sibling_or_token();
        while let Some(sibling) = current {
            match sibling.kind() {
                SyntaxKind::Comment => {
                    // As we're processing this sibling comment here, increment the skip count
                    self.skip_count += 1;

                    if check(sibling.as_token().expect("should be a token").text()) {
                        // The comment is valid, stop here
                        break;
                    }

                    // Not valid, update the span
                    span = Span::new(
                        span.start(),
                        usize::from(sibling.text_range().end()) - span.start(),
                    );
                }
                SyntaxKind::Whitespace => {
                    // Skip whitespace
                }
                _ => break,
            }

            current = sibling.next_sibling_or_token();
        }

        if self.finished {
            state.add(preamble_comment_after_version(span));
        } else {
            state.add(invalid_preamble_comment(span));
        }
    }
}
