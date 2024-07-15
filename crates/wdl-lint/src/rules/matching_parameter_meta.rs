//! A lint rule for matching parameter metadata.

use std::collections::HashMap;

use wdl_ast::v1::ParameterMetadataSection;
use wdl_ast::v1::SectionParent;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::WorkflowDefinition;
use wdl_ast::version::V1;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::Ident;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the matching parameter meta rule.
const ID: &str = "MatchingParameterMeta";

/// Creates a "missing param meta" diagnostic.
fn missing_param_meta(parent: &SectionParent, missing: &str, span: Span) -> Diagnostic {
    let (context, parent) = match parent {
        SectionParent::Task(t) => ("task", t.name()),
        SectionParent::Workflow(w) => ("workflow", w.name()),
        SectionParent::Struct(s) => ("struct", s.name()),
    };

    Diagnostic::warning(format!(
        "{context} `{parent}` is missing a parameter metadata key for input `{missing}`",
        parent = parent.as_str(),
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
        parent = parent.as_str(),
    ))
    .with_rule(ID)
    .with_label(
        "this key does not correspond to any input declaration",
        span,
    )
    .with_fix("remove the extraneous parameter metadata entry")
}

/// Detects missing or extraneous entries in a `parameter_meta` section.
#[derive(Default, Debug, Clone, Copy)]
pub struct MatchingParameterMetaRule {
    /// The version of the WDL document being linted.
    version: Option<SupportedVersion>,
}

impl Rule for MatchingParameterMetaRule {
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
        TagSet::new(&[Tag::Completeness])
    }
}

/// Checks for both missing and extra items in a `parameter_meta` section.
fn check_parameter_meta(
    parent: &SectionParent,
    expected: impl Iterator<Item = (Ident, Span)>,
    param_meta: ParameterMetadataSection,
    diagnostics: &mut Diagnostics,
) {
    let expected: HashMap<_, _> = expected.map(|(i, s)| (i.as_str().to_string(), s)).collect();

    let actual: HashMap<_, _> = param_meta
        .items()
        .map(|m| {
            let name = m.name();
            (name.as_str().to_string(), name.span())
        })
        .collect();

    for (name, span) in &expected {
        if !actual.contains_key(name) {
            diagnostics.add(missing_param_meta(parent, name, *span));
        }
    }

    for (name, span) in &actual {
        if !expected.contains_key(name) {
            diagnostics.add(extra_param_meta(parent, name, *span));
        }
    }
}

impl Visitor for MatchingParameterMetaRule {
    type State = Diagnostics;

    fn document(
        &mut self,
        _: &mut Self::State,
        reason: VisitReason,
        _: &Document,
        version: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Reset the visitor upon document entry
        *self = Default::default();
        self.version = Some(version);
    }

    fn task_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        task: &TaskDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Note that only the first input and parameter_meta sections are checked as any
        // additional sections is considered a validation error
        match task.parameter_metadata().next() {
            Some(param_meta) => {
                check_parameter_meta(
                    &SectionParent::Task(task.clone()),
                    task.inputs().next().iter().flat_map(|i| {
                        i.declarations().map(|d| {
                            let name = d.name();
                            let span = name.span();
                            (name, span)
                        })
                    }),
                    param_meta,
                    state,
                );
            }
            None => {
                // If there is no parameter_meta section, then let the
                // MissingMetas rule handle it
            }
        }
    }

    fn workflow_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        workflow: &WorkflowDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Note that only the first input and parameter_meta sections are checked as any
        // additional sections is considered a validation error
        match workflow.parameter_metadata().next() {
            Some(param_meta) => {
                check_parameter_meta(
                    &SectionParent::Workflow(workflow.clone()),
                    workflow.inputs().next().iter().flat_map(|i| {
                        i.declarations().map(|d| {
                            let name = d.name();
                            let span = name.span();
                            (name, span)
                        })
                    }),
                    param_meta,
                    state,
                );
            }
            None => {
                // If there is no parameter_meta section, then let the
                // MissingMetas rule handle it
            }
        }
    }

    fn struct_definition(
        &mut self,
        state: &mut Self::State,
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
                    def.members().map(|d| {
                        let name = d.name();
                        let span = name.span();
                        (name, span)
                    }),
                    param_meta,
                    state,
                );
            }
            None => {
                // If there is no parameter_meta section, then let the
                // MissingMetas rule handle it
            }
        }
    }
}
