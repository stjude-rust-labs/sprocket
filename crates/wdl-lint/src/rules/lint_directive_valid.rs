//! A lint rule for flagging misplaced lint directives.

use std::collections::HashMap;
use std::sync::LazyLock;

use wdl_analysis::Diagnostics;
use wdl_analysis::EXCEPT_COMMENT_PREFIX;
use wdl_analysis::Visitor;
use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;

use crate::Rule;
use crate::Tag;
use crate::TagSet;
use crate::rules;

/// The identifier for the unknown rule rule.
const ID: &str = "LintDirectiveValid";

/// Creates an "unknown rule" diagnostic.
fn misplaced_lint_directive(
    id: &str,
    span: Span,
    wrong_element: &SyntaxElement,
    exceptable_nodes: &[SyntaxKind],
) -> Diagnostic {
    let locations = exceptable_nodes
        .iter()
        .map(|node| node.describe())
        .collect::<Vec<_>>()
        .join(", ");

    Diagnostic::note(format!(
        "lint directive `{id}` has no effect above {elem}",
        elem = wrong_element.kind().describe()
    ))
    .with_rule(ID)
    .with_label("cannot make an exception for this rule", span)
    .with_label(
        "invalid element for this lint directive",
        wrong_element.text_range(),
    )
    .with_fix(format!(
        "valid locations for this directive are above: {locations}"
    ))
}

/// Creates a static LazyLock of the rules' excepatable nodes.
pub static RULE_MAP: LazyLock<HashMap<&'static str, Option<&'static [SyntaxKind]>>> =
    LazyLock::new(|| {
        let mut map = HashMap::new();
        for rule in rules() {
            map.insert(rule.id(), rule.exceptable_nodes());
        }
        map
    });

/// Detects unknown rules within lint directives.
#[derive(Default, Debug, Clone, Copy)]
pub struct LintDirectiveValidRule;

impl Rule for LintDirectiveValidRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures lint directives are placed correctly to have the intended effect."
    }

    fn explanation(&self) -> &'static str {
        "When writing WDL, lint directives are used to suppress certain rules. If a lint directive \
         is misplaced, it will have no effect. This rule flags misplaced lint directives to ensure \
         they are in the correct location."
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

impl Visitor for LintDirectiveValidRule {
    fn reset(&mut self) {
        *self = Self;
    }

    fn comment(&mut self, diagnostics: &mut Diagnostics, comment: &Comment) {
        if let Some(ids) = comment.text().strip_prefix(EXCEPT_COMMENT_PREFIX) {
            let start: usize = comment.span().start();
            let mut offset = EXCEPT_COMMENT_PREFIX.len();

            let excepted_element = comment
                .inner()
                .siblings_with_tokens(rowan::Direction::Next)
                .find_map(|s| {
                    if s.kind() == SyntaxKind::Whitespace || s.kind() == SyntaxKind::Comment {
                        None
                    } else {
                        Some(s)
                    }
                });

            for id in ids.split(',') {
                // First trim the start so we can determine how much whitespace was removed
                let trimmed_start = id.trim_start();
                // Next trim the end
                let trimmed: &str = trimmed_start.trim_end();

                // Update the offset to account for the whitespace that was removed
                offset += id.len() - trimmed.len();

                if let Some(elem) = &excepted_element
                    && let Some(Some(exceptable_nodes)) = RULE_MAP.get(trimmed)
                    && !exceptable_nodes.contains(&elem.kind())
                {
                    diagnostics.add(misplaced_lint_directive(
                        trimmed,
                        Span::new(start + offset, trimmed.len()),
                        elem,
                        exceptable_nodes,
                    ));
                }

                // Update the offset to account for the rule id and comma
                offset += trimmed.len() + 1;
            }
        }
    }
}
