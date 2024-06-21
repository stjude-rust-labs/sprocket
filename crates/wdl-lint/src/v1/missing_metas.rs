//! A lint rule for missing meta and parameter_meta sections.

use std::fmt;

use wdl_ast::v1::TaskDefinition;
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

/// Which section is missing.
enum Section {
    /// The `meta` section is missing.
    Meta,
    /// The `parameter_meta` section is missing.
    ParameterMeta,
}

impl fmt::Display for Section {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Meta => write!(f, "meta"),
            Self::ParameterMeta => write!(f, "parameter_meta"),
        }
    }
}

/// The context for which section is missing.
enum Context {
    /// A task.
    Task,
    /// A workflow.
    Workflow,
}

impl fmt::Display for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Task => write!(f, "task"),
            Self::Workflow => write!(f, "workflow"),
        }
    }
}

/// The identifier for the missing meta sections rule.
const ID: &str = "MissingMetas";

/// Creates a "missing section" diagnostic.
fn missing_section(name: &str, section: Section, context: Context, span: Span) -> Diagnostic {
    Diagnostic::note(format!(
        "{context} `{name}` is missing a `{section}` section"
    ))
    .with_rule(ID)
    .with_label(
        format!("this {context} is missing a `{section}` section"),
        span,
    )
    .with_fix(format!("add a `{section}` section to the {context}"))
}

/// Creates a "missing sections" diagnostic.
fn missing_sections(name: &str, context: Context, span: Span) -> Diagnostic {
    Diagnostic::note(format!(
        "{context} `{name}` is missing both meta and parameter_meta sections"
    ))
    .with_rule(ID)
    .with_label(
        format!("this {context} is missing both meta and parameter_meta sections"),
        span,
    )
    .with_fix(format!(
        "add meta and parameter_meta sections to the {context}"
    ))
}

/// A lint rule for missing meta and parameter_meta sections.
#[derive(Debug, Clone, Copy)]
pub struct MissingMetasRule;

impl Rule for MissingMetasRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that tasks have both a meta and a parameter_meta section."
    }

    fn explanation(&self) -> &'static str {
        "It is important that WDL code is well-documented. Every task and workflow should have \
         both a meta and parameter_meta section. Tasks without an `input` section are permitted to \
         skip the `parameter_meta` section."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Completeness, Tag::Clarity])
    }

    fn visitor(&self) -> Box<dyn Visitor<State = Diagnostics>> {
        Box::new(MissingMetasVisitor)
    }
}

/// Implements the visitor for the missing meta and parameter_meta sections
/// rule.
struct MissingMetasVisitor;

impl Visitor for MissingMetasVisitor {
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

        let inputs_present = task.inputs().next().is_some();

        if inputs_present
            && task.metadata().next().is_none()
            && task.parameter_metadata().next().is_none()
        {
            state.add(missing_sections(
                task.name().as_str(),
                Context::Task,
                task.name().span(),
            ));
        } else if task.metadata().next().is_none() {
            state.add(missing_section(
                task.name().as_str(),
                Section::Meta,
                Context::Task,
                task.name().span(),
            ));
        } else if inputs_present && task.parameter_metadata().next().is_none() {
            state.add(missing_section(
                task.name().as_str(),
                Section::ParameterMeta,
                Context::Task,
                task.name().span(),
            ));
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

        let inputs_present = workflow.inputs().next().is_some();

        if inputs_present
            && workflow.metadata().next().is_none()
            && workflow.parameter_metadata().next().is_none()
        {
            state.add(missing_sections(
                workflow.name().as_str(),
                Context::Workflow,
                workflow.name().span(),
            ));
        } else if workflow.metadata().next().is_none() {
            state.add(missing_section(
                workflow.name().as_str(),
                Section::Meta,
                Context::Workflow,
                workflow.name().span(),
            ));
        } else if inputs_present && workflow.parameter_metadata().next().is_none() {
            state.add(missing_section(
                workflow.name().as_str(),
                Section::ParameterMeta,
                Context::Workflow,
                workflow.name().span(),
            ));
        }
    }
}
