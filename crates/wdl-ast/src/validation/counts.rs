//! Validation of various counts in an AST.

use std::collections::HashSet;
use std::fmt;

use crate::Ast;
use crate::AstNode;
use crate::AstToken;
use crate::Diagnostic;
use crate::Diagnostics;
use crate::Document;
use crate::Ident;
use crate::Span;
use crate::SupportedVersion;
use crate::SyntaxNode;
use crate::TokenText;
use crate::VisitReason;
use crate::Visitor;
use crate::v1::CommandKeyword;
use crate::v1::CommandSection;
use crate::v1::HintsKeyword;
use crate::v1::InputKeyword;
use crate::v1::InputSection;
use crate::v1::MetaKeyword;
use crate::v1::MetadataSection;
use crate::v1::OutputKeyword;
use crate::v1::OutputSection;
use crate::v1::ParameterMetaKeyword;
use crate::v1::ParameterMetadataSection;
use crate::v1::RequirementsKeyword;
use crate::v1::RequirementsSection;
use crate::v1::RuntimeKeyword;
use crate::v1::RuntimeSection;
use crate::v1::SectionParent;
use crate::v1::StructDefinition;
use crate::v1::TaskDefinition;
use crate::v1::TaskHintsSection;
use crate::v1::WorkflowDefinition;

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
    /// The error occurred in a hints section.
    Hints,
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
            Self::Hints => write!(f, "hints"),
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
        task = task.text()
    ))
    .with_label("this task must have a command section", task.span())
}

/// Gets the span of the keyword token for a given section node.
fn keyword_span(node: &impl AstNode<SyntaxNode>, section: Section) -> Span {
    match section {
        Section::Command => node
            .token::<CommandKeyword<_>>()
            .expect("should have keyword")
            .span(),
        Section::Input => node
            .token::<InputKeyword<_>>()
            .expect("should have keyword")
            .span(),
        Section::Output => node
            .token::<OutputKeyword<_>>()
            .expect("should have keyword")
            .span(),
        Section::Requirements => node
            .token::<RequirementsKeyword<_>>()
            .expect("should have keyword")
            .span(),
        Section::Hints => node
            .token::<HintsKeyword<_>>()
            .expect("should have keyword")
            .span(),
        Section::Runtime => node
            .token::<RuntimeKeyword<_>>()
            .expect("should have keyword")
            .span(),
        Section::Metadata => node
            .token::<MetaKeyword<_>>()
            .expect("should have keyword")
            .span(),
        Section::ParameterMetadata => node
            .token::<ParameterMetaKeyword<_>>()
            .expect("should have keyword")
            .span(),
    }
}

/// Creates a "duplicate section" diagnostic
fn duplicate_section(
    parent: SectionParent,
    section: Section,
    first: Span,
    duplicate: &impl AstNode<SyntaxNode>,
) -> Diagnostic {
    let keyword_span = keyword_span(duplicate, section);
    let (context, name) = match parent {
        SectionParent::Task(t) => ("task", t.name()),
        SectionParent::Workflow(w) => ("workflow", w.name()),
        SectionParent::Struct(w) => ("struct", w.name()),
    };

    Diagnostic::error(format!(
        "{context} `{name}` contains a duplicate {section} section",
        name = name.text()
    ))
    .with_label(
        format!("this {section} section is a duplicate"),
        keyword_span,
    )
    .with_label(format!("first {section} section is defined here"), first)
}

/// Creates the "conflicting section" diagnostic
fn conflicting_section(
    parent: SectionParent,
    section: Section,
    conflicting: &impl AstNode<SyntaxNode>,
    first_span: Span,
    first_section: Section,
) -> Diagnostic {
    let keyword_span = keyword_span(conflicting, section);
    let (context, name) = match parent {
        SectionParent::Task(t) => ("task", t.name()),
        SectionParent::Workflow(w) => ("workflow", w.name()),
        SectionParent::Struct(w) => ("struct", w.name()),
    };

    Diagnostic::error(format!(
        "{context} `{name}` contains a conflicting section",
        name = name.text()
    ))
    .with_label(
        format!("this {section} section conflicts with a {first_section} section"),
        keyword_span,
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
        name = name.text()
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
/// * Contains at most one hints section in a task
/// * Contains at most one meta section in a task or workflow
/// * Contains at most one parameter meta section in a task or workflow
/// * Contains non-empty structs
#[derive(Default, Debug)]
pub struct CountingVisitor {
    /// Keeps track of what task names we've seen.
    tasks_seen: HashSet<TokenText>,
    /// Whether or not we should ignore the task or workflow.
    ignore_current: bool,
    /// Whether or not the document has at least one workflow.
    has_workflow: bool,
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
    /// The span of the first hints section in the task.
    hints: Option<Span>,
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
        self.ignore_current = false;
        self.command = None;
        self.input = None;
        self.output = None;
        self.requirements = None;
        self.hints = None;
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

        if !self.has_workflow && self.tasks_seen.is_empty() && !self.has_struct {
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

        self.ignore_current = self.has_workflow;
        self.has_workflow = true;
    }

    fn task_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        task: &TaskDefinition,
    ) {
        if reason == VisitReason::Exit {
            if !self.ignore_current && self.command.is_none() {
                state.add(missing_command_section(task.name()));
            }

            self.reset();
            return;
        }

        self.ignore_current = !self.tasks_seen.insert(task.name().hashable());
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
        if self.ignore_current || reason == VisitReason::Exit {
            return;
        }

        if let Some(command) = self.command {
            state.add(duplicate_section(
                section.parent(),
                Section::Command,
                command,
                section,
            ));
            return;
        }

        let token: CommandKeyword<_> = section
            .token()
            .expect("should have a command keyword token");
        self.command = Some(token.span());
    }

    fn input_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &InputSection,
    ) {
        if self.ignore_current || reason == VisitReason::Exit {
            return;
        }

        if let Some(input) = self.input {
            state.add(duplicate_section(
                section.parent(),
                Section::Input,
                input,
                section,
            ));
            return;
        }

        let token: InputKeyword<_> = section.token().expect("should have an input keyword token");
        self.input = Some(token.span());
    }

    fn output_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &OutputSection,
    ) {
        if self.ignore_current || reason == VisitReason::Exit {
            return;
        }

        if let Some(output) = self.output {
            state.add(duplicate_section(
                section.parent(),
                Section::Output,
                output,
                section,
            ));
            return;
        }

        let token: OutputKeyword<_> = section
            .token()
            .expect("should have an output keyword token");
        self.output = Some(token.span());
    }

    fn requirements_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &RequirementsSection,
    ) {
        if self.ignore_current || reason == VisitReason::Exit {
            return;
        }

        if let Some(requirements) = self.requirements {
            state.add(duplicate_section(
                section.parent(),
                Section::Requirements,
                requirements,
                section,
            ));
            return;
        }

        if let Some(runtime) = self.runtime {
            state.add(conflicting_section(
                section.parent(),
                Section::Requirements,
                section,
                runtime,
                Section::Runtime,
            ));
            return;
        }

        let token: RequirementsKeyword<_> = section
            .token()
            .expect("should have a requirements keyword token");
        self.requirements = Some(token.span());
    }

    fn task_hints_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &TaskHintsSection,
    ) {
        if self.ignore_current || reason == VisitReason::Exit {
            return;
        }

        if let Some(hints) = self.hints {
            state.add(duplicate_section(
                SectionParent::Task(section.parent()),
                Section::Hints,
                hints,
                section,
            ));
            return;
        }

        let token: HintsKeyword<_> = section.token().expect("should have a hints keyword token");
        self.hints = Some(token.span());
    }

    fn workflow_hints_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &crate::v1::WorkflowHintsSection,
    ) {
        if self.ignore_current || reason == VisitReason::Exit {
            return;
        }

        if let Some(hints) = self.hints {
            state.add(duplicate_section(
                SectionParent::Workflow(section.parent()),
                Section::Hints,
                hints,
                section,
            ));
            return;
        }

        let token: HintsKeyword<_> = section.token().expect("should have a hints keyword token");
        self.hints = Some(token.span());
    }

    fn runtime_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &RuntimeSection,
    ) {
        if self.ignore_current || reason == VisitReason::Exit {
            return;
        }

        if let Some(runtime) = self.runtime {
            state.add(duplicate_section(
                section.parent(),
                Section::Runtime,
                runtime,
                section,
            ));
            return;
        }

        if let Some(requirements) = self.requirements {
            state.add(conflicting_section(
                section.parent(),
                Section::Runtime,
                section,
                requirements,
                Section::Requirements,
            ));
            return;
        }

        let token: RuntimeKeyword<_> = section
            .token()
            .expect("should have a runtime keyword token");
        self.runtime = Some(token.span());
    }

    fn metadata_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &MetadataSection,
    ) {
        if self.ignore_current || reason == VisitReason::Exit {
            return;
        }

        if let Some(metadata) = self.metadata {
            state.add(duplicate_section(
                section.parent(),
                Section::Metadata,
                metadata,
                section,
            ));
            return;
        }

        let token: MetaKeyword<_> = section.token().expect("should have a meta keyword token");
        self.metadata = Some(token.span());
    }

    fn parameter_metadata_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &ParameterMetadataSection,
    ) {
        if self.ignore_current || reason == VisitReason::Exit {
            return;
        }

        if let Some(metadata) = self.param_metadata {
            state.add(duplicate_section(
                section.parent(),
                Section::ParameterMetadata,
                metadata,
                section,
            ));
            return;
        }

        let token: ParameterMetaKeyword<_> = section
            .token()
            .expect("should have a parameter meta keyword token");
        self.param_metadata = Some(token.span());
    }
}
