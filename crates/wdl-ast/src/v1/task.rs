//! V1 AST representation for task definitions.

use super::BoundDecl;
use super::Decl;
use super::Expr;
use super::LiteralBoolean;
use super::LiteralFloat;
use super::LiteralInteger;
use super::LiteralString;
use super::Placeholder;
use super::StructDefinition;
use super::WorkflowDefinition;
use crate::AstChildren;
use crate::AstNode;
use crate::AstToken;
use crate::Ident;
use crate::SyntaxElement;
use crate::SyntaxKind;
use crate::SyntaxNode;
use crate::SyntaxToken;
use crate::WorkflowDescriptionLanguage;
use crate::support;
use crate::support::child;
use crate::support::children;
use crate::token;

pub mod common;
pub mod requirements;
pub mod runtime;

/// Represents a task definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskDefinition(pub(crate) SyntaxNode);

impl TaskDefinition {
    /// Gets the name of the task.
    pub fn name(&self) -> Ident {
        token(&self.0).expect("task should have a name")
    }

    /// Gets the items of the task.
    pub fn items(&self) -> impl Iterator<Item = TaskItem> + use<> {
        TaskItem::children(&self.0)
    }

    /// Gets the input section of the task.
    pub fn input(&self) -> Option<InputSection> {
        child(&self.0)
    }

    /// Gets the output section of the task.
    pub fn output(&self) -> Option<OutputSection> {
        child(&self.0)
    }

    /// Gets the command section of the task.
    pub fn command(&self) -> Option<CommandSection> {
        child(&self.0)
    }

    /// Gets the requirements sections of the task.
    pub fn requirements(&self) -> Option<RequirementsSection> {
        child(&self.0)
    }

    /// Gets the hints section of the task.
    pub fn hints(&self) -> Option<TaskHintsSection> {
        child(&self.0)
    }

    /// Gets the runtime section of the task.
    pub fn runtime(&self) -> Option<RuntimeSection> {
        child(&self.0)
    }

    /// Gets the metadata section of the task.
    pub fn metadata(&self) -> Option<MetadataSection> {
        child(&self.0)
    }

    /// Gets the parameter section of the task.
    pub fn parameter_metadata(&self) -> Option<ParameterMetadataSection> {
        child(&self.0)
    }

    /// Gets the private declarations of the task.
    pub fn declarations(&self) -> AstChildren<BoundDecl> {
        children(&self.0)
    }
}

impl AstNode for TaskDefinition {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::TaskDefinitionNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::TaskDefinitionNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents an item in a task definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskItem {
    /// The item is an input section.
    Input(InputSection),
    /// The item is an output section.
    Output(OutputSection),
    /// The item is a command section.
    Command(CommandSection),
    /// The item is a requirements section.
    Requirements(RequirementsSection),
    /// The item is a task hints section.
    Hints(TaskHintsSection),
    /// The item is a runtime section.
    Runtime(RuntimeSection),
    /// The item is a metadata section.
    Metadata(MetadataSection),
    /// The item is a parameter meta section.
    ParameterMetadata(ParameterMetadataSection),
    /// The item is a private bound declaration.
    Declaration(BoundDecl),
}

impl TaskItem {
    /// Returns whether or not a [`SyntaxKind`] is able to be cast to any of the
    /// underlying members within the [`TaskItem`].
    pub fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(
            kind,
            SyntaxKind::InputSectionNode
                | SyntaxKind::OutputSectionNode
                | SyntaxKind::CommandSectionNode
                | SyntaxKind::RequirementsSectionNode
                | SyntaxKind::TaskHintsSectionNode
                | SyntaxKind::RuntimeSectionNode
                | SyntaxKind::MetadataSectionNode
                | SyntaxKind::ParameterMetadataSectionNode
                | SyntaxKind::BoundDeclNode
        )
    }

    /// Attempts to cast the [`SyntaxNode`] to any of the underlying members
    /// within the [`TaskItem`].
    pub fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::InputSectionNode => Some(Self::Input(
                InputSection::cast(syntax).expect("input section to cast"),
            )),
            SyntaxKind::OutputSectionNode => Some(Self::Output(
                OutputSection::cast(syntax).expect("output section to cast"),
            )),
            SyntaxKind::CommandSectionNode => Some(Self::Command(
                CommandSection::cast(syntax).expect("command section to cast"),
            )),
            SyntaxKind::RequirementsSectionNode => Some(Self::Requirements(
                RequirementsSection::cast(syntax).expect("requirements section to cast"),
            )),
            SyntaxKind::RuntimeSectionNode => Some(Self::Runtime(
                RuntimeSection::cast(syntax).expect("runtime section to cast"),
            )),
            SyntaxKind::MetadataSectionNode => Some(Self::Metadata(
                MetadataSection::cast(syntax).expect("metadata section to cast"),
            )),
            SyntaxKind::ParameterMetadataSectionNode => Some(Self::ParameterMetadata(
                ParameterMetadataSection::cast(syntax).expect("parameter metadata section to cast"),
            )),
            SyntaxKind::TaskHintsSectionNode => Some(Self::Hints(
                TaskHintsSection::cast(syntax).expect("task hints section to cast"),
            )),
            SyntaxKind::BoundDeclNode => Some(Self::Declaration(
                BoundDecl::cast(syntax).expect("bound decl to cast"),
            )),
            _ => None,
        }
    }

    /// Gets a reference to the underlying [`SyntaxNode`].
    pub fn syntax(&self) -> &SyntaxNode {
        match self {
            Self::Input(element) => element.syntax(),
            Self::Output(element) => element.syntax(),
            Self::Command(element) => element.syntax(),
            Self::Requirements(element) => element.syntax(),
            Self::Hints(element) => element.syntax(),
            Self::Runtime(element) => element.syntax(),
            Self::Metadata(element) => element.syntax(),
            Self::ParameterMetadata(element) => element.syntax(),
            Self::Declaration(element) => element.syntax(),
        }
    }

    /// Attempts to get a reference to the inner [`InputSection`].
    ///
    /// * If `self` is a [`TaskItem::Input`], then a reference to the inner
    ///   [`InputSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_input_section(&self) -> Option<&InputSection> {
        match self {
            Self::Input(input_section) => Some(input_section),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`InputSection`].
    ///
    /// * If `self` is a [`TaskItem::Input`], then the inner [`InputSection`] is
    ///   returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_input_section(self) -> Option<InputSection> {
        match self {
            Self::Input(input_section) => Some(input_section),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`OutputSection`].
    ///
    /// * If `self` is a [`TaskItem::Output`], then a reference to the inner
    ///   [`OutputSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_output_section(&self) -> Option<&OutputSection> {
        match self {
            Self::Output(output_section) => Some(output_section),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`OutputSection`].
    ///
    /// * If `self` is a [`TaskItem::Output`], then the inner [`OutputSection`]
    ///   is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_output_section(self) -> Option<OutputSection> {
        match self {
            Self::Output(output_section) => Some(output_section),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`CommandSection`].
    ///
    /// * If `self` is a [`TaskItem::Command`], then a reference to the inner
    ///   [`CommandSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_command_section(&self) -> Option<&CommandSection> {
        match self {
            Self::Command(command_section) => Some(command_section),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`CommandSection`].
    ///
    /// * If `self` is a [`TaskItem::Command`], then the inner
    ///   [`CommandSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_command_section(self) -> Option<CommandSection> {
        match self {
            Self::Command(command_section) => Some(command_section),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`RequirementsSection`].
    ///
    /// * If `self` is a [`TaskItem::Requirements`], then a reference to the
    ///   inner [`RequirementsSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_requirements_section(&self) -> Option<&RequirementsSection> {
        match self {
            Self::Requirements(requirements_section) => Some(requirements_section),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner
    /// [`RequirementsSection`].
    ///
    /// * If `self` is a [`TaskItem::Requirements`], then the inner
    ///   [`RequirementsSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_requirements_section(self) -> Option<RequirementsSection> {
        match self {
            Self::Requirements(requirements_section) => Some(requirements_section),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`TaskHintsSection`].
    ///
    /// * If `self` is a [`TaskItem::Hints`], then a reference to the inner
    ///   [`TaskHintsSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_hints_section(&self) -> Option<&TaskHintsSection> {
        match self {
            Self::Hints(hints_section) => Some(hints_section),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`TaskHintsSection`].
    ///
    /// * If `self` is a [`TaskItem::Hints`], then the inner
    ///   [`TaskHintsSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_hints_section(self) -> Option<TaskHintsSection> {
        match self {
            Self::Hints(hints_section) => Some(hints_section),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`RuntimeSection`].
    ///
    /// * If `self` is a [`TaskItem::Runtime`], then a reference to the inner
    ///   [`RuntimeSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_runtime_section(&self) -> Option<&RuntimeSection> {
        match self {
            Self::Runtime(runtime_section) => Some(runtime_section),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`RuntimeSection`].
    ///
    /// * If `self` is a [`TaskItem::Runtime`], then the inner
    ///   [`RuntimeSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_runtime_section(self) -> Option<RuntimeSection> {
        match self {
            Self::Runtime(runtime_section) => Some(runtime_section),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`MetadataSection`].
    ///
    /// * If `self` is a [`TaskItem::Metadata`], then a reference to the inner
    ///   [`MetadataSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_metadata_section(&self) -> Option<&MetadataSection> {
        match self {
            Self::Metadata(metadata_section) => Some(metadata_section),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`MetadataSection`].
    ///
    /// * If `self` is a [`TaskItem::Metadata`], then the inner
    ///   [`MetadataSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_metadata_section(self) -> Option<MetadataSection> {
        match self {
            Self::Metadata(metadata_section) => Some(metadata_section),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`ParameterMetadataSection`].
    ///
    /// * If `self` is a [`TaskItem::ParameterMetadata`], then a reference to
    ///   the inner [`ParameterMetadataSection`] is returned wrapped in
    ///   [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_parameter_metadata_section(&self) -> Option<&ParameterMetadataSection> {
        match self {
            Self::ParameterMetadata(parameter_metadata_section) => Some(parameter_metadata_section),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner
    /// [`ParameterMetadataSection`].
    ///
    /// * If `self` is a [`TaskItem::ParameterMetadata`], then the inner
    ///   [`ParameterMetadataSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_parameter_metadata_section(self) -> Option<ParameterMetadataSection> {
        match self {
            Self::ParameterMetadata(parameter_metadata_section) => Some(parameter_metadata_section),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`BoundDecl`].
    ///
    /// * If `self` is a [`TaskItem::Declaration`], then a reference to the
    ///   inner [`BoundDecl`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_declaration(&self) -> Option<&BoundDecl> {
        match self {
            Self::Declaration(declaration) => Some(declaration),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`BoundDecl`].
    ///
    /// * If `self` is a [`TaskItem::Declaration`], then the inner [`BoundDecl`]
    ///   is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_declaration(self) -> Option<BoundDecl> {
        match self {
            Self::Declaration(declaration) => Some(declaration),
            _ => None,
        }
    }

    /// Finds the first child that can be cast to an [`TaskItem`].
    ///
    /// This is meant to emulate the functionality of
    /// [`rowan::ast::support::child`] without requiring [`TaskItem`] to
    /// implement the `AstNode` trait.
    pub fn child(syntax: &SyntaxNode) -> Option<Self> {
        syntax.children().find_map(Self::cast)
    }

    /// Finds all children that can be cast to an [`TaskItem`].
    ///
    /// This is meant to emulate the functionality of
    /// [`rowan::ast::support::children`] without requiring [`TaskItem`] to
    /// implement the `AstNode` trait.
    pub fn children(syntax: &SyntaxNode) -> impl Iterator<Item = TaskItem> + use<> {
        syntax.children().filter_map(Self::cast)
    }
}

/// Represents the parent of a section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SectionParent {
    /// The parent is a task.
    Task(TaskDefinition),
    /// The parent is a workflow.
    Workflow(WorkflowDefinition),
    /// The parent is a struct.
    Struct(StructDefinition),
}

impl SectionParent {
    /// Returns whether or not a [`SyntaxKind`] is able to be cast to any of the
    /// underlying members within the [`SectionParent`].
    pub fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(
            kind,
            SyntaxKind::TaskDefinitionNode
                | SyntaxKind::WorkflowDefinitionNode
                | SyntaxKind::StructDefinitionNode
        )
    }

    /// Attempts to cast the [`SyntaxNode`] to any of the underlying members
    /// within the [`SectionParent`].
    pub fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::TaskDefinitionNode => Some(Self::Task(
                TaskDefinition::cast(syntax).expect("task definition to cast"),
            )),
            SyntaxKind::WorkflowDefinitionNode => Some(Self::Workflow(
                WorkflowDefinition::cast(syntax).expect("workflow definition to cast"),
            )),
            SyntaxKind::StructDefinitionNode => Some(Self::Struct(
                StructDefinition::cast(syntax).expect("struct definition to cast"),
            )),
            _ => None,
        }
    }

    /// Gets a reference to the underlying [`SyntaxNode`].
    pub fn syntax(&self) -> &SyntaxNode {
        match self {
            Self::Task(element) => element.syntax(),
            Self::Workflow(element) => element.syntax(),
            Self::Struct(element) => element.syntax(),
        }
    }

    /// Gets the name of the section parent.
    pub fn name(&self) -> Ident {
        match self {
            Self::Task(t) => t.name(),
            Self::Workflow(w) => w.name(),
            Self::Struct(s) => s.name(),
        }
    }

    /// Attempts to get a reference to the inner [`TaskDefinition`].
    ///
    /// * If `self` is a [`SectionParent::Task`], then a reference to the inner
    ///   [`TaskDefinition`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_task(&self) -> Option<&TaskDefinition> {
        match self {
            Self::Task(task) => Some(task),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`TaskDefinition`].
    ///
    /// * If `self` is a [`SectionParent::Task`], then the inner
    ///   [`TaskDefinition`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_task(self) -> Option<TaskDefinition> {
        match self {
            Self::Task(task) => Some(task),
            _ => None,
        }
    }

    /// Unwraps to a task definition.
    ///
    /// # Panics
    ///
    /// Panics if it is not a task definition.
    pub fn unwrap_task(self) -> TaskDefinition {
        match self {
            Self::Task(task) => task,
            _ => panic!("not a task definition"),
        }
    }

    /// Attempts to get a reference to the inner [`WorkflowDefinition`].
    ///
    /// * If `self` is a [`SectionParent::Workflow`], then a reference to the
    ///   inner [`WorkflowDefinition`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_workflow(&self) -> Option<&WorkflowDefinition> {
        match self {
            Self::Workflow(workflow) => Some(workflow),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`WorkflowDefinition`].
    ///
    /// * If `self` is a [`SectionParent::Workflow`], then the inner
    ///   [`WorkflowDefinition`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_workflow(self) -> Option<WorkflowDefinition> {
        match self {
            Self::Workflow(workflow) => Some(workflow),
            _ => None,
        }
    }

    /// Unwraps to a workflow definition.
    ///
    /// # Panics
    ///
    /// Panics if it is not a workflow definition.
    pub fn unwrap_workflow(self) -> WorkflowDefinition {
        match self {
            Self::Workflow(workflow) => workflow,
            _ => panic!("not a workflow definition"),
        }
    }

    /// Attempts to get a reference to the inner [`StructDefinition`].
    ///
    /// * If `self` is a [`SectionParent::Struct`], then a reference to the
    ///   inner [`StructDefinition`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_struct(&self) -> Option<&StructDefinition> {
        match self {
            Self::Struct(r#struct) => Some(r#struct),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`StructDefinition`].
    ///
    /// * If `self` is a [`SectionParent::Struct`], then the inner
    ///   [`StructDefinition`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_struct(self) -> Option<StructDefinition> {
        match self {
            Self::Struct(r#struct) => Some(r#struct),
            _ => None,
        }
    }

    /// Unwraps to a struct definition.
    ///
    /// # Panics
    ///
    /// Panics if it is not a struct definition.
    pub fn unwrap_struct(self) -> StructDefinition {
        match self {
            Self::Struct(def) => def,
            _ => panic!("not a struct definition"),
        }
    }

    /// Finds the first child that can be cast to an [`SectionParent`].
    ///
    /// This is meant to emulate the functionality of
    /// [`rowan::ast::support::child`] without requiring [`SectionParent`] to
    /// implement the `AstNode` trait.
    pub fn child(syntax: &SyntaxNode) -> Option<Self> {
        syntax.children().find_map(Self::cast)
    }

    /// Finds all children that can be cast to an [`SectionParent`].
    ///
    /// This is meant to emulate the functionality of
    /// [`rowan::ast::support::children`] without requiring [`SectionParent`] to
    /// implement the `AstNode` trait.
    pub fn children(syntax: &SyntaxNode) -> impl Iterator<Item = SectionParent> + use<> {
        syntax.children().filter_map(Self::cast)
    }
}

/// Represents an input section in a task or workflow definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputSection(pub(crate) SyntaxNode);

impl InputSection {
    /// Gets the declarations of the input section.
    pub fn declarations(&self) -> impl Iterator<Item = Decl> + use<> {
        Decl::children(&self.0)
    }

    /// Gets the parent of the input section.
    pub fn parent(&self) -> SectionParent {
        SectionParent::cast(self.0.parent().expect("should have a parent"))
            .expect("parent should cast")
    }
}

impl AstNode for InputSection {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::InputSectionNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::InputSectionNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents an output section in a task or workflow definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputSection(pub(crate) SyntaxNode);

impl OutputSection {
    /// Gets the declarations of the output section.
    pub fn declarations(&self) -> AstChildren<BoundDecl> {
        children(&self.0)
    }

    /// Gets the parent of the output section.
    pub fn parent(&self) -> SectionParent {
        SectionParent::cast(self.0.parent().expect("should have a parent"))
            .expect("parent should cast")
    }
}

impl AstNode for OutputSection {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::OutputSectionNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::OutputSectionNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents a command section in a task definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandSection(pub(crate) SyntaxNode);

impl CommandSection {
    /// Gets whether or not the command section is a heredoc command.
    pub fn is_heredoc(&self) -> bool {
        support::token(&self.0, SyntaxKind::OpenHeredoc).is_some()
    }

    /// Gets the parts of the command.
    pub fn parts(&self) -> impl Iterator<Item = CommandPart> + use<> {
        self.0.children_with_tokens().filter_map(CommandPart::cast)
    }

    /// Gets the command text if the command is not interpolated (i.e. has no
    /// placeholders).
    ///
    /// Returns `None` if the command is interpolated, as interpolated commands
    /// cannot be represented as a single span of text.
    pub fn text(&self) -> Option<CommandText> {
        let mut parts = self.parts();
        if let Some(CommandPart::Text(text)) = parts.next() {
            if parts.next().is_none() {
                return Some(text);
            }
        }

        None
    }

    /// Gets the parent of the command section.
    pub fn parent(&self) -> SectionParent {
        SectionParent::cast(self.0.parent().expect("should have a parent"))
            .expect("parent should cast")
    }
}

impl AstNode for CommandSection {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::CommandSectionNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::CommandSectionNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents a textual part of a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandText(pub(crate) SyntaxToken);

impl AstToken for CommandText {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::LiteralCommandText
    }

    fn cast(syntax: SyntaxToken) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::LiteralCommandText => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

/// Represents a part of a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandPart {
    /// A textual part of the command.
    Text(CommandText),
    /// A placeholder encountered in the command.
    Placeholder(Placeholder),
}

impl CommandPart {
    /// Unwraps the command part into text.
    ///
    /// # Panics
    ///
    /// Panics if the command part is not text.
    pub fn unwrap_text(self) -> CommandText {
        match self {
            Self::Text(text) => text,
            _ => panic!("not string text"),
        }
    }

    /// Unwraps the command part into a placeholder.
    ///
    /// # Panics
    ///
    /// Panics if the command part is not a placeholder.
    pub fn unwrap_placeholder(self) -> Placeholder {
        match self {
            Self::Placeholder(p) => p,
            _ => panic!("not a placeholder"),
        }
    }

    /// Casts the given syntax element to a command part.
    fn cast(syntax: SyntaxElement) -> Option<Self> {
        match syntax {
            SyntaxElement::Node(n) => Some(Self::Placeholder(Placeholder::cast(n)?)),
            SyntaxElement::Token(t) => Some(Self::Text(CommandText::cast(t)?)),
        }
    }
}

/// Represents a requirements section in a task definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequirementsSection(pub(crate) SyntaxNode);

impl RequirementsSection {
    /// Gets the items in the requirements section.
    pub fn items(&self) -> AstChildren<RequirementsItem> {
        children(&self.0)
    }

    /// Gets the parent of the requirements section.
    pub fn parent(&self) -> SectionParent {
        SectionParent::cast(self.0.parent().expect("should have a parent"))
            .expect("parent should cast")
    }

    /// Gets the `container` item as a
    /// [`Container`](requirements::item::Container) (if it exists).
    pub fn container(&self) -> Option<requirements::item::Container> {
        // NOTE: validation should ensure that, at most, one `container` item exists in
        // the `requirements` section.
        child::<requirements::item::Container>(&self.0)
    }
}

impl AstNode for RequirementsSection {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::RequirementsSectionNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::RequirementsSectionNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents an item in a requirements section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequirementsItem(SyntaxNode);

impl RequirementsItem {
    /// Gets the name of the requirements item.
    pub fn name(&self) -> Ident {
        token(&self.0).expect("expected an item name")
    }

    /// Gets the expression of the requirements item.
    pub fn expr(&self) -> Expr {
        Expr::child(&self.0).expect("expected an item expression")
    }

    /// Consumes `self` and attempts to cast the requirements item to a
    /// [`Container`](requirements::item::Container).
    pub fn into_container(self) -> Option<requirements::item::Container> {
        requirements::item::Container::try_from(self).ok()
    }
}

impl AstNode for RequirementsItem {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::RequirementsItemNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::RequirementsItemNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents a hints section in a task definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskHintsSection(pub(crate) SyntaxNode);

impl TaskHintsSection {
    /// Gets the items in the hints section.
    pub fn items(&self) -> AstChildren<TaskHintsItem> {
        children(&self.0)
    }

    /// Gets the parent of the hints section.
    pub fn parent(&self) -> TaskDefinition {
        TaskDefinition::cast(self.0.parent().expect("should have a parent"))
            .expect("parent should cast")
    }
}

impl AstNode for TaskHintsSection {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::TaskHintsSectionNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::TaskHintsSectionNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents an item in a task hints section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskHintsItem(SyntaxNode);

impl TaskHintsItem {
    /// Gets the name of the hints item.
    pub fn name(&self) -> Ident {
        token(&self.0).expect("expected an item name")
    }

    /// Gets the expression of the hints item.
    pub fn expr(&self) -> Expr {
        Expr::child(&self.0).expect("expected an item expression")
    }
}

impl AstNode for TaskHintsItem {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::TaskHintsItemNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::TaskHintsItemNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents a runtime section in a task definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeSection(pub(crate) SyntaxNode);

impl RuntimeSection {
    /// Gets the items in the runtime section.
    pub fn items(&self) -> AstChildren<RuntimeItem> {
        children(&self.0)
    }

    /// Gets the parent of the runtime section.
    pub fn parent(&self) -> SectionParent {
        SectionParent::cast(self.0.parent().expect("should have a parent"))
            .expect("parent should cast")
    }

    /// Gets the `container` item as a [`Container`](runtime::item::Container)
    /// (if it exists).
    pub fn container(&self) -> Option<runtime::item::Container> {
        // NOTE: validation should ensure that, at most, one `container`/`docker` item
        // exists in the `runtime` section.
        child::<runtime::item::Container>(&self.0)
    }
}

impl AstNode for RuntimeSection {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::RuntimeSectionNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::RuntimeSectionNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents an item in a runtime section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeItem(pub(crate) SyntaxNode);

impl RuntimeItem {
    /// Gets the name of the runtime item.
    pub fn name(&self) -> Ident {
        token(&self.0).expect("expected an item name")
    }

    /// Gets the expression of the runtime item.
    pub fn expr(&self) -> Expr {
        Expr::child(&self.0).expect("expected an item expression")
    }

    /// Consumes `self` and attempts to cast the runtime item to a
    /// [`Container`](runtime::item::Container).
    pub fn into_container(self) -> Option<runtime::item::Container> {
        runtime::item::Container::try_from(self).ok()
    }
}

impl AstNode for RuntimeItem {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::RuntimeItemNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::RuntimeItemNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents a metadata section in a task or workflow definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataSection(pub(crate) SyntaxNode);

impl MetadataSection {
    /// Gets the items of the metadata section.
    pub fn items(&self) -> AstChildren<MetadataObjectItem> {
        children(&self.0)
    }

    /// Gets the parent of the metadata section.
    pub fn parent(&self) -> SectionParent {
        SectionParent::cast(self.0.parent().expect("should have a parent"))
            .expect("parent should cast")
    }
}

impl AstNode for MetadataSection {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::MetadataSectionNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::MetadataSectionNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents a metadata object item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataObjectItem(pub(crate) SyntaxNode);

impl MetadataObjectItem {
    /// Gets the name of the item.
    pub fn name(&self) -> Ident {
        token(&self.0).expect("expected a name")
    }

    /// Gets the value of the item.
    pub fn value(&self) -> MetadataValue {
        child(&self.0).expect("expected a value")
    }
}

impl AstNode for MetadataObjectItem {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::MetadataObjectItemNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::MetadataObjectItemNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents a metadata value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MetadataValue {
    /// The value is a literal boolean.
    Boolean(LiteralBoolean),
    /// The value is a literal integer.
    Integer(LiteralInteger),
    /// The value is a literal float.
    Float(LiteralFloat),
    /// The value is a literal string.
    String(LiteralString),
    /// The value is a literal null.
    Null(LiteralNull),
    /// The value is a metadata object.
    Object(MetadataObject),
    /// The value is a metadata array.
    Array(MetadataArray),
}

impl MetadataValue {
    /// Unwraps the metadata value into a boolean.
    ///
    /// # Panics
    ///
    /// Panics if the metadata value is not a boolean.
    pub fn unwrap_boolean(self) -> LiteralBoolean {
        match self {
            Self::Boolean(b) => b,
            _ => panic!("not a boolean"),
        }
    }

    /// Unwraps the metadata value into an integer.
    ///
    /// # Panics
    ///
    /// Panics if the metadata value is not an integer.
    pub fn unwrap_integer(self) -> LiteralInteger {
        match self {
            Self::Integer(i) => i,
            _ => panic!("not an integer"),
        }
    }

    /// Unwraps the metadata value into a float.
    ///
    /// # Panics
    ///
    /// Panics if the metadata value is not a float.
    pub fn unwrap_float(self) -> LiteralFloat {
        match self {
            Self::Float(f) => f,
            _ => panic!("not a float"),
        }
    }

    /// Unwraps the metadata value into a string.
    ///
    /// # Panics
    ///
    /// Panics if the metadata value is not a string.
    pub fn unwrap_string(self) -> LiteralString {
        match self {
            Self::String(s) => s,
            _ => panic!("not a string"),
        }
    }

    /// Unwraps the metadata value into a null.
    ///
    /// # Panics
    ///
    /// Panics if the metadata value is not a null.
    pub fn unwrap_null(self) -> LiteralNull {
        match self {
            Self::Null(n) => n,
            _ => panic!("not a null"),
        }
    }

    /// Unwraps the metadata value into an object.
    ///
    /// # Panics
    ///
    /// Panics if the metadata value is not an object.
    pub fn unwrap_object(self) -> MetadataObject {
        match self {
            Self::Object(o) => o,
            _ => panic!("not an object"),
        }
    }

    /// Unwraps the metadata value into an array.
    ///
    /// # Panics
    ///
    /// Panics if the metadata value is not an array.
    pub fn unwrap_array(self) -> MetadataArray {
        match self {
            Self::Array(a) => a,
            _ => panic!("not an array"),
        }
    }
}

impl AstNode for MetadataValue {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(
            kind,
            SyntaxKind::LiteralBooleanNode
                | SyntaxKind::LiteralIntegerNode
                | SyntaxKind::LiteralFloatNode
                | SyntaxKind::LiteralStringNode
                | SyntaxKind::LiteralNullNode
                | SyntaxKind::MetadataObjectNode
                | SyntaxKind::MetadataArrayNode
        )
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::LiteralBooleanNode => Some(Self::Boolean(LiteralBoolean(syntax))),
            SyntaxKind::LiteralIntegerNode => Some(Self::Integer(LiteralInteger(syntax))),
            SyntaxKind::LiteralFloatNode => Some(Self::Float(LiteralFloat(syntax))),
            SyntaxKind::LiteralStringNode => Some(Self::String(LiteralString(syntax))),
            SyntaxKind::LiteralNullNode => Some(Self::Null(LiteralNull(syntax))),
            SyntaxKind::MetadataObjectNode => Some(Self::Object(MetadataObject(syntax))),
            SyntaxKind::MetadataArrayNode => Some(Self::Array(MetadataArray(syntax))),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        match self {
            Self::Boolean(b) => &b.0,
            Self::Integer(i) => &i.0,
            Self::Float(f) => &f.0,
            Self::String(s) => &s.0,
            Self::Null(n) => &n.0,
            Self::Object(o) => &o.0,
            Self::Array(a) => &a.0,
        }
    }
}

/// Represents a literal null.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiteralNull(SyntaxNode);

impl AstNode for LiteralNull {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::LiteralNullNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::LiteralNullNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents a metadata object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataObject(pub(crate) SyntaxNode);

impl MetadataObject {
    /// Gets the items of the metadata object.
    pub fn items(&self) -> AstChildren<MetadataObjectItem> {
        children(&self.0)
    }
}

impl AstNode for MetadataObject {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::MetadataObjectNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::MetadataObjectNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents a metadata array.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataArray(pub(crate) SyntaxNode);

impl MetadataArray {
    /// Gets the elements of the metadata array.
    pub fn elements(&self) -> AstChildren<MetadataValue> {
        children(&self.0)
    }
}

impl AstNode for MetadataArray {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::MetadataArrayNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::MetadataArrayNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents a parameter metadata section in a task or workflow definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParameterMetadataSection(pub(crate) SyntaxNode);

impl ParameterMetadataSection {
    /// Gets the items of the parameter metadata section.
    pub fn items(&self) -> AstChildren<MetadataObjectItem> {
        children(&self.0)
    }

    /// Gets the parent of the parameter metadata section.
    pub fn parent(&self) -> SectionParent {
        SectionParent::cast(self.0.parent().expect("should have a parent"))
            .expect("parent should cast")
    }
}

impl AstNode for ParameterMetadataSection {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::ParameterMetadataSectionNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::ParameterMetadataSectionNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::Document;
    use crate::SupportedVersion;
    use crate::VisitReason;
    use crate::Visitor;
    use crate::v1::UnboundDecl;

    #[test]
    fn tasks() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.2

task test {
    input {
        String name
    }

    output {
        String output = stdout()
    }

    command <<<
        printf "hello, ~{name}!
    >>>

    requirements {
        container: "baz/qux"
    }

    hints {
        foo: "bar"
    }

    runtime {
        container: "foo/bar"
    }

    meta {
        description: "a test"
        foo: null
    }

    parameter_meta {
        name: {
            help: "a name to greet"
        }
    }

    String x = "private"
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().as_str(), "test");

        // Task input
        let input = tasks[0].input().expect("should have an input section");
        assert_eq!(input.parent().unwrap_task().name().as_str(), "test");
        let decls: Vec<_> = input.declarations().collect();
        assert_eq!(decls.len(), 1);
        assert_eq!(
            decls[0].clone().unwrap_unbound_decl().ty().to_string(),
            "String"
        );
        assert_eq!(
            decls[0].clone().unwrap_unbound_decl().name().as_str(),
            "name"
        );

        // Task output
        let output = tasks[0].output().expect("should have an output section");
        assert_eq!(output.parent().unwrap_task().name().as_str(), "test");
        let decls: Vec<_> = output.declarations().collect();
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].ty().to_string(), "String");
        assert_eq!(decls[0].name().as_str(), "output");
        assert_eq!(decls[0].expr().unwrap_call().target().as_str(), "stdout");

        // Task command
        let command = tasks[0].command().expect("should have a command section");
        assert_eq!(command.parent().name().as_str(), "test");
        assert!(command.is_heredoc());
        let parts: Vec<_> = command.parts().collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(
            parts[0].clone().unwrap_text().as_str(),
            "\n        printf \"hello, "
        );
        assert_eq!(
            parts[1]
                .clone()
                .unwrap_placeholder()
                .expr()
                .unwrap_name_ref()
                .name()
                .as_str(),
            "name"
        );
        assert_eq!(parts[2].clone().unwrap_text().as_str(), "!\n    ");

        // Task requirements
        let requirements = tasks[0]
            .requirements()
            .expect("should have a requirements section");
        assert_eq!(requirements.parent().name().as_str(), "test");
        let items: Vec<_> = requirements.items().collect();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name().as_str(), "container");
        assert_eq!(
            items[0]
                .expr()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .as_str(),
            "baz/qux"
        );

        // Task hints
        let hints = tasks[0].hints().expect("should have a hints section");
        assert_eq!(hints.parent().name().as_str(), "test");
        let items: Vec<_> = hints.items().collect();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name().as_str(), "foo");
        assert_eq!(
            items[0]
                .expr()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .as_str(),
            "bar"
        );

        // Task runtimes
        let runtime = tasks[0].runtime().expect("should have a runtime section");
        assert_eq!(runtime.parent().name().as_str(), "test");
        let items: Vec<_> = runtime.items().collect();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name().as_str(), "container");
        assert_eq!(
            items[0]
                .expr()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .as_str(),
            "foo/bar"
        );

        // Task metadata
        let metadata = tasks[0].metadata().expect("should have a metadata section");
        assert_eq!(metadata.parent().unwrap_task().name().as_str(), "test");
        let items: Vec<_> = metadata.items().collect();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name().as_str(), "description");
        assert_eq!(
            items[0].value().unwrap_string().text().unwrap().as_str(),
            "a test"
        );

        // Second metadata
        assert_eq!(items[1].name().as_str(), "foo");
        items[1].value().unwrap_null();

        // Task parameter metadata
        let param_meta = tasks[0]
            .parameter_metadata()
            .expect("should have a parameter metadata section");
        assert_eq!(param_meta.parent().unwrap_task().name().as_str(), "test");
        let items: Vec<_> = param_meta.items().collect();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name().as_str(), "name");
        let items: Vec<_> = items[0].value().unwrap_object().items().collect();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name().as_str(), "help");
        assert_eq!(
            items[0].value().unwrap_string().text().unwrap().as_str(),
            "a name to greet"
        );

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 1);

        // First task declaration
        assert_eq!(decls[0].ty().to_string(), "String");
        assert_eq!(decls[0].name().as_str(), "x");
        assert_eq!(
            decls[0]
                .expr()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .as_str(),
            "private"
        );

        // Use a visitor to count the number of task sections
        #[derive(Default)]
        struct MyVisitor {
            tasks: usize,
            inputs: usize,
            outputs: usize,
            commands: usize,
            requirements: usize,
            hints: usize,
            runtimes: usize,
            metadata: usize,
            param_metadata: usize,
            unbound_decls: usize,
            bound_decls: usize,
        }

        impl Visitor for MyVisitor {
            type State = ();

            fn document(
                &mut self,
                _: &mut Self::State,
                _: VisitReason,
                _: &Document,
                _: SupportedVersion,
            ) {
            }

            fn task_definition(
                &mut self,
                _: &mut Self::State,
                reason: VisitReason,
                _: &TaskDefinition,
            ) {
                if reason == VisitReason::Enter {
                    self.tasks += 1;
                }
            }

            fn input_section(
                &mut self,
                _: &mut Self::State,
                reason: VisitReason,
                _: &InputSection,
            ) {
                if reason == VisitReason::Enter {
                    self.inputs += 1;
                }
            }

            fn output_section(
                &mut self,
                _: &mut Self::State,
                reason: VisitReason,
                _: &OutputSection,
            ) {
                if reason == VisitReason::Enter {
                    self.outputs += 1;
                }
            }

            fn command_section(
                &mut self,
                _: &mut Self::State,
                reason: VisitReason,
                _: &CommandSection,
            ) {
                if reason == VisitReason::Enter {
                    self.commands += 1;
                }
            }

            fn requirements_section(
                &mut self,
                _: &mut Self::State,
                reason: VisitReason,
                _: &RequirementsSection,
            ) {
                if reason == VisitReason::Enter {
                    self.requirements += 1;
                }
            }

            fn task_hints_section(
                &mut self,
                _: &mut Self::State,
                reason: VisitReason,
                _: &TaskHintsSection,
            ) {
                if reason == VisitReason::Enter {
                    self.hints += 1;
                }
            }

            fn runtime_section(
                &mut self,
                _: &mut Self::State,
                reason: VisitReason,
                _: &RuntimeSection,
            ) {
                if reason == VisitReason::Enter {
                    self.runtimes += 1;
                }
            }

            fn metadata_section(
                &mut self,
                _: &mut Self::State,
                reason: VisitReason,
                _: &MetadataSection,
            ) {
                if reason == VisitReason::Enter {
                    self.metadata += 1;
                }
            }

            fn parameter_metadata_section(
                &mut self,
                _: &mut Self::State,
                reason: VisitReason,
                _: &ParameterMetadataSection,
            ) {
                if reason == VisitReason::Enter {
                    self.param_metadata += 1;
                }
            }

            fn bound_decl(&mut self, _: &mut Self::State, reason: VisitReason, _: &BoundDecl) {
                if reason == VisitReason::Enter {
                    self.bound_decls += 1;
                }
            }

            fn unbound_decl(&mut self, _: &mut Self::State, reason: VisitReason, _: &UnboundDecl) {
                if reason == VisitReason::Enter {
                    self.unbound_decls += 1;
                }
            }
        }

        let mut visitor = MyVisitor::default();
        document.visit(&mut (), &mut visitor);
        assert_eq!(visitor.tasks, 1);
        assert_eq!(visitor.inputs, 1);
        assert_eq!(visitor.outputs, 1);
        assert_eq!(visitor.commands, 1);
        assert_eq!(visitor.requirements, 1);
        assert_eq!(visitor.hints, 1);
        assert_eq!(visitor.runtimes, 1);
        assert_eq!(visitor.metadata, 1);
        assert_eq!(visitor.param_metadata, 1);
        assert_eq!(visitor.unbound_decls, 1);
        assert_eq!(visitor.bound_decls, 2);
    }
}
