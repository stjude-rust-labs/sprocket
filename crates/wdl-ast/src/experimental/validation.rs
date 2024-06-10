//! Validator for WDL documents.

use std::sync::Arc;

use super::v1;
use super::Ast;
use super::Comment;
use super::Diagnostic;
use super::VisitReason;
use super::Whitespace;
use crate::experimental::Document;
use crate::experimental::VersionStatement;

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
/// # use wdl_ast::experimental::{Document, Validator};
/// let document = Document::parse("version 1.1\nworkflow example {}")
///     .into_result()
///     .expect("should parse without errors");
/// let validator = Validator::default();
/// assert!(validator.validate(&document).is_ok());
/// ```
#[allow(missing_debug_implementations)]
pub struct Validator {
    /// The set of version 1.x validation visitors.
    v1: Vec<Box<dyn v1::Visitor<State = Diagnostics>>>,
}

impl Validator {
    /// Creates a validator with an empty visitors set.
    pub fn empty() -> Self {
        Self {
            v1: Default::default(),
        }
    }

    /// Adds a V1 visitor to the validator.
    pub fn add_v1_visitor<V: v1::Visitor<State = Diagnostics> + 'static>(&mut self, visitor: V) {
        self.v1.push(Box::new(visitor));
    }

    /// Adds multiple V1 visitors to the validator.
    pub fn add_v1_visitors(
        &mut self,
        visitors: impl IntoIterator<Item = Box<dyn v1::Visitor<State = Diagnostics>>>,
    ) {
        self.v1.extend(visitors)
    }

    /// Validates the given document and returns the validation errors upon
    /// failure.
    pub fn validate(mut self, document: &Document) -> Result<(), Arc<[Diagnostic]>> {
        let mut diagnostics = Diagnostics::default();

        match document.ast() {
            Ast::Unsupported => {}
            Ast::V1(ast) => {
                if !self.v1.is_empty() {
                    ast.visit(&mut diagnostics, &mut self);
                }
            }
        }

        if diagnostics.0.is_empty() {
            Ok(())
        } else {
            Err(diagnostics.0.into())
        }
    }
}

impl Default for Validator {
    fn default() -> Self {
        Self {
            v1: vec![
                Box::new(v1::validation::LiteralTextVisitor),
                Box::new(v1::validation::CountingVisitor::default()),
                Box::new(v1::validation::UniqueKeysVisitor::default()),
                Box::new(v1::validation::NumberVisitor::default()),
            ],
        }
    }
}

impl v1::Visitor for Validator {
    type State = Diagnostics;

    fn document(&mut self, state: &mut Self::State, reason: VisitReason, doc: &Document) {
        for visitor in self.v1.iter_mut() {
            visitor.document(state, reason, doc);
        }
    }

    fn whitespace(&mut self, state: &mut Self::State, whitespace: &Whitespace) {
        for visitor in self.v1.iter_mut() {
            visitor.whitespace(state, whitespace);
        }
    }

    fn comment(&mut self, state: &mut Self::State, comment: &Comment) {
        for visitor in self.v1.iter_mut() {
            visitor.comment(state, comment);
        }
    }

    fn version_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &VersionStatement,
    ) {
        for visitor in self.v1.iter_mut() {
            visitor.version_statement(state, reason, stmt);
        }
    }

    fn import_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &v1::ImportStatement,
    ) {
        for visitor in self.v1.iter_mut() {
            visitor.import_statement(state, reason, stmt);
        }
    }

    fn struct_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        def: &v1::StructDefinition,
    ) {
        for visitor in self.v1.iter_mut() {
            visitor.struct_definition(state, reason, def);
        }
    }

    fn task_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        task: &v1::TaskDefinition,
    ) {
        for visitor in self.v1.iter_mut() {
            visitor.task_definition(state, reason, task);
        }
    }

    fn workflow_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        workflow: &v1::WorkflowDefinition,
    ) {
        for visitor in self.v1.iter_mut() {
            visitor.workflow_definition(state, reason, workflow);
        }
    }

    fn input_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::InputSection,
    ) {
        for visitor in self.v1.iter_mut() {
            visitor.input_section(state, reason, section);
        }
    }

    fn output_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::OutputSection,
    ) {
        for visitor in self.v1.iter_mut() {
            visitor.output_section(state, reason, section);
        }
    }

    fn command_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::CommandSection,
    ) {
        for visitor in self.v1.iter_mut() {
            visitor.command_section(state, reason, section);
        }
    }

    fn command_text(&mut self, state: &mut Self::State, text: &v1::CommandText) {
        for visitor in self.v1.iter_mut() {
            visitor.command_text(state, text);
        }
    }

    fn runtime_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::RuntimeSection,
    ) {
        for visitor in self.v1.iter_mut() {
            visitor.runtime_section(state, reason, section);
        }
    }

    fn metadata_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::MetadataSection,
    ) {
        for visitor in self.v1.iter_mut() {
            visitor.metadata_section(state, reason, section);
        }
    }

    fn parameter_metadata_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::ParameterMetadataSection,
    ) {
        for visitor in self.v1.iter_mut() {
            visitor.parameter_metadata_section(state, reason, section);
        }
    }

    fn metadata_object(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        object: &v1::MetadataObject,
    ) {
        for visitor in self.v1.iter_mut() {
            visitor.metadata_object(state, reason, object);
        }
    }

    fn unbound_decl(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        decl: &v1::UnboundDecl,
    ) {
        for visitor in self.v1.iter_mut() {
            visitor.unbound_decl(state, reason, decl);
        }
    }

    fn bound_decl(&mut self, state: &mut Self::State, reason: VisitReason, decl: &v1::BoundDecl) {
        for visitor in self.v1.iter_mut() {
            visitor.bound_decl(state, reason, decl);
        }
    }

    fn expr(&mut self, state: &mut Self::State, reason: VisitReason, expr: &v1::Expr) {
        for visitor in self.v1.iter_mut() {
            visitor.expr(state, reason, expr);
        }
    }

    fn string_text(&mut self, state: &mut Self::State, text: &v1::StringText) {
        for visitor in self.v1.iter_mut() {
            visitor.string_text(state, text);
        }
    }

    fn conditional_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &v1::ConditionalStatement,
    ) {
        for visitor in self.v1.iter_mut() {
            visitor.conditional_statement(state, reason, stmt);
        }
    }

    fn scatter_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &v1::ScatterStatement,
    ) {
        for visitor in self.v1.iter_mut() {
            visitor.scatter_statement(state, reason, stmt);
        }
    }

    fn call_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &v1::CallStatement,
    ) {
        for visitor in self.v1.iter_mut() {
            visitor.call_statement(state, reason, stmt);
        }
    }
}
