//! Validation of various counts in a V1 AST.

use std::fmt;

use miette::Diagnostic;
use miette::SourceSpan;
use rowan::ast::support;
use rowan::ast::AstNode;
use wdl_grammar::experimental::tree::SyntaxKind;
use wdl_grammar::experimental::tree::SyntaxNode;

use crate::experimental::to_source_span;
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
use crate::experimental::Diagnostics;
use crate::experimental::Document;
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

/// Represents a count validation error.
#[derive(thiserror::Error, Diagnostic, Debug, Clone, PartialEq, Eq)]
enum Error {
    /// There must be at least one definition in the file.
    #[error("there must be at least one task, workflow, or struct definition in the file")]
    AtLeastOneDefinition,
    /// Too many workflows are present in the file.
    #[error("cannot define workflow `{workflow}` as only one workflow is allowed per source file")]
    DuplicateWorkflow {
        /// The name of the workflow exceeding the maximum.
        workflow: String,
        /// The span of the exceeding workflow.
        #[label(primary, "consider moving this workflow to a new file")]
        span: SourceSpan,
        /// The span of the first workflow.
        #[label("first workflow is defined here")]
        first: SourceSpan,
    },
    /// Missing command section in a task.
    #[error("task `{task}` is missing a command section")]
    MissingCommandSection {
        /// The name of the task or workflow.
        task: String,
        /// The span of the task missing the command section.
        #[label(primary, "this task must have a command section")]
        span: SourceSpan,
    },
    /// A duplicate section was encountered in a task or workflow.
    #[error("{context} `{name}` contains a duplicate {section} section")]
    DuplicateSection {
        /// The error context.
        context: Context,
        /// The name of the task or workflow.
        name: String,
        /// The error section context.
        section: Section,
        /// The span of the duplicate section.
        #[label(primary, "this {section} section is a duplicate")]
        span: SourceSpan,
        /// The span of the original section.
        #[label("first {section} section is defined here")]
        first: SourceSpan,
    },
    /// A struct must have at least one field.
    #[error("struct `{name}` must have at least one declared member")]
    EmptyStruct {
        /// The name of empty struct.
        name: String,
        /// The span of the duplicate section.
        #[label(primary, "this struct cannot be empty")]
        span: SourceSpan,
    },
}

/// Adds a "duplicate section" diagnostic.
fn duplicate_section(
    context: Context,
    name: String,
    duplicate: &SyntaxNode,
    section: Section,
    first: SourceSpan,
    diagnostics: &mut Diagnostics,
) {
    let kind = match section {
        Section::Command => SyntaxKind::CommandKeyword,
        Section::Input => SyntaxKind::InputKeyword,
        Section::Output => SyntaxKind::OutputKeyword,
        Section::Runtime => SyntaxKind::RuntimeKeyword,
        Section::Metadata => SyntaxKind::MetaKeyword,
        Section::ParameterMetadata => SyntaxKind::ParameterMetaKeyword,
    };

    let token = support::token(duplicate, kind).expect("should have keyword token");
    diagnostics.add(Error::DuplicateSection {
        context,
        name,
        section,
        span: to_source_span(token.text_range()),
        first,
    });
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
    workflow: Option<SourceSpan>,
    /// Whether or not the document has at least one task.
    has_task: bool,
    /// Whether or not the document has at least one struct.
    has_struct: bool,
    /// The context of the current task or workflow.
    context: Option<(Context, String)>,
    /// The span of the first command section in the task.
    command: Option<SourceSpan>,
    /// The span of the first input section in the task or workflow.
    input: Option<SourceSpan>,
    /// The span of the first output section in the task or workflow.
    output: Option<SourceSpan>,
    /// The span of the first runtime section in the task.
    runtime: Option<SourceSpan>,
    /// The span of the first metadata section in the task or workflow.
    metadata: Option<SourceSpan>,
    /// The span of the first parameter metadata section in the task or
    /// workflow.
    param_metadata: Option<SourceSpan>,
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
            state.add(Error::AtLeastOneDefinition);
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
            let name = workflow.name();
            state.add(Error::DuplicateWorkflow {
                workflow: name.as_str().to_string(),
                span: to_source_span(name.syntax().text_range()),
                first,
            });
            return;
        }

        let name = workflow.name();
        self.context = Some((Context::Workflow, name.as_str().to_string()));
        self.workflow = Some(to_source_span(name.syntax().text_range()));
    }

    fn task_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        task: &TaskDefinition,
    ) {
        if reason == VisitReason::Exit {
            if self.command.is_none() {
                let name = task.name();
                state.add(Error::MissingCommandSection {
                    task: name.as_str().to_string(),
                    span: to_source_span(name.syntax().text_range()),
                });
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
            let name = def.name();
            state.add(Error::EmptyStruct {
                name: name.as_str().to_string(),
                span: to_source_span(name.syntax().text_range()),
            });
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
            let (context, name) = self.context.clone().expect("should have context");
            duplicate_section(
                context,
                name,
                section.syntax(),
                Section::Command,
                command,
                state,
            );
            return;
        }

        let token = support::token(section.syntax(), SyntaxKind::CommandKeyword)
            .expect("should have a command keyword token");
        self.command = Some(to_source_span(token.text_range()));
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
            let (context, name) = self.context.clone().expect("should have context");
            duplicate_section(
                context,
                name,
                section.syntax(),
                Section::Input,
                input,
                state,
            );
            return;
        }

        let token = support::token(section.syntax(), SyntaxKind::InputKeyword)
            .expect("should have an input keyword token");
        self.input = Some(to_source_span(token.text_range()));
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
            let (context, name) = self.context.clone().expect("should have context");
            duplicate_section(
                context,
                name,
                section.syntax(),
                Section::Output,
                output,
                state,
            );
            return;
        }

        let token = support::token(section.syntax(), SyntaxKind::OutputKeyword)
            .expect("should have an output keyword token");
        self.output = Some(to_source_span(token.text_range()));
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
            let (context, name) = self.context.clone().expect("should have context");
            duplicate_section(
                context,
                name,
                section.syntax(),
                Section::Runtime,
                runtime,
                state,
            );
            return;
        }

        let token = support::token(section.syntax(), SyntaxKind::RuntimeKeyword)
            .expect("should have a runtime keyword token");
        self.runtime = Some(to_source_span(token.text_range()));
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
            let (context, name) = self.context.clone().expect("should have context");
            duplicate_section(
                context,
                name,
                section.syntax(),
                Section::Metadata,
                metadata,
                state,
            );
            return;
        }

        let token = support::token(section.syntax(), SyntaxKind::MetaKeyword)
            .expect("should have a meta keyword token");
        self.metadata = Some(to_source_span(token.text_range()));
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
            let (context, name) = self.context.clone().expect("should have context");
            duplicate_section(
                context,
                name,
                section.syntax(),
                Section::ParameterMetadata,
                metadata,
                state,
            );
            return;
        }

        let token = support::token(section.syntax(), SyntaxKind::ParameterMetaKeyword)
            .expect("should have a parameter meta keyword token");
        self.param_metadata = Some(to_source_span(token.text_range()));
    }
}
