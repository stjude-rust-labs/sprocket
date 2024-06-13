//! A lint rule for matching parameter metadata.

use std::collections::HashMap;

use wdl_ast::v1::InputSection;
use wdl_ast::v1::ParameterMetadataSection;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::TaskOrWorkflow;
use wdl_ast::v1::Visitor;
use wdl_ast::v1::WorkflowDefinition;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Span;
use wdl_ast::VisitReason;

use super::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the matching parameter meta rule.
const ID: &str = "MatchingParameterMeta";

/// Creates a "missing param meta" diagnostic.
fn missing_param_meta(parent: &TaskOrWorkflow, missing: &str, span: Span) -> Diagnostic {
    let (context, parent) = match parent {
        TaskOrWorkflow::Task(t) => ("task", t.name()),
        TaskOrWorkflow::Workflow(w) => ("workflow", w.name()),
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
fn extra_param_meta(parent: &TaskOrWorkflow, extra: &str, span: Span) -> Diagnostic {
    let (context, parent) = match parent {
        TaskOrWorkflow::Task(t) => ("task", t.name()),
        TaskOrWorkflow::Workflow(w) => ("workflow", w.name()),
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
#[derive(Debug, Clone, Copy)]
pub struct MatchingParameterMetaRule;

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

    fn visitor(&self) -> Box<dyn Visitor<State = Diagnostics>> {
        Box::new(MatchingParameterMetaVisitor)
    }
}

/// Checks for both missing and extra items in a `parameter_meta` section.
fn check_parameter_meta(
    parent: TaskOrWorkflow,
    inputs: Option<InputSection>,
    param_meta: Option<ParameterMetadataSection>,
    diagnostics: &mut Diagnostics,
) {
    let expected: HashMap<_, _> = inputs
        .iter()
        .flat_map(|i| {
            i.declarations().map(|d| {
                let name = d.name();
                (name.as_str().to_string(), name.span())
            })
        })
        .collect();

    let actual: HashMap<_, _> = param_meta
        .iter()
        .flat_map(|m| {
            m.items().map(|i| {
                let name = i.name();
                (name.as_str().to_string(), name.span())
            })
        })
        .collect();

    for (name, span) in &expected {
        if !actual.contains_key(name) {
            diagnostics.add(missing_param_meta(&parent, name, *span));
        }
    }

    for (name, span) in &actual {
        if !expected.contains_key(name) {
            diagnostics.add(extra_param_meta(&parent, name, *span));
        }
    }
}

/// Implements the visitor for the matching parameter meta rule.
struct MatchingParameterMetaVisitor;

impl Visitor for MatchingParameterMetaVisitor {
    type State = Diagnostics;

    fn task_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        task: &TaskDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Check the parameter metadata of the task
        // Note that only the first input and parameter_meta sections are checked as any
        // additional sections is considered a validation error
        check_parameter_meta(
            TaskOrWorkflow::Task(task.clone()),
            task.inputs().next(),
            task.parameter_metadata().next(),
            state,
        );
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

        // Check the parameter metadata of the workflow
        // Note that only the first input and parameter_meta sections are checked as any
        // additional sections is considered a validation error
        check_parameter_meta(
            TaskOrWorkflow::Workflow(workflow.clone()),
            workflow.inputs().next(),
            workflow.parameter_metadata().next(),
            state,
        );
    }
}
