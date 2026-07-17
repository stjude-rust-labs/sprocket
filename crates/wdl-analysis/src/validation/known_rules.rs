//! Validation of known rule names.

use std::collections::HashSet;

use rowan::Direction;
use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::Directive;
use wdl_grammar::Diagnostic;
use wdl_grammar::Severity;
use wdl_grammar::Span;
use wdl_grammar::SupportedVersion;
use wdl_grammar::SyntaxElement;

use crate::Diagnostics;
use crate::Document;
use crate::KnownRulesRule;
use crate::VisitReason;
use crate::Visitor;
use crate::find_nearest_rule;
use crate::replacement_rule_id;

/// Creates an "unknown rule" diagnostic.
fn unknown_rule(id: &str, nearest_rule: Option<String>, span: Span) -> Diagnostic {
    let mut diagnostic = Diagnostic::note(format!("unknown rule `{id}`"))
        .with_rule(KnownRulesRule::ID)
        .with_label("cannot make an exception for this rule", span);

    if let Some(nearest_rule) = nearest_rule {
        diagnostic = diagnostic.with_fix(format!("did you mean `{nearest_rule}`?"));
    } else {
        diagnostic = diagnostic.with_fix("remove the unknown rule from the exception list");
    }

    diagnostic
}

/// Creates a diagnostic for a deprecated rule alias.
fn deprecated_rule_alias(alias: &str, replacement: &str, span: Span) -> Diagnostic {
    Diagnostic::note(format!(
        "deprecated rule `{alias}`; replace it with `{replacement}`"
    ))
    .with_rule(KnownRulesRule::ID)
    .with_label("update this `except` directive", span)
    .with_fix(format!("replace `{alias}` with `{replacement}`"))
}

/// Detects unknown rules within lint directives.
pub(crate) struct KnownRules {
    /// All rules known to the validator.
    known_rules: HashSet<String>,
    /// The severity of the diagnostic.
    severity: Option<Severity>,
}

impl KnownRules {
    /// Creates a new instance of the `KnownRules` rule.
    pub fn new(known_rules: HashSet<String>) -> Self {
        Self {
            known_rules,
            severity: None,
        }
    }

    /// Gets the set of known rules.
    pub fn known_rules(&self) -> &HashSet<String> {
        &self.known_rules
    }
}

impl Extend<String> for KnownRules {
    fn extend<T: IntoIterator<Item = String>>(&mut self, iter: T) {
        self.known_rules.extend(iter);
    }
}

impl Visitor for KnownRules {
    fn reset(&mut self) {
        let Self {
            severity,
            known_rules: _,
        } = self;

        *severity = None;
    }

    fn document(
        &mut self,
        _: &mut Diagnostics,
        visit_reason: VisitReason,
        doc: &Document,
        _: SupportedVersion,
    ) {
        if visit_reason != VisitReason::Enter {
            return;
        }

        self.severity = doc.config().diagnostics_config().known_rules;
    }

    fn comment(&mut self, diagnostics: &mut Diagnostics, comment: &Comment) {
        if self.severity.is_none() {
            return;
        }

        let Some(Directive::Except(except)) = comment.directive() else {
            return;
        };

        for rule in except {
            // Deprecated aliases are not "unknown"; instead, emit a migration
            // note that points at the current replacement rule ID.
            if let Some(replacement) = replacement_rule_id(&rule.name) {
                let diagnostic = deprecated_rule_alias(&rule.name, replacement, rule.span);

                if let Some(target) = comment
                    .inner()
                    .siblings_with_tokens(Direction::Next)
                    .find_map(|sibling| {
                        if let SyntaxElement::Node(node) = sibling {
                            Some(node)
                        } else {
                            None
                        }
                    })
                {
                    diagnostics.exceptable_add(
                        diagnostic,
                        &target,
                        &KnownRulesRule::EXCEPTABLE_NODES,
                    );
                } else {
                    diagnostics.add(diagnostic);
                }

                continue;
            }

            if self.known_rules.contains(&rule.name) {
                continue;
            }

            let diagnostic = unknown_rule(
                &rule.name,
                find_nearest_rule(self.known_rules.iter().map(String::as_str), &rule.name),
                rule.span,
            );

            if let Some(target) = comment
                .inner()
                .siblings_with_tokens(Direction::Next)
                .find_map(|sibling| {
                    if let SyntaxElement::Node(node) = sibling {
                        Some(node)
                    } else {
                        None
                    }
                })
            {
                diagnostics.exceptable_add(diagnostic, &target, &KnownRulesRule::EXCEPTABLE_NODES);
            } else {
                diagnostics.add(diagnostic);
            }
        }
    }
}
