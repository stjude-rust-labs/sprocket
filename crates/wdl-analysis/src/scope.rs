//! Representation of scopes for for WDL documents.

use std::fmt;
use std::sync::Arc;

use indexmap::IndexMap;
use petgraph::graph::NodeIndex;
use url::Url;
use wdl_ast::Ast;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::ToSpan;
use wdl_ast::WorkflowDescriptionLanguage;
use wdl_ast::support::token;

use crate::graph::DocumentGraph;
use crate::graph::ParseState;
use crate::types::Type;
use crate::types::Types;

mod v1;

/// Calculates the span of a scope given a braced node.
fn braced_scope_span(parent: &impl AstNode<Language = WorkflowDescriptionLanguage>) -> Span {
    scope_span(parent, SyntaxKind::OpenBrace, SyntaxKind::CloseBrace)
}

/// Calculates the span of a scope given a heredoc node.
fn heredoc_scope_span(parent: &impl AstNode<Language = WorkflowDescriptionLanguage>) -> Span {
    scope_span(parent, SyntaxKind::OpenHeredoc, SyntaxKind::CloseHeredoc)
}

/// Calculates the span of a scope given the node where the scope is visible.
fn scope_span(
    parent: &impl AstNode<Language = WorkflowDescriptionLanguage>,
    open: SyntaxKind,
    close: SyntaxKind,
) -> Span {
    let open = token(parent.syntax(), open)
        .expect("missing open token")
        .text_range()
        .to_span();
    let close = parent
        .syntax()
        .last_child_or_token()
        .and_then(|c| {
            if c.kind() == close {
                c.into_token()
            } else {
                None
            }
        })
        .expect("missing close token")
        .text_range()
        .to_span();

    // The span starts after the opening brace and before the closing brace
    Span::new(open.end(), close.start() - open.end())
}

/// Represents the context for diagnostic reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Context {
    /// The name is a workflow name.
    Workflow(Span),
    /// The name is a task name.
    Task(Span),
    /// The name is a struct name.
    Struct(Span),
    /// The name is a struct member name.
    StructMember(Span),
    /// A name from a scope.
    Name(NameContext),
}

impl Context {
    /// Gets the span of the name.
    fn span(&self) -> Span {
        match self {
            Self::Workflow(s) => *s,
            Self::Task(s) => *s,
            Self::Struct(s) => *s,
            Self::StructMember(s) => *s,
            Self::Name(n) => n.span(),
        }
    }
}

impl fmt::Display for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Workflow(_) => write!(f, "workflow"),
            Self::Task(_) => write!(f, "task"),
            Self::Struct(_) => write!(f, "struct"),
            Self::StructMember(_) => write!(f, "struct member"),
            Self::Name(n) => n.fmt(f),
        }
    }
}

/// Represents the context of a name in a scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NameContext {
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
    /// The name was introduced for the special `task` name in task command and
    /// outputs sections for WDL 1.2.
    Task(Span),
}

impl NameContext {
    /// Gets the span of the name.
    pub fn span(&self) -> Span {
        match self {
            Self::Input(s) => *s,
            Self::Output(s) => *s,
            Self::Decl(s) => *s,
            Self::Call(s) => *s,
            Self::ScatterVariable(s) => *s,
            Self::Task(s) => *s,
        }
    }
}

impl fmt::Display for NameContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input(_) => write!(f, "input"),
            Self::Output(_) => write!(f, "output"),
            Self::Decl(_) => write!(f, "declaration"),
            Self::Call(_) => write!(f, "call"),
            Self::ScatterVariable(_) => write!(f, "scatter variable"),
            Self::Task(_) => write!(f, "task"),
        }
    }
}

impl From<NameContext> for Context {
    fn from(context: NameContext) -> Self {
        Self::Name(context)
    }
}

/// Represents a namespace introduced by an import.
#[derive(Debug)]
pub struct Namespace {
    /// The span of the import that introduced the namespace.
    span: Span,
    /// The URI of the imported document that introduced the namespace.
    source: Arc<Url>,
    /// The namespace's document scope.
    scope: Arc<DocumentScope>,
}

impl Namespace {
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
    /// The offset of the CST node from the start of the document.
    ///
    /// This is used to adjust diagnostics resulting from traversing the struct
    /// node as if it were the root of the CST.
    offset: usize,
    /// Stores the CST node of the struct.
    ///
    /// This is used to calculate type equivalence for imports.
    node: rowan::GreenNode,
    /// The namespace that defines the struct.
    ///
    /// This is `Some` only for imported structs.
    namespace: Option<String>,
    /// The type of the struct.
    ///
    /// Initially this is `None` until a type check occurs.
    ty: Option<Type>,
}

impl Struct {
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
}

/// Represents a name in a scope.
#[derive(Debug, Clone, Copy)]
pub struct Name {
    /// The context of the name.
    context: NameContext,
    /// The type of the name.
    ///
    /// This is initially `None` until a type check occurs.
    ty: Option<Type>,
}

impl Name {
    /// Constructs a new name with the given context.
    fn new(context: NameContext) -> Self {
        Self { context, ty: None }
    }

    /// Gets the context of the name.
    pub(crate) fn context(&self) -> NameContext {
        self.context
    }

    /// Gets the type of the name.
    ///
    /// Returns `None` if the type could not be determined; for example, if the
    /// name's declared type is to an unknown struct.
    pub fn ty(&self) -> Option<Type> {
        self.ty
    }
}

/// Represents an index into a document's collection of scopes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ScopeIndex(usize);

/// Represents a scope in a WDL document.
#[derive(Debug)]
struct Scope {
    /// The index of the parent scope.
    ///
    /// This is `None` for top-level scopes (i.e. tasks and workflows).
    parent: Option<ScopeIndex>,
    /// The span in the document where the names of the scope are visible.
    span: Span,
    /// The names in the scope.
    names: IndexMap<String, Name>,
    /// The child scope indexes of this scope.
    ///
    /// Child scopes are from output sections, workflow conditional statements,
    /// and workflow scatter statements.
    children: Vec<ScopeIndex>,
}

impl Scope {
    /// Creates a new scope given the parent scope and span.
    fn new(parent: Option<ScopeIndex>, span: Span) -> Self {
        Self {
            parent,
            span,
            names: Default::default(),
            children: Default::default(),
        }
    }
}

/// Represents information about a scope for task outputs.
///
/// This is used in evaluation of a task `hints` section.
#[derive(Debug, Clone, Copy)]
enum TaskOutputScope {
    /// A task `output` section was not present.
    NotPresent,
    /// A task `output` section was present.
    ///
    /// Stores the scope index for the outputs.
    Present(ScopeIndex),
}

/// Represents a reference to a scope.
#[derive(Debug, Clone, Copy)]
pub struct ScopeRef<'a> {
    /// The reference to all scopes.
    scopes: &'a [Scope],
    /// The index of the scope.
    scope: ScopeIndex,
    /// The index to the scope that might contain input declarations.
    ///
    /// Unlike outputs, inputs don't have a dedicated scope; instead, they are
    /// accessible from the root scope of a task.
    ///
    /// This is `None` when `input` hidden types are not supported.
    inputs: Option<ScopeIndex>,
    /// The task output scope that's accessible from this scope.
    ///
    /// This is `None` when `output` hidden types are not supported.
    outputs: Option<TaskOutputScope>,
    /// Whether or not `hints` hidden types are supported in this scope.
    ///
    /// This is `true` only when evaluating the `hints` section in a task.
    hints: bool,
}

impl<'a> ScopeRef<'a> {
    /// Creates a new scope reference given the scope index.
    fn new(scopes: &'a [Scope], scope: ScopeIndex) -> Self {
        Self {
            scopes,
            scope,
            inputs: None,
            outputs: None,
            hints: false,
        }
    }

    /// Gets the parent scope.
    ///
    /// Returns `None` if there is no parent scope.
    pub fn parent(&self) -> Option<Self> {
        self.scopes[self.scope.0].parent.map(|p| Self {
            scopes: self.scopes,
            scope: p,
            inputs: self.inputs,
            outputs: self.outputs,
            hints: self.hints,
        })
    }

    /// Gets an iterator over the child scopes.
    pub fn children(&self) -> impl Iterator<Item = Self> + '_ {
        self.scopes[self.scope.0].children.iter().map(|c| Self {
            scopes: self.scopes,
            scope: *c,
            inputs: self.inputs,
            outputs: self.outputs,
            hints: self.hints,
        })
    }

    /// Gets all of the names available at this scope.
    pub fn names(&self) -> impl Iterator<Item = (&str, Name)> {
        self.scopes[self.scope.0]
            .names
            .iter()
            .map(|(s, name)| (s.as_str(), *name))
    }

    /// Gets a name local to this scope.
    ///
    /// Returns `None` if a name local to this scope was not found.
    pub fn local(&self, name: &str) -> Option<Name> {
        self.scopes[self.scope.0].names.get(name).copied()
    }

    /// Lookups a name in the scope.
    ///
    /// Returns `None` if the name is not available in the scope.
    pub fn lookup(&self, name: &str) -> Option<Name> {
        let mut scope = Some(self.scope);

        while let Some(index) = scope {
            if let Some(name) = self.scopes[index.0].names.get(name).copied() {
                return Some(name);
            }

            scope = self.scopes[index.0].parent;
        }

        None
    }

    /// Gets an input for the given name.
    ///
    /// Returns `Err(())` if input hidden types are not supported by this scope.
    ///
    /// Returns `Ok(None)` if input hidden types are supported, but the name is
    /// unknown.
    ///
    /// Returns `Ok(Some)` if input hidden types are supported and the name is
    /// known.
    pub(crate) fn input(&self, name: &str) -> Result<Option<Name>, ()> {
        match self.inputs {
            Some(scope) => Ok(self.scopes[scope.0]
                .names
                .get(name)
                .copied()
                .filter(|n| matches!(n.context, NameContext::Input(_)))),
            None => Err(()),
        }
    }

    /// Gets an output for the given name.
    ///
    /// Returns `Err(())` if output hidden types are not supported by this
    /// scope.
    ///
    /// Returns `Ok(None)` if output hidden types are supported, but the name is
    /// unknown.
    ///
    /// Returns `Ok(Some)` if output hidden types are supported and the name is
    /// known.
    pub(crate) fn output(&self, name: &str) -> Result<Option<Name>, ()> {
        match self.outputs {
            Some(TaskOutputScope::NotPresent) => Ok(None),
            Some(TaskOutputScope::Present(scope)) => Ok(self.scopes[scope.0]
                .names
                .get(name)
                .copied()
                .filter(|n| matches!(n.context, NameContext::Output(_)))),
            None => Err(()),
        }
    }

    /// Whether or not `hints` hidden types are supported by this scope.
    pub(crate) fn supports_hints(&self) -> bool {
        self.hints
    }

    /// Whether or not `input` hidden types are supported by this scope.
    pub(crate) fn supports_inputs(&self) -> bool {
        self.inputs.is_some()
    }

    /// Whether or not `output` hidden types are supported by this scope.
    pub(crate) fn supports_outputs(&self) -> bool {
        self.outputs.is_some()
    }
}

/// Represents a mutable reference to a scope.
#[derive(Debug)]
struct ScopeRefMut<'a> {
    /// The reference to all scopes.
    scopes: &'a mut [Scope],
    /// The index to the scope.
    scope: ScopeIndex,
}

impl<'a> ScopeRefMut<'a> {
    /// Lookups a name in the scope.
    ///
    /// Returns `None` if the name is not available in the scope.
    pub fn lookup(&self, name: &str) -> Option<Name> {
        let mut scope = Some(self.scope);

        while let Some(index) = scope {
            if let Some(name) = self.scopes[index.0].names.get(name).copied() {
                return Some(name);
            }

            scope = self.scopes[index.0].parent;
        }

        None
    }

    /// Inserts a name into the scope.
    pub fn insert(&mut self, key: String, name: Name) {
        self.scopes[self.scope.0].names.insert(key, name);
    }

    /// Adds a child scope to the scope.
    pub fn add_child(&mut self, child: ScopeIndex) {
        self.scopes[self.scope.0].children.push(child);
    }
}

/// Represents a task in a document.
#[derive(Debug)]
struct Task {
    /// The span of the task name.
    name_span: Span,
    /// The root scope index for the task.
    scope: ScopeIndex,
    /// The scope index for the outputs.
    outputs: Option<ScopeIndex>,
    /// The scope index for the command.
    command: Option<ScopeIndex>,
}

/// Represents a workflow in a document.
#[derive(Debug)]
struct Workflow {
    /// The span of the workflow name.
    name_span: Span,
    /// The name of the workflow.
    name: String,
    /// The scope index of the workflow.
    scope: ScopeIndex,
}

/// Represents the scope of a document.
#[derive(Debug, Default)]
pub struct DocumentScope {
    /// The version of the document.
    version: Option<SupportedVersion>,
    /// The namespaces in the document.
    namespaces: IndexMap<String, Namespace>,
    /// The tasks in the document.
    tasks: IndexMap<String, Task>,
    /// The singular workflow in the document.
    workflow: Option<Workflow>,
    /// The structs in the document.
    structs: IndexMap<String, Struct>,
    /// The scopes contained in the document.
    ///
    /// The scopes are in document order, so increasing by the start of their
    /// spans.
    scopes: Vec<Scope>,
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
            Ast::V1(ast) => v1::scope_from_ast(graph, index, &ast, &version, &mut diagnostics),
        };

        // Perform a type check
        (scope, diagnostics)
    }

    /// Gets the supported version of the document.
    ///
    /// Returns `None` if the document version is not supported.
    pub fn version(&self) -> Option<SupportedVersion> {
        self.version
    }

    /// Gets the namespaces in the document scope.
    pub fn namespaces(&self) -> impl Iterator<Item = (&str, &Namespace)> {
        self.namespaces.iter().map(|(n, ns)| (n.as_str(), ns))
    }

    /// Gets a namespace in the document scope by name.
    pub fn namespace(&self, name: &str) -> Option<&Namespace> {
        self.namespaces.get(name)
    }

    /// Gets the task scopes in the document scope.
    pub fn tasks(&self) -> impl Iterator<Item = (&str, ScopeRef<'_>)> {
        self.tasks
            .iter()
            .map(|(n, t)| (n.as_str(), ScopeRef::new(&self.scopes, t.scope)))
    }

    /// Gets a task's scope in the document scope by name.
    pub fn task_by_name(&self, name: &str) -> Option<ScopeRef<'_>> {
        self.tasks
            .get(name)
            .map(|t| ScopeRef::new(&self.scopes, t.scope))
    }

    /// Gets the workflow scope in the document scope.
    ///
    /// Returns the workflow name and scope if a workflow is present in the
    /// document.
    ///
    /// Returns `None` if the document did not contain a workflow.
    pub fn workflow(&self) -> Option<(&str, ScopeRef<'_>)> {
        self.workflow
            .as_ref()
            .map(|w| (w.name.as_str(), ScopeRef::new(&self.scopes, w.scope)))
    }

    /// Gets the structs in the document scope.
    pub fn structs(&self) -> impl Iterator<Item = (&str, &Struct)> {
        self.structs.iter().map(|(n, s)| (n.as_str(), s))
    }

    /// Gets a struct in the document scope by name.
    pub fn struct_by_name(&self, name: &str) -> Option<&Struct> {
        self.structs.get(name)
    }

    /// Gets the types of the document.
    pub fn types(&self) -> &Types {
        &self.types
    }

    /// Finds a scope based on a position within the document.
    pub fn find_scope_by_position(&self, position: usize) -> Option<ScopeRef<'_>> {
        let mut index = match self
            .scopes
            .binary_search_by_key(&position, |s| s.span.start())
        {
            Ok(index) => index,
            Err(index) => {
                // This indicates that we couldn't find a match and the match would go _before_
                // the first scope, so there is no corresponding scope.
                if index == 0 {
                    return None;
                }

                index - 1
            }
        };

        // We now have the index to start looking up the list of scopes
        // We walk up the list to try to find a span that contains the position
        loop {
            let scope = &self.scopes[index];
            if position >= scope.span.start() && position < scope.span.end() {
                return Some(ScopeRef::new(&self.scopes, ScopeIndex(index)));
            }

            if index == 0 {
                break;
            }

            index -= 1;
        }

        None
    }

    /// Adds an inner scope to the document scope.
    fn add_scope(&mut self, scope: Scope) -> ScopeIndex {
        // Scopes are added in order, so the span start should always be increasing
        assert!(
            self.scopes
                .last()
                .map(|s| s.span.start() < scope.span.start())
                .unwrap_or(true)
        );

        let index = self.scopes.len();
        self.scopes.push(scope);
        ScopeIndex(index)
    }

    /// Gets a reference to a scope.
    pub(crate) fn scope(&self, scope: ScopeIndex) -> ScopeRef<'_> {
        ScopeRef::new(&self.scopes, scope)
    }

    /// Gets a mutable reference to a scope.
    fn scope_mut(&mut self, scope: ScopeIndex) -> ScopeRefMut<'_> {
        ScopeRefMut {
            scopes: &mut self.scopes,
            scope,
        }
    }
}
