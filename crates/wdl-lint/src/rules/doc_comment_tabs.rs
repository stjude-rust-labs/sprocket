//! A lint rule for detecting tab characters in doc comments.

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

/// The identifier for the doc comment tabs rule.
const ID: &str = "DocCommentTabs";

/// Creates a diagnostic for a group of tab characters.
fn tab_in_doc_comment(span: Span) -> Diagnostic {
    Diagnostic::warning("tabs in doc comments are not recommended")
        .with_rule(ID)
        .with_highlight(span)
        .with_help("consider replacing tabs with spaces")
}

/// Detects tab characters inside doc comments.
#[derive(Default, Debug, Clone, Copy)]
pub struct DocCommentTabsRule;

impl Rule for DocCommentTabsRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that doc comments do not contain tab characters."
    }

    fn explanation(&self) -> &'static str {
        "Tabs render with different widths depending on the viewer. Doc comments should use spaces \
         instead of tabs to ensure consistent rendering."
    }

    fn examples(&self) -> &'static [&'static str] {
        &[
            r#"```wdl
version 1.3

# Using tabs for alignment
##  {
##		"foo": 123,
##		^^^^^
##	}
workflow example {
    meta {
        description: 123
    }

    output {}
}
```"#,
            r#"Use instead:

```wdl
version 1.3

# Using spaces for alignment
## {
##     "foo": 123,
##     ^^^^^
## }
workflow example {
    meta {
        description: "123"
    }

    output {}
}
```"#,
        ]
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Style, Tag::Clarity])
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        None
    }

    fn related_rules(&self) -> &[&'static str] {
        &[]
    }
}

impl Visitor for DocCommentTabsRule {
    fn reset(&mut self) {
        *self = Self;
    }

    fn comment(&mut self, diagnostics: &mut Diagnostics, comment: &Comment) {
        if !comment.is_doc_comment() {
            return;
        }
        let text = comment.text();

        let mut i = 0;
        let bytes = text.as_bytes();

        while i < bytes.len() {
            if bytes[i] == b'\t' {
                let start_offset = i;

                while i < bytes.len() && bytes[i] == b'\t' {
                    i += 1;
                }

                let len = i - start_offset;

                let absolute_start = comment.span().start() + start_offset;

                diagnostics.exceptable_add(
                    tab_in_doc_comment(Span::new(absolute_start, len)),
                    SyntaxElement::from(comment.inner().clone()),
                    &self.exceptable_nodes(),
                );
            } else {
                i += 1;
            }
        }
    }
}
