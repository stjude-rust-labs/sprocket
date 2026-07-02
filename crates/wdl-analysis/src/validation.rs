//! Validator for WDL documents.

use std::collections::HashMap;
use std::collections::HashSet;

use rowan::Direction;
use strsim::levenshtein;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::Diagnostic;
use wdl_ast::Directive;
use wdl_ast::ExceptRule;
use wdl_ast::SupportedVersion;
use wdl_ast::TreeNode;
use wdl_ast::VersionStatement;
use wdl_ast::Whitespace;
use wdl_ast::v1;
use wdl_grammar::Severity;
use wdl_grammar::Span;
use wdl_grammar::SyntaxElement;
use wdl_grammar::SyntaxKind;

use crate::ALL_RULE_IDS;
use crate::Config;
use crate::Exceptable;
use crate::KnownRulesRule;
use crate::VisitReason;
use crate::Visitor;
use crate::diagnostics::meaningless_lint_directive;
use crate::document::Document;

mod counts;
mod env;
mod exprs;
mod imports;
mod keys;
mod numbers;
mod requirements;
mod strings;
mod version;

/// Finds the nearest known rule ID to the given unknown rule ID,
/// or `None` if no rule ID is close enough.
pub fn find_nearest_rule<'a>(
    known_rules: impl IntoIterator<Item = &'a str>,
    unknown_rule_id: &str,
) -> Option<String> {
    let threshold = if unknown_rule_id.len() <= 3 {
        1
    } else if unknown_rule_id.len() <= 10 {
        unknown_rule_id.len() / 3 + 1
    } else {
        5
    };

    known_rules
        .into_iter()
        .map(|rule_id| (rule_id, levenshtein(unknown_rule_id, rule_id)))
        .filter(|(_, distance)| *distance <= threshold)
        .min_by_key(|(_, distance)| *distance)
        .map(|(rule_id, _)| rule_id.to_string())
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

/// Represents a collection of validation diagnostics.
///
/// Validation visitors receive a diagnostics collection during
/// visitation of the AST.
#[derive(Clone, Debug, Default)]
pub struct Diagnostics {
    /// Diagnostics to emit.
    pub(crate) diagnostics: Vec<Diagnostic>,
    /// `#@ except:` directives discovered during traversal.
    ///
    /// `HashMap<Rule, applied>`
    exceptions: HashMap<ExceptRule, bool>,
}

impl Diagnostics {
    /// Adds a diagnostic to the collection.
    ///
    /// NOTE: This is intended for diagnostics that cannot be suppressed.
    /// Otherwise, [`Diagnostics::exceptable_add()`] should be used.
    pub fn add(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Adds rule exceptions to the collection.
    pub fn add_exceptions(&mut self, exceptions: impl IntoIterator<Item = ExceptRule>) {
        for e in exceptions {
            self.exceptions.entry(e).or_insert(false);
        }
    }

    /// Adds a diagnostic to the collection, unless the diagnostic is for an
    /// element that has an exception for the given rule.
    ///
    /// If the diagnostic does not have a rule, the diagnostic is always added.
    pub fn exceptable_add<N: TreeNode + Exceptable>(
        &mut self,
        diagnostic: Diagnostic,
        element: &N,
        exceptable_nodes: &Option<&'static [SyntaxKind]>,
    ) {
        let Some(target_rule) = diagnostic.rule() else {
            self.add(diagnostic);
            return;
        };

        for node in element.ancestors().filter(|node| {
            exceptable_nodes
                .as_ref()
                .is_none_or(|nodes| nodes.contains(&node.kind()))
        }) {
            let mut rule_excepted = false;
            for rule in node
                .rule_exceptions()
                .into_iter()
                .filter(|rule| rule.name == target_rule)
            {
                rule_excepted = true;
                self.exceptions
                    .entry(rule)
                    .and_modify(|applied| *applied = true);
            }

            if rule_excepted {
                return;
            }
        }

        self.add(diagnostic);
    }

    /// Returns whether the collection is empty.
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Sorts the diagnostics in the collection.
    pub fn sort(&mut self) {
        self.diagnostics.sort();
    }

    /// Iterate the diagnostics emitted so far.
    pub fn iter(&self) -> std::slice::Iter<'_, Diagnostic> {
        self.diagnostics.iter()
    }
}

impl Extend<Diagnostic> for Diagnostics {
    fn extend<I: IntoIterator<Item = Diagnostic>>(&mut self, iter: I) {
        self.diagnostics.extend(iter);
    }
}

impl IntoIterator for Diagnostics {
    type IntoIter = std::vec::IntoIter<Self::Item>;
    type Item = Diagnostic;

    fn into_iter(self) -> Self::IntoIter {
        self.diagnostics.into_iter()
    }
}

impl From<Diagnostics> for Vec<Diagnostic> {
    fn from(input: Diagnostics) -> Self {
        input.diagnostics
    }
}

/// Implements an AST validator.
///
/// A validator operates on a set of AST visitors.
///
/// See the [validate](Self::validate) method to perform the validation.
#[allow(missing_debug_implementations)]
pub struct Validator {
    /// The set of validation visitors.
    visitors: Vec<Box<dyn Visitor>>,
    /// All rules known by the visitors.
    known_rules: HashSet<String>,
}

impl Validator {
    /// Creates a validator with an empty visitors set.
    pub fn empty() -> Self {
        Self {
            visitors: Vec::new(),
            // Analysis rules are always known
            known_rules: ALL_RULE_IDS.iter().cloned().collect(),
        }
    }

    /// Adds a visitor to the validator.
    pub fn add_visitor<V: Visitor + 'static>(&mut self, visitor: V) {
        self.add_visitors(std::iter::once(Box::new(visitor) as Box<dyn Visitor>));
    }

    /// Adds multiple visitors to the validator.
    pub fn add_visitors(&mut self, visitors: impl IntoIterator<Item = Box<dyn Visitor>>) {
        for visitor in visitors {
            self.known_rules.extend(visitor.known_rules());
            self.visitors.push(visitor);
        }
    }

    /// Adds rule names to the validator's known rules set.
    pub fn extend_known_rules(&mut self, rules: impl IntoIterator<Item = String>) {
        self.known_rules.extend(rules);
    }

    /// Catch any unapplied lint exceptions.
    ///
    /// When the [`Validator`] is created, it is made aware of all `#@ except`
    /// comments in the document. As it runs, exceptable diagnostics are
    /// passed through [`Diagnostics::exceptable_add()`], which
    /// tracks whether any `#@ except` comment suppresses it and marks the
    /// comment as used.
    ///
    /// Any unmarked comments, with exception to the special cases below, will
    /// be reported as `MeaninglessLintDirective`s.
    fn check_meaningless_lint_directives(&self, diagnostics: &mut Diagnostics, severity: Severity) {
        let mut meaningless_lint_directives = Diagnostics::default();

        let visitor_known_rules = self.known_rules();

        // `ExceptDirectiveValid` does a different job of checking whether a lint
        // exception is *ever* applicable to the applied node.
        // `MeaninglessLintDirective` should only fire if the exception
        // comment is valid to begin with.
        let invalid_directives = diagnostics
            .iter()
            .filter_map(|d| {
                // Unfortunately, somewhat hacky since `ExceptDirectiveValid` comes from
                // `wdl-lint`
                if d.rule() == Some("ExceptDirectiveValid") {
                    d.labels().next()
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        for (exception, applied) in &diagnostics.exceptions {
            if *applied
                // Try not to clash with `ExceptDirectiveValid`
                || invalid_directives.iter().any(|label| label.span() == exception.span)
                // If none of the visitors know the rule, it can't ever fire
                || (!ALL_RULE_IDS.iter().any(|r| r == &exception.name) && !visitor_known_rules.contains(&exception.name))
            {
                continue;
            }

            meaningless_lint_directives.add(meaningless_lint_directive(
                &exception.name,
                exception.span,
                severity,
            ));
        }

        diagnostics.extend(meaningless_lint_directives.diagnostics);
    }

    /// Validates the given document and returns the validation errors upon
    /// failure.
    pub fn validate(&mut self, document: &Document, config: &Config) -> Result<(), Diagnostics> {
        let mut diagnostics = Diagnostics {
            exceptions: document.analysis_diagnostics().exceptions.clone(),
            ..Default::default()
        };

        self.register(config);
        document.visit(&mut diagnostics, self);

        if let Some(severity) = document
            .config()
            .diagnostics_config()
            .meaningless_lint_directive
        {
            self.check_meaningless_lint_directives(&mut diagnostics, severity);
        }

        self.reset();

        if diagnostics.is_empty() {
            Ok(())
        } else {
            diagnostics.sort();
            Err(diagnostics)
        }
    }

    /// Finds the nearest known rule ID to the given unknown rule ID,
    /// or `None` if no rule ID is close enough.
    pub fn find_nearest_rule(&self, unknown_rule_id: &str) -> Option<String> {
        find_nearest_rule(self.known_rules.iter().map(String::as_str), unknown_rule_id)
    }
}

impl Default for Validator {
    /// Creates a validator with the default validation visitors.
    fn default() -> Self {
        let mut validator = Self::empty();
        validator.add_visitors([
            Box::new(strings::LiteralTextVisitor) as Box<dyn Visitor>,
            Box::<counts::CountingVisitor>::default(),
            Box::<keys::UniqueKeysVisitor>::default(),
            Box::<numbers::NumberVisitor>::default(),
            Box::<version::VersionVisitor>::default(),
            Box::<requirements::RequirementsVisitor>::default(),
            Box::<exprs::ScopedExprVisitor>::default(),
            Box::<imports::ImportsVisitor>::default(),
            Box::<env::EnvVisitor>::default(),
        ]);
        validator
    }
}

impl Visitor for Validator {
    fn known_rules(&self) -> HashSet<String> {
        let mut known_rules = HashSet::new();
        for visitor in &self.visitors {
            known_rules.extend(visitor.known_rules());
        }
        known_rules
    }

    fn register(&mut self, config: &crate::Config) {
        for visitor in self.visitors.iter_mut() {
            visitor.register(config);
        }
    }

    fn reset(&mut self) {
        for visitor in self.visitors.iter_mut() {
            visitor.reset();
        }
    }

    fn document(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        doc: &Document,
        version: SupportedVersion,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.document(diagnostics, reason, doc, version);
        }
    }

    fn whitespace(&mut self, diagnostics: &mut Diagnostics, whitespace: &Whitespace) {
        for visitor in self.visitors.iter_mut() {
            visitor.whitespace(diagnostics, whitespace);
        }
    }

    fn comment(&mut self, diagnostics: &mut Diagnostics, comment: &Comment) {
        if let Some(Directive::Except(except)) = comment.directive() {
            for rule in except {
                if self.known_rules.contains(&rule.name) {
                    continue;
                }

                let diagnostic =
                    unknown_rule(&rule.name, self.find_nearest_rule(&rule.name), rule.span);

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
            }
        }

        for visitor in self.visitors.iter_mut() {
            visitor.comment(diagnostics, comment);
        }
    }

    fn version_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        stmt: &VersionStatement,
    ) {
        if reason == VisitReason::Enter {
            // Global exceptions are always considered applied
            for (rule, applied) in &mut diagnostics.exceptions {
                if rule.span < stmt.span() {
                    *applied = true;
                }
            }
        }

        for visitor in self.visitors.iter_mut() {
            visitor.version_statement(diagnostics, reason, stmt);
        }
    }

    fn import_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        stmt: &v1::ImportStatement,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.import_statement(diagnostics, reason, stmt);
        }
    }

    fn struct_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        def: &v1::StructDefinition,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.struct_definition(diagnostics, reason, def);
        }
    }

    fn enum_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        def: &v1::EnumDefinition,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.enum_definition(diagnostics, reason, def);
        }
    }

    fn task_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        task: &v1::TaskDefinition,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.task_definition(diagnostics, reason, task);
        }
    }

    fn workflow_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        workflow: &v1::WorkflowDefinition,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.workflow_definition(diagnostics, reason, workflow);
        }
    }

    fn input_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &v1::InputSection,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.input_section(diagnostics, reason, section);
        }
    }

    fn output_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &v1::OutputSection,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.output_section(diagnostics, reason, section);
        }
    }

    fn command_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &v1::CommandSection,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.command_section(diagnostics, reason, section);
        }
    }

    fn command_text(&mut self, diagnostics: &mut Diagnostics, text: &v1::CommandText) {
        for visitor in self.visitors.iter_mut() {
            visitor.command_text(diagnostics, text);
        }
    }

    fn requirements_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &v1::RequirementsSection,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.requirements_section(diagnostics, reason, section);
        }
    }

    fn task_hints_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &v1::TaskHintsSection,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.task_hints_section(diagnostics, reason, section);
        }
    }

    fn workflow_hints_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &v1::WorkflowHintsSection,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.workflow_hints_section(diagnostics, reason, section);
        }
    }

    fn runtime_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &v1::RuntimeSection,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.runtime_section(diagnostics, reason, section);
        }
    }

    fn runtime_item(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        item: &v1::RuntimeItem,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.runtime_item(diagnostics, reason, item);
        }
    }

    fn metadata_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &v1::MetadataSection,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.metadata_section(diagnostics, reason, section);
        }
    }

    fn parameter_metadata_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &v1::ParameterMetadataSection,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.parameter_metadata_section(diagnostics, reason, section);
        }
    }

    fn metadata_object(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        object: &v1::MetadataObject,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.metadata_object(diagnostics, reason, object);
        }
    }

    fn metadata_object_item(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        item: &v1::MetadataObjectItem,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.metadata_object_item(diagnostics, reason, item);
        }
    }

    fn metadata_array(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        item: &v1::MetadataArray,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.metadata_array(diagnostics, reason, item);
        }
    }

    fn unbound_decl(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        decl: &v1::UnboundDecl,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.unbound_decl(diagnostics, reason, decl);
        }
    }

    fn bound_decl(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        decl: &v1::BoundDecl,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.bound_decl(diagnostics, reason, decl);
        }
    }

    fn expr(&mut self, diagnostics: &mut Diagnostics, reason: VisitReason, expr: &v1::Expr) {
        for visitor in self.visitors.iter_mut() {
            visitor.expr(diagnostics, reason, expr);
        }
    }

    fn string_text(&mut self, diagnostics: &mut Diagnostics, text: &v1::StringText) {
        for visitor in self.visitors.iter_mut() {
            visitor.string_text(diagnostics, text);
        }
    }

    fn placeholder(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        placeholder: &v1::Placeholder,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.placeholder(diagnostics, reason, placeholder);
        }
    }

    fn conditional_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        stmt: &v1::ConditionalStatement,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.conditional_statement(diagnostics, reason, stmt);
        }
    }

    fn scatter_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        stmt: &v1::ScatterStatement,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.scatter_statement(diagnostics, reason, stmt);
        }
    }

    fn call_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        stmt: &v1::CallStatement,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.call_statement(diagnostics, reason, stmt);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_nearest_rule() {
        let validator = Validator::default();

        // Test exact match
        let nearest = validator.find_nearest_rule("UnusedInput");
        pretty_assertions::assert_eq!(nearest.as_deref(), Some("UnusedInput"));

        // Test close match
        let nearest = validator.find_nearest_rule("UnusedInputt");
        pretty_assertions::assert_eq!(nearest.as_deref(), Some("UnusedInput"));

        // Test another exact match
        let nearest = validator.find_nearest_rule("UnusedCall");
        pretty_assertions::assert_eq!(nearest.as_deref(), Some("UnusedCall"));

        // Test a typo
        let nearest = validator.find_nearest_rule("UnusedKall");
        pretty_assertions::assert_eq!(nearest.as_deref(), Some("UnusedCall"));

        // Test a more significant typo
        let nearest = validator.find_nearest_rule("UnnecessaryFunctionAl");
        pretty_assertions::assert_eq!(nearest.as_deref(), Some("UnnecessaryFunctionCall"));

        // Test a completely different string
        let nearest = validator.find_nearest_rule("CompletelyDifferentRule");
        pretty_assertions::assert_eq!(nearest.as_deref(), None);
    }
}
