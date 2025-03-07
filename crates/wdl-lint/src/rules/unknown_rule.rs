//! A lint rule for flagging unknown rules in lint directives.

use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::EXCEPT_COMMENT_PREFIX;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;

use crate::RESERVED_RULE_IDS;
use crate::Rule;
use crate::Tag;
use crate::TagSet;
use crate::rules::RULE_MAP;
use crate::util::find_nearest_rule;

/// The identifier for the unknown rule rule.
const ID: &str = "UnknownRule";

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
pub struct UnknownRule;

impl Rule for UnknownRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Flags unknown rules in lint directives."
    }

    fn explanation(&self) -> &'static str {
        "When writing WDL, lint directives are used to suppress certain rules. If a rule is \
         unknown, nothing will be suppressed. This rule flags unknown rules as they are often \
         mistakes."
    }

    fn tags(&self) -> TagSet {
        // TODO: Is there another tag that would be appropriate?
        TagSet::new(&[Tag::Clarity])
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        Some(&[SyntaxKind::VersionStatementNode])
    }
}

impl Visitor for UnknownRule {
    type State = Diagnostics;

    fn document(&mut self, _: &mut Self::State, _: VisitReason, _: &Document, _: SupportedVersion) {
        // This is intentionally empty, as this rule has no state.
    }

    fn comment(&mut self, state: &mut Self::State, comment: &Comment) {
        if let Some(ids) = comment.as_str().strip_prefix(EXCEPT_COMMENT_PREFIX) {
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
                if !RESERVED_RULE_IDS.contains(&trimmed) && !RULE_MAP.contains_key(&trimmed) {
                    // Since this rule can only be excepted in a document-wide fashion,
                    // if the rule is running we can directly add the diagnostic
                    // without checking for the exceptable nodes
                    state.add(unknown_rule(
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
