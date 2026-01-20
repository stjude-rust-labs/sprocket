//! Implementation of the linter.

use std::collections::HashSet;

use indexmap::IndexMap;
use wdl_analysis::Diagnostics;
use wdl_analysis::Document as AnalysisDocument;
use wdl_analysis::SyntaxNodeExt;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::Comment;
use wdl_ast::SupportedVersion;
use wdl_ast::VersionStatement;
use wdl_ast::Whitespace;
use wdl_ast::v1;

use crate::Config;
use crate::Rule;
use crate::rules;

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
pub struct Linter {
    /// The map of rule name to rule.
    rules: IndexMap<&'static str, Box<dyn Rule>>,
    /// The set of rule ids that are disabled for the current document.
    document_exceptions: HashSet<String>,
}

impl Linter {
    /// Creates a new linter with the given rules.
    pub fn new(rules: impl IntoIterator<Item = Box<dyn Rule>>) -> Self {
        Self {
            rules: rules.into_iter().map(|r| (r.id(), r)).collect(),
            document_exceptions: HashSet::default(),
        }
    }

    /// Invokes a callback on each rule
    fn each_enabled_rule<F>(&mut self, diagnostics: &mut Diagnostics, mut cb: F)
    where
        F: FnMut(&mut Diagnostics, &mut dyn Rule),
    {
        for (id, rule) in &mut self.rules {
            if self.document_exceptions.contains(id.to_owned()) {
                continue;
            }
            cb(diagnostics, rule.as_mut());
        }
    }
}

impl Default for Linter {
    fn default() -> Self {
        Self {
            rules: rules(&Config::default())
                .into_iter()
                .map(|r| (r.id(), r))
                .collect(),
            document_exceptions: HashSet::default(),
        }
    }
}

impl Visitor for Linter {
    fn reset(&mut self) {
        // Reset the state of each rule
        for rule in self.rules.values_mut() {
            rule.reset();
        }

        // Reset the document exceptions
        self.document_exceptions.clear();
    }

    fn document(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        doc: &AnalysisDocument,
        version: SupportedVersion,
    ) {
        if reason == VisitReason::Enter {
            self.document_exceptions.extend(
                doc.root()
                    .version_statement()
                    .expect("document should have version statement")
                    .inner()
                    .rule_exceptions(),
            );
        }

        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.document(diagnostics, reason, doc, version);
        });
    }

    fn whitespace(&mut self, diagnostics: &mut Diagnostics, whitespace: &Whitespace) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.whitespace(diagnostics, whitespace);
        });
    }

    fn comment(&mut self, diagnostics: &mut Diagnostics, comment: &Comment) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.comment(diagnostics, comment);
        });
    }

    fn version_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        stmt: &VersionStatement,
    ) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.version_statement(diagnostics, reason, stmt);
        });
    }

    fn import_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        stmt: &v1::ImportStatement,
    ) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.import_statement(diagnostics, reason, stmt)
        });
    }

    fn struct_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        def: &v1::StructDefinition,
    ) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.struct_definition(diagnostics, reason, def)
        });
    }

    fn task_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        task: &v1::TaskDefinition,
    ) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.task_definition(diagnostics, reason, task)
        });
    }

    fn workflow_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        workflow: &v1::WorkflowDefinition,
    ) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.workflow_definition(diagnostics, reason, workflow)
        });
    }

    fn input_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &v1::InputSection,
    ) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.input_section(diagnostics, reason, section)
        });
    }

    fn output_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &v1::OutputSection,
    ) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.output_section(diagnostics, reason, section)
        });
    }

    fn command_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &v1::CommandSection,
    ) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.command_section(diagnostics, reason, section)
        });
    }

    fn command_text(&mut self, diagnostics: &mut Diagnostics, text: &v1::CommandText) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.command_text(diagnostics, text);
        });
    }

    fn requirements_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &v1::RequirementsSection,
    ) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.requirements_section(diagnostics, reason, section)
        });
    }

    fn task_hints_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &v1::TaskHintsSection,
    ) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.task_hints_section(diagnostics, reason, section)
        });
    }

    fn workflow_hints_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &v1::WorkflowHintsSection,
    ) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.workflow_hints_section(diagnostics, reason, section)
        });
    }

    fn runtime_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &v1::RuntimeSection,
    ) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.runtime_section(diagnostics, reason, section)
        });
    }

    fn runtime_item(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        item: &v1::RuntimeItem,
    ) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.runtime_item(diagnostics, reason, item)
        });
    }

    fn metadata_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &v1::MetadataSection,
    ) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.metadata_section(diagnostics, reason, section)
        });
    }

    fn parameter_metadata_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &v1::ParameterMetadataSection,
    ) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.parameter_metadata_section(diagnostics, reason, section)
        });
    }

    fn metadata_object(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        object: &v1::MetadataObject,
    ) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.metadata_object(diagnostics, reason, object)
        });
    }

    fn metadata_object_item(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        item: &v1::MetadataObjectItem,
    ) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.metadata_object_item(diagnostics, reason, item)
        });
    }

    fn metadata_array(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        item: &v1::MetadataArray,
    ) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.metadata_array(diagnostics, reason, item)
        });
    }

    fn unbound_decl(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        decl: &v1::UnboundDecl,
    ) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.unbound_decl(diagnostics, reason, decl)
        });
    }

    fn bound_decl(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        decl: &v1::BoundDecl,
    ) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.bound_decl(diagnostics, reason, decl)
        });
    }

    fn expr(&mut self, diagnostics: &mut Diagnostics, reason: VisitReason, expr: &v1::Expr) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.expr(diagnostics, reason, expr)
        });
    }

    fn string_text(&mut self, diagnostics: &mut Diagnostics, text: &v1::StringText) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.string_text(diagnostics, text);
        });
    }

    fn placeholder(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        placeholder: &v1::Placeholder,
    ) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.placeholder(diagnostics, reason, placeholder)
        });
    }

    fn conditional_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        stmt: &v1::ConditionalStatement,
    ) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.conditional_statement(diagnostics, reason, stmt)
        });
    }

    fn scatter_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        stmt: &v1::ScatterStatement,
    ) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.scatter_statement(diagnostics, reason, stmt)
        });
    }

    fn call_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        stmt: &v1::CallStatement,
    ) {
        self.each_enabled_rule(diagnostics, |diagnostics, rule| {
            rule.call_statement(diagnostics, reason, stmt)
        });
    }
}
