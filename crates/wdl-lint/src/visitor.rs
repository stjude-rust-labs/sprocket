//! Implementation of the lint visitor.

use std::collections::HashSet;

use indexmap::IndexMap;
use wdl_ast::v1;
use wdl_ast::AstNode;
use wdl_ast::Comment;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::SupportedVersion;
use wdl_ast::VersionStatement;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;
use wdl_ast::Whitespace;

use crate::rules;
use crate::Rule;

/// A visitor that runs linting rules.
///
/// By default, the visitor runs all lint rules.
///
/// This visitor respects `#@ except` comments that precede AST nodes.
///
/// The format for the comment is `#@ except: <ids>`, where `ids` is a
/// comma-separated list of lint rule identifiers.
///
/// Any `#@ except` comments that come before the version statement will disable
/// the rule for the entire document.
///
/// Otherwise, `#@ except` comments disable the rule for the immediately
/// following AST node.
#[allow(missing_debug_implementations)]
pub struct LintVisitor {
    /// The map of rule name to rule.
    rules: IndexMap<&'static str, Box<dyn Rule>>,
    /// The set of rule ids that are disabled for the current document.
    document_exceptions: HashSet<String>,
}

impl LintVisitor {
    /// Creates a new linting visitor with the given rules.
    pub fn new(rules: impl IntoIterator<Item = Box<dyn Rule>>) -> Self {
        Self {
            rules: rules.into_iter().map(|r| (r.id(), r)).collect(),
            document_exceptions: HashSet::default(),
        }
    }

    /// Invokes a callback on each rule
    fn each_enabled_rule<F>(&mut self, state: &mut Diagnostics, mut cb: F)
    where
        F: FnMut(&mut Diagnostics, &mut dyn Rule),
    {
        for (id, rule) in &mut self.rules {
            if self.document_exceptions.contains(id.to_owned()) {
                continue;
            }
            cb(state, rule.as_mut());
        }
    }
}

impl Default for LintVisitor {
    fn default() -> Self {
        Self {
            rules: rules().into_iter().map(|r| (r.id(), r)).collect(),
            document_exceptions: HashSet::default(),
        }
    }
}

impl Visitor for LintVisitor {
    type State = Diagnostics;

    fn document(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        doc: &Document,
        version: SupportedVersion,
    ) {
        if reason == VisitReason::Enter {
            // Reset state for a new document
            self.document_exceptions.clear();
        }

        self.document_exceptions.extend(
            state.exceptions_for(
                doc.version_statement()
                    .expect("document should have version statement")
                    .syntax(),
            ),
        );

        self.each_enabled_rule(state, |state, rule| {
            rule.document(state, reason, doc, version);
        });
    }

    fn whitespace(&mut self, state: &mut Self::State, whitespace: &Whitespace) {
        self.each_enabled_rule(state, |state, rule| {
            rule.whitespace(state, whitespace);
        });
    }

    fn comment(&mut self, state: &mut Self::State, comment: &Comment) {
        self.each_enabled_rule(state, |state, rule| {
            rule.comment(state, comment);
        });
    }

    fn version_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &VersionStatement,
    ) {
        self.each_enabled_rule(state, |state, rule| {
            rule.version_statement(state, reason, stmt);
        });
    }

    fn import_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &v1::ImportStatement,
    ) {
        self.each_enabled_rule(state, |state, rule| {
            rule.import_statement(state, reason, stmt)
        });
    }

    fn struct_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        def: &v1::StructDefinition,
    ) {
        self.each_enabled_rule(state, |state, rule| {
            rule.struct_definition(state, reason, def)
        });
    }

    fn task_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        task: &v1::TaskDefinition,
    ) {
        self.each_enabled_rule(state, |state, rule| {
            rule.task_definition(state, reason, task)
        });
    }

    fn workflow_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        workflow: &v1::WorkflowDefinition,
    ) {
        self.each_enabled_rule(state, |state, rule| {
            rule.workflow_definition(state, reason, workflow)
        });
    }

    fn input_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::InputSection,
    ) {
        self.each_enabled_rule(state, |state, rule| {
            rule.input_section(state, reason, section)
        });
    }

    fn output_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::OutputSection,
    ) {
        self.each_enabled_rule(state, |state, rule| {
            rule.output_section(state, reason, section)
        });
    }

    fn command_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::CommandSection,
    ) {
        self.each_enabled_rule(state, |state, rule| {
            rule.command_section(state, reason, section)
        });
    }

    fn command_text(&mut self, state: &mut Self::State, text: &v1::CommandText) {
        self.each_enabled_rule(state, |state, rule| {
            rule.command_text(state, text);
        });
    }

    fn requirements_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::RequirementsSection,
    ) {
        self.each_enabled_rule(state, |state, rule| {
            rule.requirements_section(state, reason, section)
        });
    }

    fn hints_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::HintsSection,
    ) {
        self.each_enabled_rule(state, |state, rule| {
            rule.hints_section(state, reason, section)
        });
    }

    fn runtime_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::RuntimeSection,
    ) {
        self.each_enabled_rule(state, |state, rule| {
            rule.runtime_section(state, reason, section)
        });
    }

    fn runtime_item(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        item: &v1::RuntimeItem,
    ) {
        self.each_enabled_rule(state, |state, rule| rule.runtime_item(state, reason, item));
    }

    fn metadata_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::MetadataSection,
    ) {
        self.each_enabled_rule(state, |state, rule| {
            rule.metadata_section(state, reason, section)
        });
    }

    fn parameter_metadata_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::ParameterMetadataSection,
    ) {
        self.each_enabled_rule(state, |state, rule| {
            rule.parameter_metadata_section(state, reason, section)
        });
    }

    fn metadata_object(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        object: &v1::MetadataObject,
    ) {
        self.each_enabled_rule(state, |state, rule| {
            rule.metadata_object(state, reason, object)
        });
    }

    fn metadata_object_item(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        item: &v1::MetadataObjectItem,
    ) {
        self.each_enabled_rule(state, |state, rule| {
            rule.metadata_object_item(state, reason, item)
        });
    }

    fn metadata_array(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        item: &v1::MetadataArray,
    ) {
        self.each_enabled_rule(state, |state, rule| {
            rule.metadata_array(state, reason, item)
        });
    }

    fn unbound_decl(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        decl: &v1::UnboundDecl,
    ) {
        self.each_enabled_rule(state, |state, rule| rule.unbound_decl(state, reason, decl));
    }

    fn bound_decl(&mut self, state: &mut Self::State, reason: VisitReason, decl: &v1::BoundDecl) {
        self.each_enabled_rule(state, |state, rule| rule.bound_decl(state, reason, decl));
    }

    fn expr(&mut self, state: &mut Self::State, reason: VisitReason, expr: &v1::Expr) {
        self.each_enabled_rule(state, |state, rule| rule.expr(state, reason, expr));
    }

    fn string_text(&mut self, state: &mut Self::State, text: &v1::StringText) {
        self.each_enabled_rule(state, |state, rule| {
            rule.string_text(state, text);
        });
    }

    fn placeholder(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        placeholder: &v1::Placeholder,
    ) {
        self.each_enabled_rule(state, |state, rule| {
            rule.placeholder(state, reason, placeholder)
        });
    }

    fn conditional_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &v1::ConditionalStatement,
    ) {
        self.each_enabled_rule(state, |state, rule| {
            rule.conditional_statement(state, reason, stmt)
        });
    }

    fn scatter_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &v1::ScatterStatement,
    ) {
        self.each_enabled_rule(state, |state, rule| {
            rule.scatter_statement(state, reason, stmt)
        });
    }

    fn call_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &v1::CallStatement,
    ) {
        self.each_enabled_rule(state, |state, rule| {
            rule.call_statement(state, reason, stmt)
        });
    }
}

#[cfg(test)]
mod test {
    use wdl_ast::Validator;

    use super::*;

    #[test]
    fn it_supports_reuse() {
        let source = r#"## Test source
#@ except: MissingMetas, MissingOutput

version 1.1

workflow test {
}
"#;

        let (document, diagnostics) = wdl_ast::Document::parse(source);
        assert!(diagnostics.is_empty());

        let mut validator = Validator::default();
        validator.add_visitor(LintVisitor::default());

        // Validate the document twice to ensure that reusing the lint visitor generates
        // no new diagnostics
        validator
            .validate(&document)
            .expect("should not have any diagnostics");
        validator
            .validate(&document)
            .expect("should not have any diagnostics");
    }
}
