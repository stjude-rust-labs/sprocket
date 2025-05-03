//! A lint rule for matching parameter metadata.

use std::collections::HashMap;

use wdl_analysis::Diagnostics;
use wdl_analysis::Document;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::Decl;
use wdl_ast::v1::ParameterMetadataSection;
use wdl_ast::v1::SectionParent;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::WorkflowDefinition;
use wdl_ast::version::V1;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the matching parameter meta rule.
const ID: &str = "ParameterMetaMatched";

/// Creates a "missing param meta" diagnostic.
fn missing_param_meta(parent: &SectionParent, missing: &str, span: Span) -> Diagnostic {
    let (context, parent) = match parent {
        SectionParent::Task(t) => ("task", t.name()),
        SectionParent::Workflow(w) => ("workflow", w.name()),
        SectionParent::Struct(s) => ("struct", s.name()),
    };

    Diagnostic::warning(format!(
        "{context} `{parent}` is missing a parameter metadata key for input `{missing}`",
        parent = parent.text(),
    ))
    .with_rule(ID)
    .with_label(
        "this input does not have an entry in the parameter metadata section",
        span,
    )
    .with_fix(format!(
        "add a `{missing}` key to the `parameter_meta` section with a detailed description of the \
         input.",
    ))
}

/// Creates an "extra param meta" diagnostic.
fn extra_param_meta(parent: &SectionParent, extra: &str, span: Span) -> Diagnostic {
    let (context, parent) = match parent {
        SectionParent::Task(t) => ("task", t.name()),
        SectionParent::Workflow(w) => ("workflow", w.name()),
        SectionParent::Struct(s) => ("struct", s.name()),
    };

    Diagnostic::note(format!(
        "{context} `{parent}` has an extraneous parameter metadata key named `{extra}`",
        parent = parent.text(),
    ))
    .with_rule(ID)
    .with_label(
        "this key does not correspond to any input declaration",
        span,
    )
    .with_fix("remove the extraneous key from the `parameter_meta` section")
}

/// Creates a "mismatched order" diagnostic.
fn mismatched_param_order(parent: &SectionParent, span: Span, expected_order: &str) -> Diagnostic {
    let (context, parent) = match parent {
        SectionParent::Task(t) => ("task", t.name()),
        SectionParent::Workflow(w) => ("workflow", w.name()),
        SectionParent::Struct(s) => ("struct", s.name()),
    };

    Diagnostic::note(format!(
        "parameter metadata in {context} `{parent}` is out of order",
        parent = parent.text(),
    ))
    .with_rule(ID)
    .with_label(
        "parameter metadata must be in the same order as inputs",
        span,
    )
    .with_fix(format!(
        "based on the current `input` order, order the parameter metadata as:\n{}",
        expected_order
    ))
}

/// Detects missing or extraneous entries in a `parameter_meta` section.
#[derive(Default, Debug, Clone, Copy)]
pub struct ParameterMetaMatchedRule {
    /// The version of the WDL document being linted.
    version: Option<SupportedVersion>,
}

impl Rule for ParameterMetaMatchedRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that inputs have a matching entry in a `parameter_meta` section."
    }

    fn explanation(&self) -> &'static str {
        "Each input parameter within a task or workflow should have an associated `parameter_meta` \
         entry with a detailed description of the input. Non-input keys are not permitted within \
         the `parameter_meta` block."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Completeness, Tag::Sorting])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::ParameterMetadataSectionNode,
        ])
    }

    fn related_rules(&self) -> &[&'static str] {
        &[
            "MetaDescription",
            "InputSorted",
            "OutputSection",
            "RequirementsSection",
            "RuntimeSection",
            "MatchingOutputMeta",
        ]
    }
}

/// Checks for both missing and extra items in a `parameter_meta` section
/// along with the order of the items.
fn check_parameter_meta(
    parent: &SectionParent,
    expected: Vec<Decl>,
    param_meta: ParameterMetadataSection,
    diagnostics: &mut Diagnostics,
    exceptable_nodes: &Option<&'static [SyntaxKind]>,
) {
    let expected_map: HashMap<_, _> = expected
        .iter()
        .map(|decl| (decl.name().text().to_string(), decl.name().span()))
        .collect();
    let actual_map: HashMap<_, _> = param_meta
        .items()
        .map(|m| {
            let name = m.name();
            (name.text().to_string(), name.span())
        })
        .collect();

    // We determine the intersection of expected and actual parameter names.
    // Using these we next check for missing and extraneous parameters separately.
    let expected_order: Vec<_> = expected
        .iter()
        .map(|decl| decl.name().text().to_string())
        .filter(|name| actual_map.contains_key(name))
        .collect();

    let actual_order: Vec<_> = param_meta
        .items()
        .map(|m| m.name().text().to_string())
        .filter(|name| expected_map.contains_key(name))
        .collect();

    for (name, span) in &expected_map {
        if !actual_map.contains_key(name) {
            diagnostics.exceptable_add(
                missing_param_meta(parent, name, *span),
                SyntaxElement::from(param_meta.inner().clone()),
                exceptable_nodes,
            );
        }
    }

    for (name, span) in &actual_map {
        if !expected_map.contains_key(name) {
            diagnostics.exceptable_add(
                extra_param_meta(parent, name, *span),
                SyntaxElement::from(param_meta.inner().clone()),
                exceptable_nodes,
            );
        }
    }

    if expected_order != actual_order {
        let span = param_meta
            .inner()
            .first_token()
            .expect("must have parameter meta token")
            .text_range()
            .into();
        diagnostics.exceptable_add(
            mismatched_param_order(parent, span, &expected_order.join("\n")),
            SyntaxElement::from(param_meta.inner().clone()),
            exceptable_nodes,
        );
    }
}

impl Visitor for ParameterMetaMatchedRule {
    fn reset(&mut self) {
        *self = Default::default();
    }

    fn document(
        &mut self,
        _: &mut Diagnostics,
        reason: VisitReason,
        _: &Document,
        version: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        self.version = Some(version);
    }

    fn task_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        task: &TaskDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Note that only the first input and parameter_meta sections are checked as any
        // additional sections is considered a validation error
        match task.parameter_metadata() {
            Some(param_meta) => {
                check_parameter_meta(
                    &SectionParent::Task(task.clone()),
                    task.input().iter().flat_map(|i| i.declarations()).collect(),
                    param_meta,
                    diagnostics,
                    &self.exceptable_nodes(),
                );
            }
            None => {
                // If there is no parameter_meta section, then let the
                // MetaSections rule handle it
            }
        }
    }

    fn workflow_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        workflow: &WorkflowDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Note that only the first input and parameter_meta sections are checked as any
        // additional sections is considered a validation error
        match workflow.parameter_metadata() {
            Some(param_meta) => {
                check_parameter_meta(
                    &SectionParent::Workflow(workflow.clone()),
                    workflow
                        .input()
                        .iter()
                        .flat_map(|i| i.declarations())
                        .collect(),
                    param_meta,
                    diagnostics,
                    &self.exceptable_nodes(),
                );
            }
            None => {
                // If there is no parameter_meta section, then let the
                // MetaSections rule handle it
            }
        }
    }

    fn struct_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        def: &wdl_ast::v1::StructDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Only check struct definitions for WDL >=1.2
        if self.version.expect("should have version") < SupportedVersion::V1(V1::Two) {
            return;
        }

        // Note that only the first input and parameter_meta sections are checked as any
        // additional sections is considered a validation error
        match def.parameter_metadata().next() {
            Some(param_meta) => {
                check_parameter_meta(
                    &SectionParent::Struct(def.clone()),
                    def.members().map(Decl::Unbound).collect(),
                    param_meta,
                    diagnostics,
                    &self.exceptable_nodes(),
                );
            }
            None => {
                // If there is no parameter_meta section, then let the
                // MetaSections rule handle it
            }
        }
    }
}
