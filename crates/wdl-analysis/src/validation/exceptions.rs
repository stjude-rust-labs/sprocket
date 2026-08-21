//! Validation of known rule names.

use std::collections::HashMap;

use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::Directive;
use wdl_grammar::Diagnostic;
use wdl_grammar::Severity;
use wdl_grammar::Span;
use wdl_grammar::SupportedVersion;
use wdl_grammar::SyntaxElement;
use wdl_grammar::SyntaxKind;
use wdl_grammar::SyntaxNode;

use crate::Diagnostics;
use crate::Document;
use crate::ExceptDirectiveValidRule;
use crate::KnownRulesRule;
use crate::VisitReason;
use crate::Visitor;
use crate::find_nearest_rule;

/// Creates a "misplaced directive" diagnostic.
fn misplaced_except_directive(
    id: &str,
    span: Span,
    wrong_element: &SyntaxNode,
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
    .with_rule(ExceptDirectiveValidRule::ID)
    .with_label("cannot make an exception for this rule", span)
    .with_fix(format!(
        "valid locations for this directive are above: {locations}"
    ))
}

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

/// Detects unknown rules within lint directives.
struct KnownRules(Option<Severity>);

/// Detects improperly placed lint directives.
struct ExceptDirectiveValid(Option<Severity>);

/// A visitor that ensures well-formed lint exceptions.
pub struct Exceptions {
    /// The rules that the validator is aware of.
    rules: HashMap<String, Option<&'static [SyntaxKind]>>,
    /// The `KnownRules` rule handler.
    known_rules: KnownRules,
    /// The `ExceptDirectiveValid` rule handler.
    except_directive_valid: ExceptDirectiveValid,
}

impl Exceptions {
    /// Create a new `Exceptions` visitor with a set of known rules.
    pub fn new(rules: HashMap<String, Option<&'static [SyntaxKind]>>) -> Self {
        Self {
            rules,
            known_rules: KnownRules(None),
            except_directive_valid: ExceptDirectiveValid(None),
        }
    }

    /// Gets the set of known rules.
    pub fn known_rules(&self) -> &HashMap<String, Option<&'static [SyntaxKind]>> {
        &self.rules
    }

    /// Adds rule names to the known rules set.
    pub fn extend_rules(
        &mut self,
        rules: impl IntoIterator<Item = (String, Option<&'static [SyntaxKind]>)>,
    ) {
        self.rules.extend(rules);
    }
}

impl Visitor for Exceptions {
    fn reset(&mut self) {
        let Self {
            rules: _,
            known_rules,
            except_directive_valid,
        } = self;

        known_rules.0 = None;
        except_directive_valid.0 = None;
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

        self.known_rules.0 = doc.config().diagnostics_config().known_rules;
        self.except_directive_valid.0 = doc.config().diagnostics_config().except_directive_valid;
    }

    fn comment(&mut self, diagnostics: &mut Diagnostics, comment: &Comment) {
        if self.known_rules.0.is_none() && self.except_directive_valid.0.is_none() {
            return;
        }

        let Some(Directive::Except(except)) = comment.directive() else {
            return;
        };

        let start: usize = comment.span().start();

        let excepted_element = comment
            .inner()
            .siblings_with_tokens(rowan::Direction::Next)
            .find_map(|s| {
                if let SyntaxElement::Node(node) = s {
                    Some(node)
                } else {
                    None
                }
            });

        for rule in except {
            if let Some(exceptable_nodes) = self.rules.get(&rule.name) {
                let Some(exceptable_nodes) = exceptable_nodes else {
                    continue; // `None` means exceptable on any node
                };

                if let Some(severity) = self.except_directive_valid.0
                    && let Some(excepted_element) = &excepted_element
                    && !exceptable_nodes.contains(&excepted_element.kind())
                {
                    let diagnostic = misplaced_except_directive(
                        &rule.name,
                        Span::new(
                            start + comment.text().find(&rule.name).unwrap(),
                            rule.name.len(),
                        ),
                        excepted_element,
                        exceptable_nodes,
                    )
                    .with_severity(severity);

                    diagnostics.exceptable_add(
                        diagnostic,
                        excepted_element,
                        &ExceptDirectiveValidRule::EXCEPTABLE_NODES,
                    );
                }
            } else if let Some(severity) = self.known_rules.0 {
                let diagnostic = unknown_rule(
                    &rule.name,
                    find_nearest_rule(self.rules.keys().map(String::as_str), &rule.name),
                    rule.span,
                )
                .with_severity(severity);

                match excepted_element.as_ref() {
                    None => diagnostics.add(diagnostic),
                    Some(target) => diagnostics.exceptable_add(
                        diagnostic,
                        target,
                        &KnownRulesRule::EXCEPTABLE_NODES,
                    ),
                }
            }
        }
    }
}
