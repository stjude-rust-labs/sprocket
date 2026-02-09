//! A lint rule for flagging misplaced except directives.

use std::collections::HashMap;
use std::sync::LazyLock;

use wdl_analysis::Diagnostics;
use wdl_analysis::Visitor;
use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;

use crate::Config;
use crate::Rule;
use crate::Tag;
use crate::TagSet;
use crate::rules;

/// The identifier for the except directive valid rule.
const ID: &str = "ExceptDirectiveValid";

/// Creates a "misplaced directive" diagnostic.
fn misplaced_except_directive(
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
        "`except` directive `{id}` has no effect above {elem}",
        elem = wrong_element.kind().describe()
    ))
    .with_rule(ID)
    .with_label("cannot make an exception for this rule", span)
    .with_label(
        "invalid element for this `except` directive",
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
        for rule in rules(&Config::default()) {
            map.insert(rule.id(), rule.exceptable_nodes());
        }
        map
    });

/// Detects unknown rules within lint directives.
#[derive(Default, Debug, Clone, Copy)]
pub struct ExceptDirectiveValidRule;

impl Rule for ExceptDirectiveValidRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn version(&self) -> &'static str {
        "0.6.0"
    }

    fn description(&self) -> &'static str {
        "Ensures `except` directives are placed correctly to have the intended effect."
    }

    fn explanation(&self) -> &'static str {
        "When writing WDL, `except` directives are used to suppress certain rules. If an `except` \
         directive is misplaced, it will have no effect. This rule flags misplaced `except` \
         directives to ensure they are in the correct location."
    }

    fn examples(&self) -> &'static [&'static str] {
        &[
            r#"```wdl
version 1.2

workflow example {
    meta {}

    output {
        # MatchingOutputMeta exceptions aren't valid
        # in this context
        #@ except: MatchingOutputMeta
        String name = "Jimmy"
    }
}
```"#,
            r#"Use instead:
version 1.2

#@ except: MatchingOutputMeta
workflow example {
    meta {}

    output {
        String name = "Jimmy"
    }
}
```"#,
        ]
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Clarity, Tag::Correctness, Tag::SprocketCompatibility])
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        Some(&[SyntaxKind::VersionStatementNode])
    }

    fn related_rules(&self) -> &[&'static str] {
        &[]
    }
}

impl Visitor for ExceptDirectiveValidRule {
    fn reset(&mut self) {
        *self = Self;
    }

    fn comment(&mut self, diagnostics: &mut Diagnostics, comment: &Comment) {
        if let Some(wdl_ast::Directive::Except(ids)) = comment.directive() {
            let start: usize = comment.span().start();

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

            for id in ids {
                if let Some(elem) = &excepted_element
                    && let Some(Some(exceptable_nodes)) = RULE_MAP.get(id.as_str())
                    && !exceptable_nodes.contains(&elem.kind())
                {
                    diagnostics.add(misplaced_except_directive(
                        &id,
                        Span::new(start + comment.text().find(&id).unwrap(), id.len()),
                        elem,
                        exceptable_nodes,
                    ));
                }
            }
        }
    }
}
