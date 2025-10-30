//! AST representation for a 1.x WDL document.

use crate::AstNode;
use crate::SyntaxKind;
use crate::SyntaxNode;
use crate::TreeNode;

mod decls;
mod display;
mod r#enum;
mod expr;
mod import;
mod r#struct;
mod task;
mod tokens;
mod workflow;

pub use decls::*;
pub use r#enum::*;
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
pub struct Ast<N: TreeNode = SyntaxNode>(pub(crate) N);

impl<N: TreeNode> Ast<N> {
    /// Gets all of the document items in the AST.
    pub fn items(&self) -> impl Iterator<Item = DocumentItem<N>> + use<'_, N> {
        DocumentItem::children(&self.0)
    }

    /// Gets the import statements in the AST.
    pub fn imports(&self) -> impl Iterator<Item = ImportStatement<N>> + use<'_, N> {
        self.children()
    }

    /// Gets the struct definitions in the AST.
    pub fn structs(&self) -> impl Iterator<Item = StructDefinition<N>> + use<'_, N> {
        self.children()
    }

    /// Gets the enum definitions in the AST.
    pub fn enums(&self) -> impl Iterator<Item = EnumDefinition<N>> + use<'_, N> {
        self.children()
    }

    /// Gets the task definitions in the AST.
    pub fn tasks(&self) -> impl Iterator<Item = TaskDefinition<N>> + use<'_, N> {
        self.children()
    }

    /// Gets the workflow definitions in the AST.
    pub fn workflows(&self) -> impl Iterator<Item = WorkflowDefinition<N>> + use<'_, N> {
        self.children()
    }
}

impl<N: TreeNode> AstNode<N> for Ast<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::RootNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::RootNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a document item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentItem<N: TreeNode = SyntaxNode> {
    /// The item is an import statement.
    Import(ImportStatement<N>),
    /// The item is a struct definition.
    Struct(StructDefinition<N>),
    /// The item is an enum definition.
    Enum(EnumDefinition<N>),
    /// The item is a task definition.
    Task(TaskDefinition<N>),
    /// The item is a workflow definition.
    Workflow(WorkflowDefinition<N>),
}

impl<N: TreeNode> DocumentItem<N> {
    // Returns whether or not the given syntax kind can be cast to
    /// [`DocumentItem`].
    pub fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(
            kind,
            SyntaxKind::ImportStatementNode
                | SyntaxKind::StructDefinitionNode
                | SyntaxKind::EnumDefinitionNode
                | SyntaxKind::TaskDefinitionNode
                | SyntaxKind::WorkflowDefinitionNode
        )
    }

    /// Casts the given node to [`DocumentItem`].
    ///
    /// Returns `None` if the node cannot be cast.
    pub fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::ImportStatementNode => Some(Self::Import(
                ImportStatement::cast(inner).expect("import statement to cast"),
            )),
            SyntaxKind::StructDefinitionNode => Some(Self::Struct(
                StructDefinition::cast(inner).expect("struct definition to cast"),
            )),
            SyntaxKind::EnumDefinitionNode => Some(Self::Enum(
                EnumDefinition::cast(inner).expect("enum definition to cast"),
            )),
            SyntaxKind::TaskDefinitionNode => Some(Self::Task(
                TaskDefinition::cast(inner).expect("task definition to cast"),
            )),
            SyntaxKind::WorkflowDefinitionNode => Some(Self::Workflow(
                WorkflowDefinition::cast(inner).expect("workflow definition to cast"),
            )),
            _ => None,
        }
    }

    /// Gets a reference to the inner node.
    pub fn inner(&self) -> &N {
        match self {
            Self::Import(e) => e.inner(),
            Self::Struct(e) => e.inner(),
            Self::Enum(e) => e.inner(),
            Self::Task(e) => e.inner(),
            Self::Workflow(e) => e.inner(),
        }
    }

    /// Attempts to get a reference to the inner [`ImportStatement`].
    ///
    /// * If `self` is a [`DocumentItem::Import`], then a reference to the inner
    ///   [`ImportStatement`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_import_statement(&self) -> Option<&ImportStatement<N>> {
        match self {
            Self::Import(i) => Some(i),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`ImportStatement`].
    ///
    /// * If `self` is a [`DocumentItem::Import`], then the inner
    ///   [`ImportStatement`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_import_statement(self) -> Option<ImportStatement<N>> {
        match self {
            Self::Import(i) => Some(i),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`StructDefinition`].
    ///
    /// * If `self` is a [`DocumentItem::Struct`], then a reference to the inner
    ///   [`StructDefinition`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_struct_definition(&self) -> Option<&StructDefinition<N>> {
        match self {
            Self::Struct(i) => Some(i),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`StructDefinition`].
    ///
    /// * If `self` is a [`DocumentItem::Struct`], then the inner
    ///   [`StructDefinition`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_struct_definition(self) -> Option<StructDefinition<N>> {
        match self {
            Self::Struct(i) => Some(i),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`TaskDefinition`].
    ///
    /// * If `self` is a [`DocumentItem::Task`], then a reference to the inner
    ///   [`TaskDefinition`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_task_definition(&self) -> Option<&TaskDefinition<N>> {
        match self {
            Self::Task(i) => Some(i),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`TaskDefinition`].
    ///
    /// * If `self` is a [`DocumentItem::Task`], then the inner
    ///   [`TaskDefinition`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_task_definition(self) -> Option<TaskDefinition<N>> {
        match self {
            Self::Task(i) => Some(i),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`WorkflowDefinition`].
    ///
    /// * If `self` is a [`DocumentItem::Workflow`], then a reference to the
    ///   inner [`WorkflowDefinition`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_workflow_definition(&self) -> Option<&WorkflowDefinition<N>> {
        match self {
            Self::Workflow(i) => Some(i),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`WorkflowDefinition`].
    ///
    /// * If `self` is a [`DocumentItem::Workflow`], then the inner
    ///   [`WorkflowDefinition`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_workflow_definition(self) -> Option<WorkflowDefinition<N>> {
        match self {
            Self::Workflow(i) => Some(i),
            _ => None,
        }
    }

    /// Finds the first child that can be cast to a [`DocumentItem`].
    pub fn child(node: &N) -> Option<Self> {
        node.children().find_map(Self::cast)
    }

    /// Finds all children that can be cast to a [`DocumentItem`].
    pub fn children(node: &N) -> impl Iterator<Item = Self> + use<'_, N> {
        node.children().filter_map(Self::cast)
    }
}
