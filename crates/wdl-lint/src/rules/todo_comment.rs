//! A lint rule for flagging TODOs.

use wdl_analysis::Diagnostics;
use wdl_analysis::Example;
use wdl_analysis::LabeledSnippet;
use wdl_analysis::Visitor;
use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::Diagnostic;
use wdl_ast::Severity;
use wdl_ast::Span;
use wdl_ast::TreeToken;

use crate::Config;
use crate::Rule;
use crate::Tag;
use crate::TagSet;
use crate::util::diagnostic;

/// The identifier for the todos rule.
const ID: &str = "TodoComment";

/// The `TODO` token.
const TODO: &str = "TODO";

/// Detects remaining TODOs within comments.
#[derive(Debug, Clone, Copy)]
pub struct TodoCommentRule {
    /// The severity of the rule's diagnostics.
    severity: Severity,
}

/// Creates a "todo comment" diagnostic.
fn todo_comment(
    severity: Severity,
    comment: &str,
    comment_span: Span,
    offset: usize,
) -> Diagnostic {
    let start = comment_span.start() + offset;

    diagnostic(severity, format!("remaining `{TODO}` item found"))
        .with_rule(ID)
        .with_highlight(Span::new(start, comment.len()))
        .with_fix("remove the `TODO` item once it has been implemented")
}

impl TodoCommentRule {
    /// Creates a new instance of the rule.
    pub fn new(config: &Config) -> Self {
        Self {
            severity: config.todo_comment.diagnostic_severity(),
        }
    }
}

impl Default for TodoCommentRule {
    fn default() -> Self {
        Self::new(&Config::default())
    }
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

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: Some("The following comment will be flagged"),
                snippet: r#"version 1.2

# TODO: Implement this workflow
workflow example {
    meta {
    }

    output {
    }
}
"#,
            },
            revised: None,
        }]
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Style])
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        None
    }

    fn related_rules(&self) -> &'static [&'static str] {
        &[]
    }
}

impl Visitor for TodoCommentRule {
    fn reset(&mut self) {}

    fn comment(&mut self, diagnostics: &mut Diagnostics, comment: &Comment) {
        for (offset, pattern) in comment.text().match_indices(TODO) {
            diagnostics.exceptable_add(
                todo_comment(self.severity, pattern, comment.span(), offset),
                &TreeToken::parent(comment.inner()),
                &self.exceptable_nodes(),
            );
        }
    }
}
