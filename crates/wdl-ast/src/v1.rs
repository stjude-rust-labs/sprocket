//! AST representation for a 1.x WDL document.

use crate::AstChildren;
use crate::AstNode;
use crate::SyntaxKind;
use crate::SyntaxNode;
use crate::WorkflowDescriptionLanguage;
use crate::support::children;

mod decls;
mod expr;
mod import;
mod r#struct;
mod task;
mod tokens;
mod workflow;

pub use decls::*;
pub use expr::*;
pub use import::*;
pub use r#struct::*;
pub use task::*;
pub use tokens::*;
pub use workflow::*;

/// Represents a WDL V1 Abstract Syntax Tree (AST).
///
/// The AST is a facade over a [SyntaxTree][1].
///
/// A syntax tree is comprised of nodes that have either
/// other nodes or tokens as children.
///
/// A token is a span of text from the WDL source text and
/// is terminal in the tree.
///
/// Elements of an AST are trivially cloned.
///
/// [1]: crate::SyntaxTree
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ast(SyntaxNode);

impl Ast {
    /// Gets all of the document items in the AST.
    pub fn items(&self) -> impl Iterator<Item = DocumentItem> {
        DocumentItem::children(&self.0)
    }

    /// Gets the import statements in the AST.
    pub fn imports(&self) -> AstChildren<ImportStatement> {
        children(&self.0)
    }

    /// Gets the struct definitions in the AST.
    pub fn structs(&self) -> AstChildren<StructDefinition> {
        children(&self.0)
    }

    /// Gets the task definitions in the AST.
    pub fn tasks(&self) -> AstChildren<TaskDefinition> {
        children(&self.0)
    }

    /// Gets the workflow definitions in the AST.
    pub fn workflows(&self) -> AstChildren<WorkflowDefinition> {
        children(&self.0)
    }
}

impl AstNode for Ast {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::RootNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::RootNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents a document item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentItem {
    /// The item is an import statement.
    Import(ImportStatement),
    /// The item is a struct definition.
    Struct(StructDefinition),
    /// The item is a task definition.
    Task(TaskDefinition),
    /// The item is a workflow definition.
    Workflow(WorkflowDefinition),
}

impl DocumentItem {
    /// Returns whether or not a [`SyntaxKind`] is able to be cast to any of the
    /// underlying members within the [`DocumentItem`].
    pub fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(
            kind,
            SyntaxKind::ImportStatementNode
                | SyntaxKind::StructDefinitionNode
                | SyntaxKind::TaskDefinitionNode
                | SyntaxKind::WorkflowDefinitionNode
        )
    }

    /// Attempts to cast the [`SyntaxNode`] to any of the underlying members
    /// within the [`DocumentItem`].
    pub fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::ImportStatementNode => Some(Self::Import(
                ImportStatement::cast(syntax).expect("import statement to cast"),
            )),
            SyntaxKind::StructDefinitionNode => Some(Self::Struct(
                StructDefinition::cast(syntax).expect("struct definition to cast"),
            )),
            SyntaxKind::TaskDefinitionNode => Some(Self::Task(
                TaskDefinition::cast(syntax).expect("task definition to cast"),
            )),
            SyntaxKind::WorkflowDefinitionNode => Some(Self::Workflow(
                WorkflowDefinition::cast(syntax).expect("workflow definition to cast"),
            )),
            _ => None,
        }
    }

    /// Gets a reference to the underlying [`SyntaxNode`].
    pub fn syntax(&self) -> &SyntaxNode {
        match self {
            Self::Import(element) => element.syntax(),
            Self::Struct(element) => element.syntax(),
            Self::Task(element) => element.syntax(),
            Self::Workflow(element) => element.syntax(),
        }
    }

    /// Attempts to get a reference to the inner [`ImportStatement`].
    ///
    /// * If `self` is a [`DocumentItem::Import`], then a reference to the inner
    ///   [`ImportStatement`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_import_statement(&self) -> Option<&ImportStatement> {
        match self {
            DocumentItem::Import(import) => Some(import),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`ImportStatement`].
    ///
    /// * If `self` is a [`DocumentItem::Import`], then the inner
    ///   [`ImportStatement`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_import_statement(self) -> Option<ImportStatement> {
        match self {
            DocumentItem::Import(import) => Some(import),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`StructDefinition`].
    ///
    /// * If `self` is a [`DocumentItem::Struct`], then a reference to the inner
    ///   [`StructDefinition`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_struct_definition(&self) -> Option<&StructDefinition> {
        match self {
            DocumentItem::Struct(r#struct) => Some(r#struct),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`StructDefinition`].
    ///
    /// * If `self` is a [`DocumentItem::Struct`], then the inner
    ///   [`StructDefinition`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_struct_definition(self) -> Option<StructDefinition> {
        match self {
            DocumentItem::Struct(r#struct) => Some(r#struct),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`TaskDefinition`].
    ///
    /// * If `self` is a [`DocumentItem::Task`], then a reference to the inner
    ///   [`TaskDefinition`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_task_definition(&self) -> Option<&TaskDefinition> {
        match self {
            DocumentItem::Task(task) => Some(task),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`TaskDefinition`].
    ///
    /// * If `self` is a [`DocumentItem::Task`], then the inner
    ///   [`TaskDefinition`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_task_definition(self) -> Option<TaskDefinition> {
        match self {
            DocumentItem::Task(task) => Some(task),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`WorkflowDefinition`].
    ///
    /// * If `self` is a [`DocumentItem::Workflow`], then a reference to the
    ///   inner [`WorkflowDefinition`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_workflow_definition(&self) -> Option<&WorkflowDefinition> {
        match self {
            DocumentItem::Workflow(workflow) => Some(workflow),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`WorkflowDefinition`].
    ///
    /// * If `self` is a [`DocumentItem::Workflow`], then the inner
    ///   [`WorkflowDefinition`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_workflow_definition(self) -> Option<WorkflowDefinition> {
        match self {
            DocumentItem::Workflow(workflow) => Some(workflow),
            _ => None,
        }
    }

    /// Finds the first child that can be cast to an [`DocumentItem`].
    ///
    /// This is meant to emulate the functionality of
    /// [`rowan::ast::support::child`] without requiring [`DocumentItem`] to
    /// implement the `AstNode` trait.
    pub fn child(syntax: &SyntaxNode) -> Option<Self> {
        syntax.children().find_map(Self::cast)
    }

    /// Finds all children that can be cast to an [`DocumentItem`].
    ///
    /// This is meant to emulate the functionality of
    /// [`rowan::ast::support::children`] without requiring [`DocumentItem`] to
    /// implement the `AstNode` trait.
    pub fn children(syntax: &SyntaxNode) -> impl Iterator<Item = DocumentItem> {
        syntax.children().filter_map(Self::cast)
    }
}
