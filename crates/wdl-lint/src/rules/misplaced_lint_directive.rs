//! A lint rule for flagging misplaced lint directives.

use std::collections::HashMap;
use std::sync::LazyLock;

use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::EXCEPT_COMMENT_PREFIX;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;

use crate::Rule;
use crate::Tag;
use crate::TagSet;
use crate::optional_rules;
use crate::rules;

/// The identifier for the unknown rule rule.
const ID: &str = "MisplacedLintDirective";

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
        // insert optional rules as well
        for rule in optional_rules() {
            map.insert(rule.id(), rule.exceptable_nodes());
        }
        map
    });

/// Detects unknown rules within lint directives.
#[derive(Default, Debug, Clone, Copy)]
pub struct MisplacedLintDirectiveRule;

impl Rule for MisplacedLintDirectiveRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Flags misplaced lint directives which will have no effect."
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

impl Visitor for MisplacedLintDirectiveRule {
    type State = Diagnostics;

    fn document(&mut self, _: &mut Self::State, _: VisitReason, _: &Document, _: SupportedVersion) {
        // This is intentionally empty, as this rule has no state.
    }

    fn comment(&mut self, state: &mut Self::State, comment: &Comment) {
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

                if let Some(elem) = &excepted_element {
                    if let Some(Some(exceptable_nodes)) = RULE_MAP.get(trimmed) {
                        if !exceptable_nodes.contains(&elem.kind()) {
                            state.add(misplaced_lint_directive(
                                trimmed,
                                Span::new(start + offset, trimmed.len()),
                                elem,
                                exceptable_nodes,
                            ));
                        }
                    }
                }

                // Update the offset to account for the rule id and comma
                offset += trimmed.len() + 1;
            }
        }
    }
}
