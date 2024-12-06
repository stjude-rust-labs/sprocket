//! A lint rule for flagging TODOs.

use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxElement;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the todos rule.
const ID: &str = "Todo";

/// The `TODO` token.
const TODO: &str = "TODO";

/// Detects remaining TODOs within comments.
#[derive(Default, Debug, Clone, Copy)]
pub struct TodoRule;

/// Creates a "todo comment" diagnostic.
fn todo_comment(comment: &str, comment_span: Span, offset: usize) -> Diagnostic {
    let start = comment_span.start() + offset;

    Diagnostic::note(format!("remaining `{TODO}` item found"))
        .with_rule(ID)
        .with_highlight(Span::new(start, comment.len()))
        .with_fix("remove the `TODO` item once it has been implemented")
}

impl Rule for TodoRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Flags TODO statements in comments to ensure they are not forgotten."
    }

    fn explanation(&self) -> &'static str {
        "When writing WDL, future tasks are often marked as `TODO`. This indicates that the \
         implementor intended to go back to the code and handle the todo item. Todo items should \
         not be long-term fixtures within code and, as such, they are flagged to ensure none are \
         forgotten."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Completeness])
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        None
    }
}

impl Visitor for TodoRule {
    type State = Diagnostics;

    fn document(&mut self, _: &mut Self::State, _: VisitReason, _: &Document, _: SupportedVersion) {
        // This is intentionally empty, as this rule has no state.
    }

    fn comment(&mut self, state: &mut Self::State, comment: &Comment) {
        for (offset, pattern) in comment.as_str().match_indices(TODO) {
            state.exceptable_add(
                todo_comment(pattern, comment.span(), offset),
                SyntaxElement::from(comment.syntax().clone()),
                &self.exceptable_nodes(),
            );
        }
    }
}
