//! A lint rule that checks for an incorrect preamble comments.

use wdl_ast::experimental::v1::Visitor;
use wdl_ast::experimental::AstToken;
use wdl_ast::experimental::Comment;
use wdl_ast::experimental::Diagnostic;
use wdl_ast::experimental::Diagnostics;
use wdl_ast::experimental::Span;
use wdl_ast::experimental::VersionStatement;
use wdl_ast::experimental::VisitReason;

use super::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the preamble comments rule.
const ID: &str = "PreambleComments";

/// Creates an "invalid preamble comment" diagnostic.
fn invalid_preamble_comment(span: Span) -> Diagnostic {
    Diagnostic::note("preamble comments must start with `##`")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("change the comment to start with `##` followed by a space")
}

/// Creates a "too many pound signs" diagnostic.
fn too_many_pound_signs(span: Span) -> Diagnostic {
    Diagnostic::note("preamble comments cannot start with more than two `#`")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("change the comment to start with `##` followed by a space")
}

/// Creates a "missing space" diagnostic.
fn missing_space(span: Span) -> Diagnostic {
    Diagnostic::note("preamble comments must have a space after `##`")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("add a space between `##` and the start of the comment")
}

/// Creates a "preamble comment after version" diagnostic.
fn preamble_comment_after_version(span: Span) -> Diagnostic {
    Diagnostic::note("preamble comments cannot come after the version statement")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("change the comment to start with `#` followed by a space")
}

/// Detects incorrect comments in a document preamble.
#[derive(Debug, Clone, Copy)]
pub struct PreambleCommentsRule;

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

    fn visitor(&self) -> Box<dyn Visitor<State = Diagnostics>> {
        Box::new(PreambleCommentsVisitor::default())
    }
}

/// Implements the visitor for the preamble comments rule.
#[derive(Default, Debug)]
struct PreambleCommentsVisitor {
    /// Whether or not the preamble has finished.
    finished: bool,
}

impl Visitor for PreambleCommentsVisitor {
    type State = Diagnostics;

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
        let text = comment.as_str();
        if self.finished {
            if let Some(text) = text.strip_prefix("##") {
                if !text.starts_with('#') {
                    state.add(preamble_comment_after_version(comment.span()));
                }
            }

            return;
        }

        if let Some(text) = text.strip_prefix("##") {
            // Check for too many pound signs
            if text.starts_with('#') {
                state.add(too_many_pound_signs(comment.span()));
                return;
            }

            // Check for missing space
            if !text.is_empty() && !text.starts_with(' ') {
                state.add(missing_space(comment.span()));
                return;
            }

            // Valid preamble comment
            return;
        }

        state.add(invalid_preamble_comment(comment.span()));
    }
}
