//! Validator for WDL documents.

use wdl_ast::Comment;
use wdl_ast::Diagnostic;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::VersionStatement;
use wdl_ast::Whitespace;
use wdl_ast::v1;

use crate::Config;
use crate::Exceptable;
use crate::VisitReason;
use crate::Visitor;
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

/// Represents a collection of validation diagnostics.
///
/// Validation visitors receive a diagnostics collection during
/// visitation of the AST.
#[derive(Debug, Default)]
pub struct Diagnostics(pub(crate) Vec<Diagnostic>);

impl Diagnostics {
    /// Adds a diagnostic to the collection.
    pub fn add(&mut self, diagnostic: Diagnostic) {
        self.0.push(diagnostic);
    }

    /// Adds a diagnostic to the collection, unless the diagnostic is for an
    /// element that has an exception for the given rule.
    ///
    /// If the diagnostic does not have a rule, the diagnostic is always added.
    pub fn exceptable_add(
        &mut self,
        diagnostic: Diagnostic,
        element: SyntaxElement,
        exceptable_nodes: &Option<&'static [SyntaxKind]>,
    ) {
        if let Some(rule) = diagnostic.rule() {
            for node in element.ancestors().filter(|node| {
                exceptable_nodes
                    .as_ref()
                    .is_none_or(|nodes| nodes.contains(&node.kind()))
            }) {
                if node.is_rule_excepted(rule) {
                    // Rule is currently excepted, don't add the diagnostic
                    return;
                }
            }
        }

        self.add(diagnostic);
    }

    /// Extends the collection with another collection of diagnostics.
    pub fn extend(&mut self, diagnostics: Diagnostics) {
        self.0.extend(diagnostics.0);
    }

    /// Returns whether the collection is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Sorts the diagnostics in the collection.
    pub fn sort(&mut self) {
        self.0.sort();
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
}

impl Validator {
    /// Creates a validator with an empty visitors set.
    pub const fn empty() -> Self {
        Self {
            visitors: Vec::new(),
        }
    }

    /// Adds a visitor to the validator.
    pub fn add_visitor<V: Visitor + 'static>(&mut self, visitor: V) {
        self.visitors.push(Box::new(visitor));
    }

    /// Adds multiple visitors to the validator.
    pub fn add_visitors(&mut self, visitors: impl IntoIterator<Item = Box<dyn Visitor>>) {
        self.visitors.extend(visitors)
    }

    /// Validates the given document and returns the validation errors upon
    /// failure.
    pub fn validate(
        &mut self,
        document: &Document,
        config: &Config,
    ) -> Result<(), Vec<Diagnostic>> {
        let mut diagnostics = Diagnostics::default();
        self.register(config);
        document.visit(&mut diagnostics, self);

        self.reset();

        if diagnostics.is_empty() {
            Ok(())
        } else {
            diagnostics.sort();
            Err(diagnostics.0)
        }
    }
}

impl Default for Validator {
    /// Creates a validator with the default validation visitors.
    fn default() -> Self {
        Self {
            visitors: vec![
                Box::new(strings::LiteralTextVisitor),
                Box::<counts::CountingVisitor>::default(),
                Box::<keys::UniqueKeysVisitor>::default(),
                Box::<numbers::NumberVisitor>::default(),
                Box::<version::VersionVisitor>::default(),
                Box::<requirements::RequirementsVisitor>::default(),
                Box::<exprs::ScopedExprVisitor>::default(),
                Box::<imports::ImportsVisitor>::default(),
                Box::<env::EnvVisitor>::default(),
            ],
        }
    }
}

impl Visitor for Validator {
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
