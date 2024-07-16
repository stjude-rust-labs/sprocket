//! Validator for WDL documents.

use super::v1;
use super::Comment;
use super::Diagnostic;
use super::VisitReason;
use super::Whitespace;
use crate::Document;
use crate::SupportedVersion;
use crate::VersionStatement;
use crate::Visitor;

mod counts;
mod exprs;
mod keys;
mod numbers;
mod requirements;
mod strings;
mod version;

/// Represents a collection of validation diagnostics.
///
/// Validation visitors receive a diagnostics collection during
/// visitation of the AST.
#[allow(missing_debug_implementations)]
#[derive(Default)]
pub struct Diagnostics(Vec<Diagnostic>);

impl Diagnostics {
    /// Adds a diagnostic to the collection.
    pub fn add(&mut self, diagnostic: Diagnostic) {
        self.0.push(diagnostic);
    }
}

/// Implements an AST validator.
///
/// A validator operates on a set of AST visitors, providing a collection
/// of diagnostics as the visitation state.
///
/// See the [validate](Self::validate) method to perform the validation.
///
/// # Examples
///
/// ```rust
/// # use wdl_ast::{Document, Validator};
/// let (document, diagnostics) = Document::parse("version 1.1\nworkflow example {}");
/// assert!(diagnostics.is_empty());
/// let mut validator = Validator::default();
/// assert!(validator.validate(&document).is_ok());
/// ```
#[allow(missing_debug_implementations)]
pub struct Validator {
    /// The set of validation visitors.
    visitors: Vec<Box<dyn Visitor<State = Diagnostics>>>,
}

impl Validator {
    /// Creates a validator with an empty visitors set.
    pub const fn empty() -> Self {
        Self {
            visitors: Vec::new(),
        }
    }

    /// Adds a visitor to the validator.
    pub fn add_visitor<V: Visitor<State = Diagnostics> + 'static>(&mut self, visitor: V) {
        self.visitors.push(Box::new(visitor));
    }

    /// Adds multiple visitors to the validator.
    pub fn add_visitors(
        &mut self,
        visitors: impl IntoIterator<Item = Box<dyn Visitor<State = Diagnostics>>>,
    ) {
        self.visitors.extend(visitors)
    }

    /// Validates the given document and returns the validation errors upon
    /// failure.
    pub fn validate(&mut self, document: &Document) -> Result<(), Vec<Diagnostic>> {
        let mut diagnostics = Diagnostics::default();
        document.visit(&mut diagnostics, self);

        if diagnostics.0.is_empty() {
            Ok(())
        } else {
            diagnostics.0.sort();
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
            ],
        }
    }
}

impl Visitor for Validator {
    type State = Diagnostics;

    fn document(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        doc: &Document,
        version: SupportedVersion,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.document(state, reason, doc, version);
        }
    }

    fn whitespace(&mut self, state: &mut Self::State, whitespace: &Whitespace) {
        for visitor in self.visitors.iter_mut() {
            visitor.whitespace(state, whitespace);
        }
    }

    fn comment(&mut self, state: &mut Self::State, comment: &Comment) {
        for visitor in self.visitors.iter_mut() {
            visitor.comment(state, comment);
        }
    }

    fn version_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &VersionStatement,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.version_statement(state, reason, stmt);
        }
    }

    fn import_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &v1::ImportStatement,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.import_statement(state, reason, stmt);
        }
    }

    fn struct_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        def: &v1::StructDefinition,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.struct_definition(state, reason, def);
        }
    }

    fn task_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        task: &v1::TaskDefinition,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.task_definition(state, reason, task);
        }
    }

    fn workflow_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        workflow: &v1::WorkflowDefinition,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.workflow_definition(state, reason, workflow);
        }
    }

    fn input_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::InputSection,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.input_section(state, reason, section);
        }
    }

    fn output_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::OutputSection,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.output_section(state, reason, section);
        }
    }

    fn command_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::CommandSection,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.command_section(state, reason, section);
        }
    }

    fn command_text(&mut self, state: &mut Self::State, text: &v1::CommandText) {
        for visitor in self.visitors.iter_mut() {
            visitor.command_text(state, text);
        }
    }

    fn requirements_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::RequirementsSection,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.requirements_section(state, reason, section);
        }
    }

    fn hints_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::HintsSection,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.hints_section(state, reason, section);
        }
    }

    fn runtime_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::RuntimeSection,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.runtime_section(state, reason, section);
        }
    }

    fn runtime_item(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        item: &v1::RuntimeItem,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.runtime_item(state, reason, item);
        }
    }

    fn metadata_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::MetadataSection,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.metadata_section(state, reason, section);
        }
    }

    fn parameter_metadata_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::ParameterMetadataSection,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.parameter_metadata_section(state, reason, section);
        }
    }

    fn metadata_object(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        object: &v1::MetadataObject,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.metadata_object(state, reason, object);
        }
    }

    fn metadata_object_item(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        item: &v1::MetadataObjectItem,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.metadata_object_item(state, reason, item);
        }
    }

    fn unbound_decl(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        decl: &v1::UnboundDecl,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.unbound_decl(state, reason, decl);
        }
    }

    fn bound_decl(&mut self, state: &mut Self::State, reason: VisitReason, decl: &v1::BoundDecl) {
        for visitor in self.visitors.iter_mut() {
            visitor.bound_decl(state, reason, decl);
        }
    }

    fn expr(&mut self, state: &mut Self::State, reason: VisitReason, expr: &v1::Expr) {
        for visitor in self.visitors.iter_mut() {
            visitor.expr(state, reason, expr);
        }
    }

    fn string_text(&mut self, state: &mut Self::State, text: &v1::StringText) {
        for visitor in self.visitors.iter_mut() {
            visitor.string_text(state, text);
        }
    }

    fn placeholder(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        placeholder: &v1::Placeholder,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.placeholder(state, reason, placeholder);
        }
    }

    fn conditional_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &v1::ConditionalStatement,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.conditional_statement(state, reason, stmt);
        }
    }

    fn scatter_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &v1::ScatterStatement,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.scatter_statement(state, reason, stmt);
        }
    }

    fn call_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &v1::CallStatement,
    ) {
        for visitor in self.visitors.iter_mut() {
            visitor.call_statement(state, reason, stmt);
        }
    }
}
