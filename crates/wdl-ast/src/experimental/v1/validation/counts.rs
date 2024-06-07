//! Validation of various counts in a V1 AST.

use std::fmt;

use rowan::ast::support;
use rowan::ast::AstNode;

use crate::experimental::v1::CommandSection;
use crate::experimental::v1::InputSection;
use crate::experimental::v1::MetadataSection;
use crate::experimental::v1::OutputSection;
use crate::experimental::v1::ParameterMetadataSection;
use crate::experimental::v1::RuntimeSection;
use crate::experimental::v1::StructDefinition;
use crate::experimental::v1::TaskDefinition;
use crate::experimental::v1::Visitor;
use crate::experimental::v1::WorkflowDefinition;
use crate::experimental::AstToken;
use crate::experimental::Diagnostic;
use crate::experimental::Diagnostics;
use crate::experimental::Document;
use crate::experimental::Ident;
use crate::experimental::Span;
use crate::experimental::SyntaxKind;
use crate::experimental::SyntaxNode;
use crate::experimental::ToSpan;
use crate::experimental::VisitReason;

/// Represents context of an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Context {
    /// The error occurred in a task.
    Task,
    /// The error occurred in a workflow.
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

/// Represents section context of an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    /// The error occurred in a task command section.
    Command,
    /// The error occurred in an input section.
    Input,
    /// The error occurred in an output section.
    Output,
    /// The error occurred in a task runtime section.
    Runtime,
    /// The error occurred in a metadata section.
    Metadata,
    /// The error occurred in a parameter metadata section.
    ParameterMetadata,
}

impl fmt::Display for Section {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Command => write!(f, "command"),
            Self::Input => write!(f, "input"),
            Self::Output => write!(f, "output"),
            Self::Runtime => write!(f, "runtime"),
            Self::Metadata => write!(f, "metadata"),
            Self::ParameterMetadata => write!(f, "parameter metadata"),
        }
    }
}

/// Creates a "at least one definition" diagnostic
fn at_least_one_definition() -> Diagnostic {
    Diagnostic::error("there must be at least one task, workflow, or struct definition in the file")
}

/// Creates a "duplicate workflow" diagnostic
fn duplicate_workflow(name: Ident, first: Span) -> Diagnostic {
    Diagnostic::error(format!(
        "cannot define workflow `{name}` as only one workflow is allowed per source file",
        name = name.as_str()
    ))
    .with_label("consider moving this workflow to a new file", name.span())
    .with_label("first workflow is defined here", first)
}

/// Creates a "missing command section" diagnostic
fn missing_command_section(task: Ident) -> Diagnostic {
    Diagnostic::error(format!(
        "task `{task}` is missing a command section",
        task = task.as_str()
    ))
    .with_label("this task must have a command section", task.span())
}

/// Creates a "duplicate section" diagnostic
fn duplicate_section(
    context: Context,
    name: &str,
    section: Section,
    first: Span,
    duplicate: &SyntaxNode,
) -> Diagnostic {
    let kind = match section {
        Section::Command => SyntaxKind::CommandKeyword,
        Section::Input => SyntaxKind::InputKeyword,
        Section::Output => SyntaxKind::OutputKeyword,
        Section::Runtime => SyntaxKind::RuntimeKeyword,
        Section::Metadata => SyntaxKind::MetaKeyword,
        Section::ParameterMetadata => SyntaxKind::ParameterMetaKeyword,
    };

    let token = support::token(duplicate, kind).expect("should have keyword token");

    Diagnostic::error(format!(
        "{context} `{name}` contains a duplicate {section} section"
    ))
    .with_label(
        format!("this {section} section is a duplicate"),
        token.text_range(),
    )
    .with_label(format!("first {section} section is defined here"), first)
}

/// Creates an "empty struct" diagnostic
fn empty_struct(name: Ident) -> Diagnostic {
    Diagnostic::error(format!(
        "struct `{name}` must have at least one declared member",
        name = name.as_str()
    ))
    .with_label("this struct cannot be empty", name.span())
}

/// A visitor for counting items within an AST.
///
/// Ensures that a document:
///
/// * Contains at least one definition in the file
/// * Contains at most one workflow
/// * Contains exactly one command section in every task
/// * Contains at most one input section in a task or workflow
/// * Contains at most one output section in a task or workflow
/// * Contains at most one runtime section in a task
/// * Contains at most one meta section in a task or workflow
/// * Contains at most one parameter meta section in a task or workflow
/// * Contains non-empty structs
#[derive(Default, Debug)]
pub struct CountingVisitor {
    /// The span of the first workflow in the file.
    workflow: Option<Span>,
    /// Whether or not the document has at least one task.
    has_task: bool,
    /// Whether or not the document has at least one struct.
    has_struct: bool,
    /// The context of the current task or workflow.
    context: Option<(Context, String)>,
    /// The span of the first command section in the task.
    command: Option<Span>,
    /// The span of the first input section in the task or workflow.
    input: Option<Span>,
    /// The span of the first output section in the task or workflow.
    output: Option<Span>,
    /// The span of the first runtime section in the task.
    runtime: Option<Span>,
    /// The span of the first metadata section in the task or workflow.
    metadata: Option<Span>,
    /// The span of the first parameter metadata section in the task or
    /// workflow.
    param_metadata: Option<Span>,
}

impl CountingVisitor {
    /// Resets the task/workflow count state.
    fn reset(&mut self) {
        self.context = None;
        self.command = None;
        self.input = None;
        self.output = None;
        self.runtime = None;
        self.metadata = None;
        self.param_metadata = None;
    }
}

impl Visitor for CountingVisitor {
    type State = Diagnostics;

    fn document(&mut self, state: &mut Self::State, reason: VisitReason, _: &Document) {
        if reason == VisitReason::Enter {
            return;
        }

        if self.workflow.is_none() && !self.has_task && !self.has_struct {
            state.add(at_least_one_definition());
        }
    }

    fn workflow_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        workflow: &WorkflowDefinition,
    ) {
        if reason == VisitReason::Exit {
            self.reset();
            return;
        }

        if let Some(first) = self.workflow {
            state.add(duplicate_workflow(workflow.name(), first));
            return;
        }

        let name = workflow.name();
        self.context = Some((Context::Workflow, name.as_str().to_string()));
        self.workflow = Some(name.span());
    }

    fn task_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        task: &TaskDefinition,
    ) {
        if reason == VisitReason::Exit {
            if self.command.is_none() {
                state.add(missing_command_section(task.name()));
            }

            self.reset();
            return;
        }

        self.context = Some((Context::Task, task.name().as_str().to_string()));
        self.has_task = true;
    }

    fn struct_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        def: &StructDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if def.members().next().is_none() {
            state.add(empty_struct(def.name()));
        }

        self.has_task = true;
    }

    fn command_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &CommandSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Some(command) = self.command {
            let (context, name) = self.context.as_ref().expect("should have context");
            state.add(duplicate_section(
                *context,
                name,
                Section::Command,
                command,
                section.syntax(),
            ));
            return;
        }

        let token = support::token(section.syntax(), SyntaxKind::CommandKeyword)
            .expect("should have a command keyword token");
        self.command = Some(token.text_range().to_span());
    }

    fn input_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &InputSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Some(input) = self.input {
            let (context, name) = self.context.as_ref().expect("should have context");
            state.add(duplicate_section(
                *context,
                name,
                Section::Input,
                input,
                section.syntax(),
            ));
            return;
        }

        let token = support::token(section.syntax(), SyntaxKind::InputKeyword)
            .expect("should have an input keyword token");
        self.input = Some(token.text_range().to_span());
    }

    fn output_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &OutputSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Some(output) = self.output {
            let (context, name) = self.context.as_ref().expect("should have context");
            state.add(duplicate_section(
                *context,
                name,
                Section::Output,
                output,
                section.syntax(),
            ));
            return;
        }

        let token = support::token(section.syntax(), SyntaxKind::OutputKeyword)
            .expect("should have an output keyword token");
        self.output = Some(token.text_range().to_span());
    }

    fn runtime_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &RuntimeSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Some(runtime) = self.runtime {
            let (context, name) = self.context.as_ref().expect("should have context");
            state.add(duplicate_section(
                *context,
                name,
                Section::Runtime,
                runtime,
                section.syntax(),
            ));
            return;
        }

        let token = support::token(section.syntax(), SyntaxKind::RuntimeKeyword)
            .expect("should have a runtime keyword token");
        self.runtime = Some(token.text_range().to_span());
    }

    fn metadata_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &MetadataSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Some(metadata) = self.metadata {
            let (context, name) = self.context.as_ref().expect("should have context");
            state.add(duplicate_section(
                *context,
                name,
                Section::Metadata,
                metadata,
                section.syntax(),
            ));
            return;
        }

        let token = support::token(section.syntax(), SyntaxKind::MetaKeyword)
            .expect("should have a meta keyword token");
        self.metadata = Some(token.text_range().to_span());
    }

    fn parameter_metadata_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &ParameterMetadataSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Some(metadata) = self.param_metadata {
            let (context, name) = self.context.as_ref().expect("should have context");
            state.add(duplicate_section(
                *context,
                name,
                Section::ParameterMetadata,
                metadata,
                section.syntax(),
            ));
            return;
        }

        let token = support::token(section.syntax(), SyntaxKind::ParameterMetaKeyword)
            .expect("should have a parameter meta keyword token");
        self.param_metadata = Some(token.text_range().to_span());
    }
}
