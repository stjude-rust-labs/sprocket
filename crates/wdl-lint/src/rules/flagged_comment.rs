//! A lint rule for flagging keywords in comments.

use wdl_analysis::Diagnostics;
use wdl_analysis::Example;
use wdl_analysis::LabeledSnippet;
use wdl_analysis::Visitor;
use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::CommentKind;
use wdl_ast::Diagnostic;
use wdl_ast::Severity;
use wdl_ast::Span;
use wdl_ast::TreeToken;

use crate::Config;
use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the flagged comment rule.
const ID: &str = "FlaggedComment";

/// A keyword found in a comment.
#[derive(Debug, PartialEq, Eq)]
struct Flagged<'a> {
    /// The byte offset of the match within the comment text.
    offset: usize,
    /// The configured keyword that matched.
    keyword: &'a str,
}

/// Finds the keywords within the given comment text.
///
/// Matching is case-sensitive. The `keywords` must be sorted from longest to
/// shortest so that, when keywords overlap (e.g. `FIX` and `FIXME`), the
/// longest one is reported and the overlapping match is not reported again.
///
/// The results are ordered by their position in the text.
fn find_flagged<'a>(text: &str, keywords: &'a [String]) -> Vec<Flagged<'a>> {
    let mut found: Vec<Flagged<'a>> = Vec::new();
    for keyword in keywords {
        for (offset, _) in text.match_indices(keyword.as_str()) {
            let end = offset + keyword.len();
            if found
                .iter()
                .any(|f| offset < f.offset + f.keyword.len() && f.offset < end)
            {
                continue;
            }

            found.push(Flagged { offset, keyword });
        }
    }

    found.sort_by_key(|f| f.offset);
    found
}

/// Detects remaining flagged keywords (e.g. `TODO`) within comments.
#[derive(Debug, Clone)]
pub struct FlaggedCommentRule {
    /// The severity of the rule's diagnostics.
    severity: Severity,
    /// The keywords to flag, sorted from longest to shortest without
    /// duplicates.
    keywords: Vec<String>,
}

/// Creates a "flagged comment" diagnostic.
fn flagged_comment(
    severity: Severity,
    keyword: &str,
    comment_span: Span,
    offset: usize,
) -> Diagnostic {
    let start = comment_span.start() + offset;

    Diagnostic::new(severity, format!("remaining `{keyword}` item found"))
        .with_rule(ID)
        .with_highlight(Span::new(start, keyword.len()))
        .with_fix(format!(
            "remove the `{keyword}` item once it has been implemented"
        ))
}

impl FlaggedCommentRule {
    /// Creates a new instance of the rule.
    pub fn new(config: &Config) -> Self {
        // Empty keywords would match everywhere and are rejected when the
        // configuration is loaded, but they are skipped here as well for
        // configurations built programmatically.
        let mut keywords: Vec<String> = config
            .flagged_comment
            .keywords
            .iter()
            .filter(|k| !k.trim().is_empty())
            .cloned()
            .collect();
        keywords.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        keywords.dedup();

        Self {
            severity: config.flagged_comment.diagnostic_severity(),
            keywords,
        }
    }
}

impl Rule for FlaggedCommentRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Flags keywords in comments (by default, `TODO`) to ensure they are not forgotten."
    }

    fn explanation(&self) -> &'static str {
        "When writing WDL, future tasks are often marked with a keyword like `TODO`. This \
         indicates that the implementor intended to go back to the code and handle the item. \
         Flagged items should not be long-term fixtures within code and, as such, comments \
         containing them are flagged to ensure none are forgotten. The keywords to flag are \
         configurable with the `keywords` option and default to `TODO`. Keywords are matched as \
         case-sensitive substrings of the comment text. If keywords overlap (e.g., `FIX` and \
         `FIXME`), only the longest is reported."
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

impl Visitor for FlaggedCommentRule {
    fn reset(&mut self) {
        *self = Self {
            severity: self.severity,
            keywords: std::mem::take(&mut self.keywords),
        };
    }

    fn comment(&mut self, diagnostics: &mut Diagnostics, comment: &Comment) {
        if comment.kind() != CommentKind::Line {
            // Ignore doc comments and directives. Flagged words in those
            // contexts are likely intentional.
            return;
        }

        for flagged in find_flagged(comment.text(), &self.keywords) {
            diagnostics.exceptable_add(
                flagged_comment(
                    self.severity,
                    flagged.keyword,
                    comment.span(),
                    flagged.offset,
                ),
                &TreeToken::parent(comment.inner()),
                &self.exceptable_nodes(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sorts keywords as the rule does.
    fn keywords(keywords: &[&str]) -> Vec<String> {
        let mut config = Config::default();
        config.flagged_comment.keywords = keywords.iter().map(|k| k.to_string()).collect();
        FlaggedCommentRule::new(&config).keywords
    }

    /// Finds the flagged `(offset, keyword)` pairs in `text`.
    fn find(text: &str, configured: &[&str]) -> Vec<(usize, String)> {
        let configured = keywords(configured);
        find_flagged(text, &configured)
            .into_iter()
            .map(|f| (f.offset, f.keyword.to_string()))
            .collect()
    }

    #[test]
    fn defaults_to_todo() {
        let rule = FlaggedCommentRule::new(&Config::default());
        assert_eq!(rule.keywords, ["TODO"]);
        assert_eq!(rule.severity, Severity::Note);
    }

    #[test]
    fn matches_at_start_middle_and_end() {
        assert_eq!(find("TODO first", &["TODO"]), [(0, String::from("TODO"))]);
        assert_eq!(
            find("a TODO middle", &["TODO"]),
            [(2, String::from("TODO"))]
        );
        assert_eq!(
            find("at the end TODO", &["TODO"]),
            [(11, String::from("TODO"))]
        );
    }

    #[test]
    fn matching_is_case_sensitive() {
        assert!(find("todo Todo tOdO", &["TODO"]).is_empty());
    }

    #[test]
    fn finds_multiple_keywords_in_order() {
        assert_eq!(
            find("XXX then TODO then XXX", &["TODO", "XXX"]),
            [
                (0, String::from("XXX")),
                (9, String::from("TODO")),
                (19, String::from("XXX"))
            ]
        );
    }

    #[test]
    fn longer_keywords_take_priority() {
        assert_eq!(
            keywords(&["FIX", "FIXME", "FIX", "TODO"]),
            ["FIXME", "TODO", "FIX"]
        );
        assert_eq!(
            find("FIXME and FIX", &["FIX", "FIXME"]),
            [(0, String::from("FIXME")), (10, String::from("FIX"))]
        );
    }

    #[test]
    fn overlapping_matches_are_reported_once() {
        assert_eq!(
            find("FIXME", &["FIX", "FIXME"]),
            [(0, String::from("FIXME"))]
        );

        // The longest keyword wins even when a shorter one starts earlier.
        assert_eq!(find("ABXQ", &["AB", "BXQ"]), [(1, String::from("BXQ"))]);
    }

    #[test]
    fn skips_blank_keywords() {
        assert_eq!(keywords(&["", " ", "TODO"]), ["TODO"]);
        assert!(find("anything", &[""]).is_empty());
    }

    #[test]
    fn handles_multibyte_text() {
        assert_eq!(find("é TODO ü", &["TODO"]), [(3, String::from("TODO"))]);
        assert_eq!(
            find("¡TÓDO! ¿ÑANDÚ?", &["ÑANDÚ"]),
            [(11, String::from("ÑANDÚ"))]
        );
    }
}
