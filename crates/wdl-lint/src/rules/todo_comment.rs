//! A lint rule for flagging TODOs.

use wdl_analysis::Diagnostics;
use wdl_analysis::Example;
use wdl_analysis::LabeledSnippet;
use wdl_analysis::Visitor;
use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;

use crate::Config;
use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the todos rule.
const ID: &str = "TodoComment";

/// Detects remaining TODOs within comments.
#[derive(Debug, Clone)]
pub struct TodoCommentRule {
    /// The comment keywords that trigger the rule.
    keywords: Vec<String>,
}

impl TodoCommentRule {
    /// Creates a new instance of the rule from the given configuration.
    pub fn new(config: &Config) -> Self {
        Self {
            keywords: config.resolved(ID).keywords,
        }
    }
}

/// Creates a "todo comment" diagnostic.
fn todo_comment(keyword: &str, matched: &str, comment_span: Span, offset: usize) -> Diagnostic {
    let start = comment_span.start() + offset;

    Diagnostic::note(format!("remaining `{keyword}` item found"))
        .with_rule(ID)
        .with_highlight(Span::new(start, matched.len()))
        .with_fix(format!(
            "remove the `{keyword}` item once it has been implemented"
        ))
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
    fn reset(&mut self) {
        *self = Self {
            keywords: std::mem::take(&mut self.keywords),
        };
    }

    fn comment(&mut self, diagnostics: &mut Diagnostics, comment: &Comment) {
        for keyword in &self.keywords {
            for (offset, pattern) in comment.text().match_indices(keyword.as_str()) {
                diagnostics.exceptable_add(
                    todo_comment(keyword, pattern, comment.span(), offset),
                    SyntaxElement::from(comment.inner().clone()),
                    &self.exceptable_nodes(),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use wdl_ast::AstNode as _;
    use wdl_ast::Document;

    use super::*;

    #[test]
    fn configured_keyword_emits_diagnostic() {
        // SAFETY: the static configuration uses a known rule and valid keyword.
        let config = toml_spanner::from_str("[TodoComment]\nkeywords = [\"FIXME\"]\n").unwrap();
        let mut rule = TodoCommentRule::new(&config);
        let (document, parse_diagnostics) = Document::parse(
            "version 1.2\n\n# FIXME: finish this\nworkflow test {}\n",
            None,
        );
        assert!(parse_diagnostics.is_empty());
        // SAFETY: the parsed source contains one comment token.
        let comment = document
            .inner()
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .find_map(Comment::cast)
            .unwrap();
        let mut diagnostics = Diagnostics::default();

        rule.comment(&mut diagnostics, &comment);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics.as_mut_slice()[0].rule(), Some(ID));
    }
}
