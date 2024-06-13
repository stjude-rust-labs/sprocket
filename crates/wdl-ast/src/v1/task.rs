//! V1 AST representation for task definitions.

use super::BoundDecl;
use super::Decl;
use super::Expr;
use super::LiteralBoolean;
use super::LiteralFloat;
use super::LiteralInteger;
use super::LiteralString;
use super::Placeholder;
use super::WorkflowDefinition;
use crate::support;
use crate::support::child;
use crate::support::children;
use crate::token;
use crate::AstChildren;
use crate::AstNode;
use crate::AstToken;
use crate::Ident;
use crate::SyntaxElement;
use crate::SyntaxKind;
use crate::SyntaxNode;
use crate::SyntaxToken;
use crate::WorkflowDescriptionLanguage;

/// Represents a task definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskDefinition(pub(super) SyntaxNode);

impl TaskDefinition {
    /// Gets the name of the task.
    pub fn name(&self) -> Ident {
        token(&self.0).expect("task should have a name")
    }

    /// Gets the items of the task.
    pub fn items(&self) -> AstChildren<TaskItem> {
        children(&self.0)
    }

    /// Gets the input sections of the task.
    pub fn inputs(&self) -> AstChildren<InputSection> {
        children(&self.0)
    }

    /// Gets the output sections of the task.
    pub fn outputs(&self) -> AstChildren<OutputSection> {
        children(&self.0)
    }

    /// Gets the command sections of the task.
    pub fn commands(&self) -> AstChildren<CommandSection> {
        children(&self.0)
    }

    /// Gets the runtime sections of the task.
    pub fn runtimes(&self) -> AstChildren<RuntimeSection> {
        children(&self.0)
    }

    /// Gets the metadata sections of the task.
    pub fn metadata(&self) -> AstChildren<MetadataSection> {
        children(&self.0)
    }

    /// Gets the parameter sections of the task.
    pub fn parameter_metadata(&self) -> AstChildren<ParameterMetadataSection> {
        children(&self.0)
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
    /// The item is a runtime section.
    Runtime(RuntimeSection),
    /// The item is a metadata section.
    Metadata(MetadataSection),
    /// The item is a parameter meta section.
    ParameterMetadata(ParameterMetadataSection),
    /// The item is a private bound declaration.
    Declaration(BoundDecl),
}

impl AstNode for TaskItem {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(
            kind,
            SyntaxKind::InputSectionNode
                | SyntaxKind::OutputSectionNode
                | SyntaxKind::CommandSectionNode
                | SyntaxKind::RuntimeSectionNode
                | SyntaxKind::MetadataSectionNode
                | SyntaxKind::ParameterMetadataSectionNode
                | SyntaxKind::BoundDeclNode
        )
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::InputSectionNode => Some(Self::Input(InputSection(syntax))),
            SyntaxKind::OutputSectionNode => Some(Self::Output(OutputSection(syntax))),
            SyntaxKind::CommandSectionNode => Some(Self::Command(CommandSection(syntax))),
            SyntaxKind::RuntimeSectionNode => Some(Self::Runtime(RuntimeSection(syntax))),
            SyntaxKind::MetadataSectionNode => Some(Self::Metadata(MetadataSection(syntax))),
            SyntaxKind::ParameterMetaKeyword => {
                Some(Self::ParameterMetadata(ParameterMetadataSection(syntax)))
            }
            SyntaxKind::BoundDeclNode => Some(Self::Declaration(BoundDecl(syntax))),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        match self {
            Self::Input(i) => &i.0,
            Self::Output(o) => &o.0,
            Self::Command(c) => &c.0,
            Self::Runtime(r) => &r.0,
            Self::Metadata(m) => &m.0,
            Self::ParameterMetadata(m) => &m.0,
            Self::Declaration(d) => &d.0,
        }
    }
}

/// Represents either a task or a workflow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskOrWorkflow {
    /// The item is a task.
    Task(TaskDefinition),
    /// The item is a workflow.
    Workflow(WorkflowDefinition),
}

impl TaskOrWorkflow {
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
}

impl AstNode for TaskOrWorkflow {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(
            kind,
            SyntaxKind::TaskDefinitionNode | SyntaxKind::WorkflowDefinitionNode
        )
    }

    fn cast(node: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match node.kind() {
            SyntaxKind::TaskDefinitionNode => Some(Self::Task(TaskDefinition(node))),
            SyntaxKind::WorkflowDefinitionNode => Some(Self::Workflow(WorkflowDefinition(node))),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        match self {
            Self::Task(t) => &t.0,
            Self::Workflow(w) => &w.0,
        }
    }
}

/// Represents an input section in a task or workflow definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputSection(pub(super) SyntaxNode);

impl InputSection {
    /// Gets the declarations of the input section.
    pub fn declarations(&self) -> AstChildren<Decl> {
        children(&self.0)
    }

    /// Gets the parent of the input section.
    pub fn parent(&self) -> TaskOrWorkflow {
        TaskOrWorkflow::cast(self.0.parent().expect("should have a parent"))
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
pub struct OutputSection(pub(super) SyntaxNode);

impl OutputSection {
    /// Gets the declarations of the output section.
    pub fn declarations(&self) -> AstChildren<BoundDecl> {
        children(&self.0)
    }

    /// Gets the parent of the output section.
    pub fn parent(&self) -> TaskOrWorkflow {
        TaskOrWorkflow::cast(self.0.parent().expect("should have a parent"))
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
pub struct CommandSection(pub(super) SyntaxNode);

impl CommandSection {
    /// Gets whether or not the command section is a heredoc command.
    pub fn is_heredoc(&self) -> bool {
        support::token(&self.0, SyntaxKind::OpenHeredoc).is_some()
    }

    /// Gets the parts of the command.
    pub fn parts(&self) -> impl Iterator<Item = CommandPart> {
        self.0.children_with_tokens().filter_map(CommandPart::cast)
    }

    /// Gets the command text if the command is not interpolated (i.e.
    /// has no placeholders).
    ///
    /// Returns `None` if the command is interpolated, as
    /// interpolated commands cannot be represented as a single
    /// span of text.
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
    pub fn parent(&self) -> TaskDefinition {
        TaskDefinition::cast(self.0.parent().expect("should have a parent"))
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
pub struct CommandText(pub(super) SyntaxToken);

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

/// Represents a runtime section in a task definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeSection(pub(super) SyntaxNode);

impl RuntimeSection {
    /// Gets the items in the runtime section.
    pub fn items(&self) -> AstChildren<RuntimeItem> {
        children(&self.0)
    }

    /// Gets the parent of the runtime section.
    pub fn parent(&self) -> TaskDefinition {
        TaskDefinition::cast(self.0.parent().expect("should have a parent"))
            .expect("parent should cast")
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
pub struct RuntimeItem(pub(super) SyntaxNode);

impl RuntimeItem {
    /// Gets the name of the runtime item.
    pub fn name(&self) -> Ident {
        token(&self.0).expect("expected an item name")
    }

    /// Gets the expression of the runtime item.
    pub fn expr(&self) -> Expr {
        child(&self.0).expect("expected an item expression")
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
pub struct MetadataSection(pub(super) SyntaxNode);

impl MetadataSection {
    /// Gets the items of the metadata section.
    pub fn items(&self) -> AstChildren<MetadataObjectItem> {
        children(&self.0)
    }

    /// Gets the parent of the metadata section.
    pub fn parent(&self) -> TaskOrWorkflow {
        TaskOrWorkflow::cast(self.0.parent().expect("should have a parent"))
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
pub struct MetadataObjectItem(pub(super) SyntaxNode);

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
pub struct LiteralNull(pub(super) SyntaxNode);

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
pub struct MetadataObject(pub(super) SyntaxNode);

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
pub struct MetadataArray(pub(super) SyntaxNode);

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
pub struct ParameterMetadataSection(pub(super) SyntaxNode);

impl ParameterMetadataSection {
    /// Gets the items of the parameter metadata section.
    pub fn items(&self) -> AstChildren<MetadataObjectItem> {
        children(&self.0)
    }

    /// Gets the parent of the parameter metadata section.
    pub fn parent(&self) -> TaskOrWorkflow {
        TaskOrWorkflow::cast(self.0.parent().expect("should have a parent"))
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
    use crate::v1::UnboundDecl;
    use crate::v1::Visitor;
    use crate::Document;
    use crate::VisitReason;

    #[test]
    fn tasks() {
        let parse = Document::parse(
            r#"
version 1.1

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

        let document = parse.into_result().expect("there should be no errors");
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().as_str(), "test");

        // Task inputs
        let inputs: Vec<_> = tasks[0].inputs().collect();
        assert_eq!(inputs.len(), 1);

        // First task input
        assert_eq!(inputs[0].parent().unwrap_task().name().as_str(), "test");
        let decls: Vec<_> = inputs[0].declarations().collect();
        assert_eq!(decls.len(), 1);
        assert_eq!(
            decls[0].clone().unwrap_unbound_decl().ty().to_string(),
            "String"
        );
        assert_eq!(
            decls[0].clone().unwrap_unbound_decl().name().as_str(),
            "name"
        );

        // Task outputs
        let outputs: Vec<_> = tasks[0].outputs().collect();
        assert_eq!(outputs.len(), 1);

        // First task output
        assert_eq!(outputs[0].parent().unwrap_task().name().as_str(), "test");
        let decls: Vec<_> = outputs[0].declarations().collect();
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].ty().to_string(), "String");
        assert_eq!(decls[0].name().as_str(), "output");
        assert_eq!(
            decls[0]
                .expr()
                .unwrap_call()
                .target()
                .unwrap_name_ref()
                .name()
                .as_str(),
            "stdout"
        );

        // Task commands
        let commands: Vec<_> = tasks[0].commands().collect();
        assert_eq!(commands.len(), 1);

        // First task command
        assert_eq!(commands[0].parent().name().as_str(), "test");
        assert!(commands[0].is_heredoc());
        let parts: Vec<_> = commands[0].parts().collect();
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

        // Task runtimes
        let runtimes: Vec<_> = tasks[0].runtimes().collect();
        assert_eq!(runtimes.len(), 1);

        // First task runtime
        assert_eq!(runtimes[0].parent().name().as_str(), "test");
        let items: Vec<_> = runtimes[0].items().collect();
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
        let metadata: Vec<_> = tasks[0].metadata().collect();
        assert_eq!(metadata.len(), 1);

        // First metadata
        assert_eq!(metadata[0].parent().unwrap_task().name().as_str(), "test");
        let items: Vec<_> = metadata[0].items().collect();
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
        let param_meta: Vec<_> = tasks[0].parameter_metadata().collect();
        assert_eq!(param_meta.len(), 1);

        // First task parameter metadata
        assert_eq!(param_meta[0].parent().unwrap_task().name().as_str(), "test");
        let items: Vec<_> = param_meta[0].items().collect();
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
            runtimes: usize,
            metadata: usize,
            param_metadata: usize,
            unbound_decls: usize,
            bound_decls: usize,
        }

        impl Visitor for MyVisitor {
            type State = ();

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
        ast.visit(&mut (), &mut visitor);
        assert_eq!(visitor.tasks, 1);
        assert_eq!(visitor.inputs, 1);
        assert_eq!(visitor.outputs, 1);
        assert_eq!(visitor.commands, 1);
        assert_eq!(visitor.runtimes, 1);
        assert_eq!(visitor.metadata, 1);
        assert_eq!(visitor.param_metadata, 1);
        assert_eq!(visitor.unbound_decls, 1);
        assert_eq!(visitor.bound_decls, 2);
    }
}
