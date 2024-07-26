//! Validation of various counts in an AST.

use std::fmt;

use wdl_grammar::SyntaxToken;

use crate::support;
use crate::v1::CommandSection;
use crate::v1::InputSection;
use crate::v1::MetadataSection;
use crate::v1::OutputSection;
use crate::v1::ParameterMetadataSection;
use crate::v1::RequirementsSection;
use crate::v1::RuntimeSection;
use crate::v1::SectionParent;
use crate::v1::StructDefinition;
use crate::v1::TaskDefinition;
use crate::v1::WorkflowDefinition;
use crate::Ast;
use crate::AstNode;
use crate::AstToken;
use crate::Diagnostic;
use crate::Diagnostics;
use crate::Document;
use crate::Ident;
use crate::Span;
use crate::SupportedVersion;
use crate::SyntaxKind;
use crate::SyntaxNode;
use crate::ToSpan;
use crate::VisitReason;
use crate::Visitor;

/// Represents section context of an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    /// The error occurred in a task command section.
    Command,
    /// The error occurred in an input section.
    Input,
    /// The error occurred in an output section.
    Output,
    /// The error occurred in a requirements section.
    Requirements,
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
            Self::Requirements => write!(f, "requirements"),
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

/// Creates a "missing command section" diagnostic
fn missing_command_section(task: Ident) -> Diagnostic {
    Diagnostic::error(format!(
        "task `{task}` is missing a command section",
        task = task.as_str()
    ))
    .with_label("this task must have a command section", task.span())
}

/// Gets the keyword token for a given section node.
fn keyword(node: &SyntaxNode, section: Section) -> SyntaxToken {
    let kind = match section {
        Section::Command => SyntaxKind::CommandKeyword,
        Section::Input => SyntaxKind::InputKeyword,
        Section::Output => SyntaxKind::OutputKeyword,
        Section::Requirements => SyntaxKind::RequirementsKeyword,
        Section::Runtime => SyntaxKind::RuntimeKeyword,
        Section::Metadata => SyntaxKind::MetaKeyword,
        Section::ParameterMetadata => SyntaxKind::ParameterMetaKeyword,
    };

    support::token(node, kind).expect("should have keyword token")
}

/// Creates a "duplicate section" diagnostic
fn duplicate_section(
    parent: SectionParent,
    section: Section,
    first: Span,
    duplicate: &SyntaxNode,
) -> Diagnostic {
    let token = keyword(duplicate, section);
    let (context, name) = match parent {
        SectionParent::Task(t) => ("task", t.name()),
        SectionParent::Workflow(w) => ("workflow", w.name()),
        SectionParent::Struct(w) => ("struct", w.name()),
    };

    Diagnostic::error(format!(
        "{context} `{name}` contains a duplicate {section} section",
        name = name.as_str()
    ))
    .with_label(
        format!("this {section} section is a duplicate"),
        token.text_range(),
    )
    .with_label(format!("first {section} section is defined here"), first)
}

/// Creates the "conflicting section" diagnostic
fn conflicting_section(
    parent: SectionParent,
    section: Section,
    conflicting: &SyntaxNode,
    first_span: Span,
    first_section: Section,
) -> Diagnostic {
    let token = keyword(conflicting, section);
    let (context, name) = match parent {
        SectionParent::Task(t) => ("task", t.name()),
        SectionParent::Workflow(w) => ("workflow", w.name()),
        SectionParent::Struct(w) => ("struct", w.name()),
    };

    Diagnostic::error(format!(
        "{context} `{name}` contains a conflicting section",
        name = name.as_str()
    ))
    .with_label(
        format!("this {section} section conflicts with a {first_section} section"),
        token.text_range(),
    )
    .with_label(
        format!("the conflicting {first_section} section is defined here"),
        first_span,
    )
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
/// * Contains at most one requirements or runtime section in a task
/// * Contains at most one meta section in a task or workflow
/// * Contains at most one parameter meta section in a task or workflow
/// * Contains non-empty structs
#[derive(Default, Debug)]
pub struct CountingVisitor {
    /// Whether or not the document has at least one workflow.
    has_workflow: bool,
    /// Whether or not the document has at least one task.
    has_task: bool,
    /// Whether or not the document has at least one struct.
    has_struct: bool,
    /// The span of the first command section in the task.
    command: Option<Span>,
    /// The span of the first input section in the task or workflow.
    input: Option<Span>,
    /// The span of the first output section in the task or workflow.
    output: Option<Span>,
    /// The span of the first requirements section in the task.
    requirements: Option<Span>,
    /// The span of the first runtime section in the task.
    runtime: Option<Span>,
    /// The span of the first metadata section in the task, workflow, or struct.
    metadata: Option<Span>,
    /// The span of the first parameter metadata section in the task, workflow,
    /// or struct.
    param_metadata: Option<Span>,
}

impl CountingVisitor {
    /// Resets the task/workflow count state.
    fn reset(&mut self) {
        self.command = None;
        self.input = None;
        self.output = None;
        self.requirements = None;
        self.runtime = None;
        self.metadata = None;
        self.param_metadata = None;
    }
}

impl Visitor for CountingVisitor {
    type State = Diagnostics;

    fn document(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        doc: &Document,
        _: SupportedVersion,
    ) {
        if reason == VisitReason::Enter {
            // Upon entry of of a document, reset the visitor entirely.
            *self = Default::default();
            return;
        }

        // Ignore documents that are not supported
        if matches!(doc.ast(), Ast::Unsupported) {
            return;
        }

        if !self.has_workflow && !self.has_task && !self.has_struct {
            state.add(at_least_one_definition());
        }
    }

    fn workflow_definition(
        &mut self,
        _: &mut Self::State,
        reason: VisitReason,
        _: &WorkflowDefinition,
    ) {
        if reason == VisitReason::Exit {
            self.reset();
            return;
        }

        self.has_workflow = true;
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

        self.has_task = true;
    }

    fn struct_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        def: &StructDefinition,
    ) {
        if reason == VisitReason::Exit {
            self.reset();
            return;
        }

        if def.members().next().is_none() {
            state.add(empty_struct(def.name()));
        }

        self.has_struct = true;
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
            state.add(duplicate_section(
                section.parent(),
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
            state.add(duplicate_section(
                section.parent(),
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
            state.add(duplicate_section(
                section.parent(),
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

    fn requirements_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &RequirementsSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Some(requirements) = self.requirements {
            state.add(duplicate_section(
                section.parent(),
                Section::Requirements,
                requirements,
                section.syntax(),
            ));
            return;
        }

        if let Some(runtime) = self.runtime {
            state.add(conflicting_section(
                section.parent(),
                Section::Requirements,
                section.syntax(),
                runtime,
                Section::Runtime,
            ));
            return;
        }

        let token = support::token(section.syntax(), SyntaxKind::RequirementsKeyword)
            .expect("should have a requirements keyword token");
        self.requirements = Some(token.text_range().to_span());
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
            state.add(duplicate_section(
                section.parent(),
                Section::Runtime,
                runtime,
                section.syntax(),
            ));
            return;
        }

        if let Some(requirements) = self.requirements {
            state.add(conflicting_section(
                section.parent(),
                Section::Runtime,
                section.syntax(),
                requirements,
                Section::Requirements,
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
            state.add(duplicate_section(
                section.parent(),
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
            state.add(duplicate_section(
                section.parent(),
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
