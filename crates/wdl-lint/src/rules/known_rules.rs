//! A lint rule for flagging unknown rules in lint directives.

use std::collections::HashSet;
use std::sync::LazyLock;

use wdl_analysis::Diagnostics;
use wdl_analysis::EXCEPT_COMMENT_PREFIX;
use wdl_analysis::Visitor;
use wdl_analysis::rules as analysis_rules;
use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxKind;

use crate::Rule;
use crate::Tag;
use crate::TagSet;
use crate::rules::RULE_MAP;
use crate::util::find_nearest_rule;

/// A set of known analysis rules.
static ANALYSIS_RULES: LazyLock<HashSet<String>> = LazyLock::new(|| {
    analysis_rules()
        .iter()
        .map(|r| r.id().to_string())
        .collect()
});

/// The identifier for the unknown rule rule.
const ID: &str = "KnownRules";

/// Creates an "unknown rule" diagnostic.
fn unknown_rule(id: &str, span: Span) -> Diagnostic {
    let mut diagnostic = Diagnostic::note(format!("unknown lint rule `{id}`"))
        .with_rule(ID)
        .with_label("cannot make an exception for this rule", span);

    // Find the nearest rule to suggest
    if let Some(nearest_rule) = find_nearest_rule(id) {
        diagnostic = diagnostic.with_fix(format!("did you mean `{nearest_rule}`?"));
    } else {
        diagnostic = diagnostic.with_fix("remove the unknown rule from the exception list");
    }

    diagnostic
}

/// Detects unknown rules within lint directives.
#[derive(Default, Debug, Clone, Copy)]
pub struct KnownRulesRule;

impl Rule for KnownRulesRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures only known rules are used in lint directives."
    }

    fn explanation(&self) -> &'static str {
        "When writing WDL, lint directives are used to suppress certain rules. If a rule is \
         unknown, nothing will be suppressed. This rule flags unknown rules as they are often \
         mistakes."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Clarity, Tag::Correctness])
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        Some(&[SyntaxKind::VersionStatementNode])
    }

    fn related_rules(&self) -> &[&'static str] {
        &[]
    }
}

impl Visitor for KnownRulesRule {
    fn reset(&mut self) {
        *self = Self;
    }

    fn comment(&mut self, diagnostics: &mut Diagnostics, comment: &Comment) {
        if let Some(ids) = comment.text().strip_prefix(EXCEPT_COMMENT_PREFIX) {
            let start: usize = comment.span().start();
            let mut offset = EXCEPT_COMMENT_PREFIX.len();
            for id in ids.split(',') {
                // First trim the start so we can determine how much whitespace was removed
                let trimmed_start = id.trim_start();
                // Next trim the end
                let trimmed: &str = trimmed_start.trim_end();

                // Update the offset to account for the whitespace that was removed
                offset += id.len() - trimmed.len();

                // Check if the rule is known
                if !ANALYSIS_RULES.contains(trimmed) && !RULE_MAP.contains_key(&trimmed) {
                    // Since this rule can only be excepted in a document-wide fashion,
                    // if the rule is running we can directly add the diagnostic
                    // without checking for the exceptable nodes
                    diagnostics.add(unknown_rule(
                        trimmed,
                        Span::new(start + offset, trimmed.len()),
                    ));
                }

                // Update the offset to account for the rule id and comma
                offset += trimmed.len() + 1;
            }
        }
    }
}
