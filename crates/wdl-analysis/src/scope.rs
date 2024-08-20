//! Implementation of scopes for WDL documents.

use std::fmt;
use std::sync::Arc;

use indexmap::IndexMap;
use petgraph::graph::NodeIndex;
use rowan::GreenNode;
use url::Url;
use wdl_ast::support::token;
use wdl_ast::v1::StructDefinition;
use wdl_ast::Ast;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_ast::ToSpan;

use crate::graph::DocumentGraph;
use crate::graph::ParseState;
use crate::Type;
use crate::Types;

mod v1;

/// Represents the context of a name for diagnostic reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NameContext {
    /// The name is a workflow name.
    Workflow(Span),
    /// The name is a task name.
    Task(Span),
    /// The name is a struct name.
    Struct(Span),
    /// The name is a struct member name.
    StructMember(Span),
    /// A name local to a scope.
    Scoped(ScopedNameContext),
}

impl NameContext {
    /// Gets the span of the name.
    fn span(&self) -> Span {
        match self {
            Self::Workflow(s) => *s,
            Self::Task(s) => *s,
            Self::Struct(s) => *s,
            Self::StructMember(s) => *s,
            Self::Scoped(n) => n.span(),
        }
    }
}

impl fmt::Display for NameContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Workflow(_) => write!(f, "workflow"),
            Self::Task(_) => write!(f, "task"),
            Self::Struct(_) => write!(f, "struct"),
            Self::StructMember(_) => write!(f, "struct member"),
            Self::Scoped(n) => n.fmt(f),
        }
    }
}

/// Represents the context of a name in a workflow or task scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopedNameContext {
    /// The name was introduced by an task or workflow input.
    Input(Span),
    /// The name was introduced by an task or workflow output.
    Output(Span),
    /// The name was introduced by a private declaration.
    Decl(Span),
    /// The name was introduced by a workflow call statement.
    Call(Span),
    /// The name was introduced by a variable in workflow scatter statement.
    ScatterVariable(Span),
}

impl ScopedNameContext {
    /// Gets the span of the name.
    pub fn span(&self) -> Span {
        match self {
            Self::Input(s) => *s,
            Self::Output(s) => *s,
            Self::Decl(s) => *s,
            Self::Call(s) => *s,
            Self::ScatterVariable(s) => *s,
        }
    }
}

impl fmt::Display for ScopedNameContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input(_) => write!(f, "input"),
            Self::Output(_) => write!(f, "output"),
            Self::Decl(_) => write!(f, "declaration"),
            Self::Call(_) => write!(f, "call"),
            Self::ScatterVariable(_) => write!(f, "scatter variable"),
        }
    }
}

impl From<ScopedNameContext> for NameContext {
    fn from(context: ScopedNameContext) -> Self {
        Self::Scoped(context)
    }
}

/// Represents a name in a task or workflow scope.
#[derive(Debug, Clone)]
pub struct ScopedName {
    /// The context of the name.
    context: ScopedNameContext,
    /// The CST node that introduced the name.
    node: GreenNode,
    /// The type of the name.
    ///
    /// Initially this is `None` until a type check occurs.
    ty: Option<Type>,
    /// Whether or not the name was implicitly introduced.
    ///
    /// This is true for names introduced in outer scopes from workflow scatter
    /// and conditional statements.
    implicit: bool,
}

impl ScopedName {
    /// Constructs a new scoped name.
    pub(crate) fn new(context: ScopedNameContext, node: GreenNode, implicit: bool) -> Self {
        Self {
            context,
            node,
            ty: None,
            implicit,
        }
    }

    /// Gets the context of the scoped name.
    pub fn context(&self) -> ScopedNameContext {
        self.context
    }

    /// Gets the node of the scoped name.
    ///
    /// This may be a bound declaration, an unbound declaration, a workflow call
    /// statement, or a workflow scatter statement.
    pub fn node(&self) -> &GreenNode {
        &self.node
    }

    /// Gets the type of the name.
    ///
    /// A value of `None` indicates that the type could not be determined; this
    /// may occur if the type is a name reference to a struct that does not
    /// exist.
    pub fn ty(&self) -> Option<Type> {
        self.ty
    }

    /// Whether or not the name was introduced implicitly into the scope.
    ///
    /// This is true for names introduced in outer scopes from workflow scatter
    /// and conditional statements.
    pub fn implicit(&self) -> bool {
        self.implicit
    }

    /// Determines if the name was introduced for a scatter variable.
    fn is_scatter_variable(&self) -> bool {
        if !self.implicit {
            return matches!(self.context, ScopedNameContext::ScatterVariable(_));
        }

        false
    }
}

/// Represents a namespace introduced by an import.
#[derive(Debug)]
pub struct Namespace {
    /// The span of the import that introduced the namespace.
    span: Span,
    /// The CST node of the import that introduced the namespace.
    node: GreenNode,
    /// The URI of the imported document that introduced the namespace.
    source: Arc<Url>,
    /// The namespace's document scope.
    scope: Arc<DocumentScope>,
}

impl Namespace {
    /// Gets the CST node that introduced the namespace.
    ///
    /// The node is an import statement.
    pub fn node(&self) -> &GreenNode {
        &self.node
    }

    /// Gets the URI of the imported document that introduced the namespace.
    pub fn source(&self) -> &Arc<Url> {
        &self.source
    }

    /// Gets the scope of the imported document.
    pub fn scope(&self) -> &DocumentScope {
        &self.scope
    }
}

/// Represents a struct in a document.
#[derive(Debug, Clone)]
pub struct Struct {
    /// The span that introduced the struct.
    ///
    /// This is either the name of a struct definition (local) or an import's
    /// URI or alias (imported).
    span: Span,
    /// The namespace that defines the struct.
    ///
    /// This is `Some` only for imported structs.
    namespace: Option<String>,
    /// The CST node of the struct definition.
    node: GreenNode,
    /// The type of the struct.
    ///
    /// Initially this is `None` until a type check occurs.
    ty: Option<Type>,
    /// The index into the locally defined structs.
    ///
    /// This is `None` for imported structs.
    index: Option<usize>,
}

impl Struct {
    /// Gets the CST node of the struct definition.
    pub fn node(&self) -> &GreenNode {
        &self.node
    }

    /// Gets the namespace that defines this struct.
    ///
    /// Returns `None` for structs defined in the containing scope or `Some` for
    /// a struct introduced by an import.
    pub fn namespace(&self) -> Option<&str> {
        self.namespace.as_deref()
    }

    /// Gets the type of the struct.
    ///
    /// A value of `None` indicates that the type could not be determined for
    /// the struct; this may happen if the struct definition is recursive.
    pub fn ty(&self) -> Option<Type> {
        self.ty
    }

    /// Compares two structs for structural equality.
    fn is_equal(&self, other: &Self) -> bool {
        let a = StructDefinition::cast(SyntaxNode::new_root(self.node.clone()))
            .expect("node should cast");
        let b = StructDefinition::cast(SyntaxNode::new_root(other.node.clone()))
            .expect("node should cast");
        for (a, b) in a.members().zip(b.members()) {
            if a.name().as_str() != b.name().as_str() {
                return false;
            }

            if a.ty() != b.ty() {
                return false;
            }
        }

        true
    }
}

/// Represents a scope in a WDL document.
#[derive(Debug)]
pub struct Scope {
    /// The span in the document where the names of the scope are visible.
    span: Span,
    /// The CST node that introduced the scope.
    ///
    /// This may be a struct, task, workflow, conditional statement, or scatter
    /// statement.
    node: GreenNode,
    /// The names in the task scope.
    names: IndexMap<String, ScopedName>,
    /// The child scopes of this scope.
    ///
    /// Child scopes are from workflow conditional and scatter statements.
    children: Vec<Scope>,
}

impl Scope {
    /// Gets the span where the names of the scope are visible.
    pub fn span(&self) -> Span {
        self.span
    }

    /// Gets the CST node that introduced the scope.
    ///
    /// This may be a struct, task, workflow, conditional statement, or scatter
    /// statement.
    pub fn node(&self) -> &GreenNode {
        &self.node
    }

    /// Gets the names in the scope.
    pub fn names(&self) -> impl Iterator<Item = (&String, &ScopedName)> {
        self.names.iter()
    }

    /// Gets a name within the scope.
    pub fn get(&self, name: &str) -> Option<&ScopedName> {
        self.names.get(name)
    }

    /// Gets the child scopes of this scope.
    ///
    /// Child scopes may exist in workflows when conditional or scatter
    /// statements are present.
    pub fn children(&self) -> impl Iterator<Item = &Scope> {
        self.children.iter()
    }

    /// Finds the deepest child scope by position within the document.
    pub fn find_child_scope(&self, position: usize) -> Option<&Scope> {
        let scope = match self
            .children
            .binary_search_by_key(&position, |s| s.span.start())
        {
            Ok(index) => &self.children[index],
            Err(insertion) => {
                // This indicates that we couldn't find a match and the match would go _before_
                // the first child scope, so there is no corresponding scope.
                if insertion == 0 {
                    return None;
                }

                // Check to see if the span before the insertion point actually contains the
                // position.
                let child = &self.children[insertion - 1];
                if position - child.span.start() < child.span.len() {
                    return None;
                }

                child
            }
        };

        Some(scope.find_child_scope(position).unwrap_or(scope))
    }
}

/// Represents context about a scope in a document.
#[derive(Debug, Clone, Copy)]
enum ScopeContext {
    /// The scope is a task.
    ///
    /// The value is an index into the document's `tasks` collection.
    Task(usize),
    /// The scope is a workflow.
    Workflow,
}

/// Represents a task scope.
#[derive(Debug)]
struct TaskScope {
    /// The span of the task name.
    name_span: Span,
    /// The scope of the task.
    scope: Scope,
}

/// Represents a workflow scope.
#[derive(Debug)]
struct WorkflowScope {
    /// The span of the workflow name.
    name_span: Span,
    /// The name of the workflow.
    name: String,
    /// The scope of the workflow.
    scope: Scope,
}

/// Represents the scope of a document.
#[derive(Debug, Default)]
pub struct DocumentScope {
    /// The namespaces in the document.
    namespaces: IndexMap<String, Namespace>,
    /// The tasks in the document.
    tasks: IndexMap<String, TaskScope>,
    /// The singular workflow in the document.
    workflow: Option<WorkflowScope>,
    /// The structs in the document.
    structs: IndexMap<String, Struct>,
    /// A sorted list of scopes within the document.
    ///
    /// This can be used to quickly search for a scope by span.
    scopes: Vec<(Span, ScopeContext)>,
    /// The collection of types for the document.
    types: Types,
}

impl DocumentScope {
    /// Creates a new document scope for a given document.
    pub(crate) fn new(graph: &DocumentGraph, index: NodeIndex) -> (Self, Vec<Diagnostic>) {
        let node = graph.get(index);

        let mut diagnostics = match node.parse_state() {
            ParseState::NotParsed => panic!("node should have been parsed"),
            ParseState::Error(_) => return (Default::default(), Default::default()),
            ParseState::Parsed { diagnostics, .. } => {
                Vec::from_iter(diagnostics.as_ref().iter().cloned())
            }
        };

        let document = node.document().expect("node should have been parsed");

        let version = match document.version_statement() {
            Some(stmt) => stmt.version(),
            None => {
                // Don't process a document with a missing version
                return (Default::default(), diagnostics);
            }
        };

        let scope = match document.ast() {
            Ast::Unsupported => Default::default(),
            Ast::V1(ast) => Self::from_ast_v1(graph, index, &ast, &version, &mut diagnostics),
        };

        (scope, diagnostics)
    }

    /// Gets the namespaces in the document scope.
    pub fn namespaces(&self) -> impl Iterator<Item = (&String, &Namespace)> {
        self.namespaces.iter()
    }

    /// Gets a namespace in the document scope by name.
    pub fn namespace(&self, name: &str) -> Option<&Namespace> {
        self.namespaces.get(name)
    }

    /// Gets the task scopes in the document scope.
    pub fn task_scopes(&self) -> impl Iterator<Item = (&String, &Scope)> {
        self.tasks.iter().map(|(n, s)| (n, &s.scope))
    }

    /// Gets a task scope in the document scope by name.
    pub fn task_scope(&self, name: &str) -> Option<&Scope> {
        self.tasks.get(name).map(|s| &s.scope)
    }

    /// Gets the workflow scope in the document scope.
    pub fn workflow_scope(&self) -> Option<&Scope> {
        self.workflow.as_ref().map(|s| &s.scope)
    }

    /// Gets the structs in the document scope.
    pub fn structs(&self) -> impl Iterator<Item = (&String, &Struct)> {
        self.structs.iter()
    }

    /// Gets a struct in the document scope by name.
    pub fn struct_(&self, name: &str) -> Option<&Struct> {
        self.structs.get(name)
    }

    /// Gets the types of the document.
    pub fn types(&self) -> &Types {
        &self.types
    }

    /// Finds the deepest scope based on a position within the document.
    pub fn find_scope_by_position(&self, position: usize) -> Option<&Scope> {
        let context = match self
            .scopes
            .binary_search_by_key(&position, |(s, _)| s.start())
        {
            Ok(index) => self.scopes[index].1,
            Err(insertion) => {
                // This indicates that we couldn't find a match and the match would go _before_
                // the first scope, so there is no corresponding scope.
                if insertion == 0 {
                    return None;
                }

                // Check to see if the span before the insertion point actually contains the
                // position.
                let (span, context) = &self.scopes[insertion - 1];
                if position - span.start() < span.len() {
                    return None;
                }

                *context
            }
        };

        let scope = match context {
            ScopeContext::Task(index) => &self.tasks[index].scope,
            ScopeContext::Workflow => {
                &self
                    .workflow
                    .as_ref()
                    .expect("expected a workflow scope")
                    .scope
            }
        };

        Some(scope.find_child_scope(position).unwrap_or(scope))
    }

    /// Calculates the span of a scope given a node which uses braces to
    /// delineate the scope.
    fn scope_span(parent: &SyntaxNode) -> Span {
        let open = token(parent, SyntaxKind::OpenBrace).expect("task must have an opening brace");
        let close = parent
            .last_child_or_token()
            .and_then(SyntaxElement::into_token)
            .expect("task must have a last token");
        assert_eq!(
            close.kind(),
            SyntaxKind::CloseBrace,
            "the last token of a task should be a close brace"
        );
        let open = open.text_range().to_span();
        let close = close.text_range().to_span();

        // The span starts after the opening brace and before the closing brace
        Span::new(open.end(), close.start() - open.end())
    }
}
