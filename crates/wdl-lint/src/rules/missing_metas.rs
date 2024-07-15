//! A lint rule for missing meta and parameter_meta sections.

use std::fmt;

use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::WorkflowDefinition;
use wdl_ast::version::V1;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::Ident;
use wdl_ast::SupportedVersion;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;

use crate::Rule;
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
    /// A struct.
    Struct,
}

impl fmt::Display for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Task => write!(f, "task"),
            Self::Workflow => write!(f, "workflow"),
            Self::Struct => write!(f, "struct"),
        }
    }
}

/// The identifier for the missing meta sections rule.
const ID: &str = "MissingMetas";

/// Creates a "missing section" diagnostic.
fn missing_section(name: Ident, section: Section, context: Context) -> Diagnostic {
    Diagnostic::note(format!(
        "{context} `{name}` is missing a `{section}` section",
        name = name.as_str(),
    ))
    .with_rule(ID)
    .with_label(
        format!("this {context} is missing a `{section}` section"),
        name.span(),
    )
    .with_fix(format!("add a `{section}` section to the {context}"))
}

/// Creates a "missing sections" diagnostic.
fn missing_sections(name: Ident, context: Context) -> Diagnostic {
    Diagnostic::note(format!(
        "{context} `{name}` is missing both meta and parameter_meta sections",
        name = name.as_str(),
    ))
    .with_rule(ID)
    .with_label(
        format!("this {context} is missing both meta and parameter_meta sections"),
        name.span(),
    )
    .with_fix(format!(
        "add meta and parameter_meta sections to the {context}"
    ))
}

/// A lint rule for missing meta and parameter_meta sections.
#[derive(Default, Debug, Clone, Copy)]
pub struct MissingMetasRule {
    /// The version of the WDL document being linted.
    version: Option<SupportedVersion>,
}

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
}

impl Visitor for MissingMetasRule {
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

        let inputs_present = task.inputs().next().is_some();

        if inputs_present
            && task.metadata().next().is_none()
            && task.parameter_metadata().next().is_none()
        {
            state.add(missing_sections(task.name(), Context::Task));
        } else if task.metadata().next().is_none() {
            state.add(missing_section(task.name(), Section::Meta, Context::Task));
        } else if inputs_present && task.parameter_metadata().next().is_none() {
            state.add(missing_section(
                task.name(),
                Section::ParameterMeta,
                Context::Task,
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
            state.add(missing_sections(workflow.name(), Context::Workflow));
        } else if workflow.metadata().next().is_none() {
            state.add(missing_section(
                workflow.name(),
                Section::Meta,
                Context::Workflow,
            ));
        } else if inputs_present && workflow.parameter_metadata().next().is_none() {
            state.add(missing_section(
                workflow.name(),
                Section::ParameterMeta,
                Context::Workflow,
            ));
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

        if def.metadata().next().is_none() && def.parameter_metadata().next().is_none() {
            state.add(missing_sections(def.name(), Context::Struct));
        } else if def.metadata().next().is_none() {
            state.add(missing_section(def.name(), Section::Meta, Context::Struct));
        } else if def.parameter_metadata().next().is_none() {
            state.add(missing_section(
                def.name(),
                Section::ParameterMeta,
                Context::Struct,
            ));
        }
    }
}
