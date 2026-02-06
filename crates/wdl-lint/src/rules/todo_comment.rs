//! A lint rule for flagging TODOs.

use wdl_analysis::Diagnostics;
use wdl_analysis::Visitor;
use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the todos rule.
const ID: &str = "TodoComment";

/// The `TODO` token.
const TODO: &str = "TODO";

/// Detects remaining TODOs within comments.
#[derive(Default, Debug, Clone, Copy)]
pub struct TodoCommentRule;

/// Creates a "todo comment" diagnostic.
fn todo_comment(comment: &str, comment_span: Span, offset: usize) -> Diagnostic {
    let start = comment_span.start() + offset;

    Diagnostic::note(format!("remaining `{TODO}` item found"))
        .with_rule(ID)
        .with_highlight(Span::new(start, comment.len()))
        .with_fix("remove the `TODO` item once it has been implemented")
}

impl Rule for TodoCommentRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Flags TODO statements in comments to ensure they are not forgotten."
    }

    fn explanation(&self) -> &'static str {
        "When writing WDL, future tasks are often marked as `TODO`. This indicates that the \
         implementor intended to go back to the code and handle the todo item. TODO items should \
         not be long-term fixtures within code and, as such, they are flagged to ensure none are \
         forgotten."
    }

    fn examples(&self) -> &'static [&'static str] {
        &[
            r#"```wdl
version 1.2

# The following comment will be flagged:
# TODO: Implement this workflow
workflow example {
    meta {}

    output {}
}
```"#,
        ]
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Style])
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        None
    }

    fn related_rules(&self) -> &[&'static str] {
        &[]
    }
}

impl Visitor for TodoCommentRule {
    fn reset(&mut self) {
        *self = Self;
    }

    fn comment(&mut self, diagnostics: &mut Diagnostics, comment: &Comment) {
        for (offset, pattern) in comment.text().match_indices(TODO) {
            diagnostics.exceptable_add(
                todo_comment(pattern, comment.span(), offset),
                SyntaxElement::from(comment.inner().clone()),
                &self.exceptable_nodes(),
            );
        }
    }
}
