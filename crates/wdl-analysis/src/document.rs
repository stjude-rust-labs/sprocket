//! Representation of analyzed WDL documents.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;

use indexmap::IndexMap;
use petgraph::graph::NodeIndex;
use url::Url;
use wdl_ast::Ast;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::ToSpan;
use wdl_ast::WorkflowDescriptionLanguage;
use wdl_ast::support::token;

use crate::DiagnosticsConfig;
use crate::diagnostics::unused_import;
use crate::graph::DocumentGraph;
use crate::graph::ParseState;
use crate::types::Type;
use crate::types::Types;

mod v1;

/// The `task` variable name available in task command sections and outputs in
/// WDL 1.2.
pub(crate) const TASK_VAR_NAME: &str = "task";

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

/// Represents a namespace introduced by an import.
#[derive(Debug)]
pub struct Namespace {
    /// The span of the import that introduced the namespace.
    span: Span,
    /// The URI of the imported document that introduced the namespace.
    source: Arc<Url>,
    /// The namespace's document.
    document: Arc<Document>,
    /// Whether or not the namespace is used (i.e. referenced) in the document.
    used: bool,
    /// Whether or not the namespace is excepted from the "unused import"
    /// diagnostic.
    excepted: bool,
}

impl Namespace {
    /// Gets the span of the import that introduced the namespace.
    pub fn span(&self) -> Span {
        self.span
    }

    /// Gets the URI of the imported document that introduced the namespace.
    pub fn source(&self) -> &Arc<Url> {
        &self.source
    }

    /// Gets the imported document.
    pub fn document(&self) -> &Document {
        &self.document
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
    /// Returns `None` for structs defined in the containing document or `Some`
    /// for a struct introduced by an import.
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

/// Represents information about a name in a scope.
#[derive(Debug, Clone, Copy)]
pub struct Name {
    /// The span of the name.
    span: Span,
    /// The type of the name.
    ty: Type,
}

impl Name {
    /// Gets the span of the name.
    pub fn span(&self) -> Span {
        self.span
    }

    /// Gets the type of the name.
    pub fn ty(&self) -> Type {
        self.ty
    }
}

/// Represents an index of a scope in a collection of scopes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ScopeIndex(usize);

/// Represents a scope in a WDL document.
#[derive(Debug)]
struct Scope {
    /// The index of the parent scope.
    ///
    /// This is `None` for task and workflow scopes.
    parent: Option<ScopeIndex>,
    /// The span in the document where the names of the scope are visible.
    span: Span,
    /// The map of names in scope to their span and types.
    names: IndexMap<String, Name>,
}

impl Scope {
    /// Creates a new scope given the parent scope and span.
    fn new(parent: Option<ScopeIndex>, span: Span) -> Self {
        Self {
            parent,
            span,
            names: Default::default(),
        }
    }

    /// Inserts a name into the scope.
    pub fn insert(&mut self, name: impl Into<String>, span: Span, ty: Type) {
        self.names.insert(name.into(), Name { span, ty });
    }
}

/// Represents a reference to a scope.
#[derive(Debug, Clone, Copy)]
pub struct ScopeRef<'a> {
    /// The reference to the scopes collection.
    scopes: &'a [Scope],
    /// The index of the scope in the collection.
    index: ScopeIndex,
    /// The name of the task associated with the scope.
    ///
    /// This is `Some` only when evaluating a task `hints` section.
    task_name: Option<&'a str>,
    /// The input type map.
    ///
    /// This is `Some` only when evaluating a task `hints` section.
    inputs: Option<&'a HashMap<String, Input>>,
    /// The output type map.
    ///
    /// This is `Some` only when evaluating a task `hints` section.
    outputs: Option<&'a HashMap<String, Output>>,
}

impl<'a> ScopeRef<'a> {
    /// Creates a new scope reference given the scope index.
    fn new(scopes: &'a [Scope], index: ScopeIndex) -> Self {
        Self {
            scopes,
            index,
            task_name: None,
            inputs: None,
            outputs: None,
        }
    }

    /// Gets the span of the scope.
    pub fn span(&self) -> Span {
        self.scopes[self.index.0].span
    }

    /// Gets the parent scope.
    ///
    /// Returns `None` if there is no parent scope.
    pub fn parent(&self) -> Option<Self> {
        self.scopes[self.index.0].parent.map(|p| Self {
            scopes: self.scopes,
            index: p,
            task_name: self.task_name,
            inputs: self.inputs,
            outputs: self.outputs,
        })
    }

    /// Gets all of the names available at this scope.
    pub fn names(&self) -> impl Iterator<Item = (&str, Name)> + use<'_> {
        self.scopes[self.index.0]
            .names
            .iter()
            .map(|(name, span_ty)| (name.as_str(), *span_ty))
    }

    /// Gets a name local to this scope.
    ///
    /// Returns `None` if a name local to this scope was not found.
    pub fn local(&self, name: &str) -> Option<Name> {
        self.scopes[self.index.0].names.get(name).copied()
    }

    /// Lookups a name in the scope.
    ///
    /// Returns `None` if the name is not available in the scope.
    pub fn lookup(&self, name: &str) -> Option<Name> {
        let mut current = Some(self.index);

        while let Some(index) = current {
            if let Some(name) = self.scopes[index.0].names.get(name).copied() {
                return Some(name);
            }

            current = self.scopes[index.0].parent;
        }

        None
    }

    /// Gets an input for the given name.
    ///
    /// Returns `Err(())` if input hidden types are not supported by this scope.
    ///
    /// Returns `Ok(None)` if input hidden types are supported, but the
    /// specified name is not a known input.
    ///
    /// Returns `Ok(Some)` if input hidden types are supported and the specified
    /// name is a known input.
    pub(crate) fn input(&self, name: &str) -> Result<Option<Input>, ()> {
        match self.inputs {
            Some(map) => Ok(map.get(name).copied()),
            None => Err(()),
        }
    }

    /// Gets an output for the given name.
    ///
    /// Returns `Err(())` if output hidden types are not supported by this
    /// scope.
    ///
    /// Returns `Ok(None)` if input hidden types are supported, but the
    /// specified name is not a known output.
    ///
    /// Returns `Ok(Some)` if input hidden types are supported and the specified
    /// name is a known output.
    pub(crate) fn output(&self, name: &str) -> Result<Option<Output>, ()> {
        match self.outputs {
            Some(map) => Ok(map.get(name).copied()),
            None => Err(()),
        }
    }

    /// The task name associated with the scope.
    pub(crate) fn task_name(&self) -> Option<&str> {
        self.task_name
    }

    /// Whether or not `hints` hidden types are supported by this scope.
    pub(crate) fn supports_hints(&self) -> bool {
        self.task_name.is_some()
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
    index: ScopeIndex,
}

impl<'a> ScopeRefMut<'a> {
    /// Creates a new mutable scope reference given the scope index.
    fn new(scopes: &'a mut [Scope], index: ScopeIndex) -> Self {
        Self { scopes, index }
    }

    /// Lookups a name in the scope.
    ///
    /// Returns `None` if the name is not available in the scope.
    pub fn lookup(&self, name: &str) -> Option<Name> {
        let mut current = Some(self.index);

        while let Some(index) = current {
            if let Some(name) = self.scopes[index.0].names.get(name).copied() {
                return Some(name);
            }

            current = self.scopes[index.0].parent;
        }

        None
    }

    /// Inserts a name into the scope.
    pub fn insert(&mut self, name: impl Into<String>, span: Span, ty: Type) {
        self.scopes[self.index.0]
            .names
            .insert(name.into(), Name { span, ty });
    }

    /// Converts the mutable scope reference to an immutable scope reference.
    pub fn into_scope_ref(self) -> ScopeRef<'a> {
        ScopeRef {
            scopes: self.scopes,
            index: self.index,
            task_name: None,
            inputs: None,
            outputs: None,
        }
    }
}

/// Represents a task or workflow input.
#[derive(Debug, Clone, Copy)]
pub struct Input {
    /// The type of the input.
    ty: Type,
    /// Whether or not the input is required.
    required: bool,
}

impl Input {
    /// Gets the type of the input.
    pub fn ty(&self) -> Type {
        self.ty
    }

    /// Whether or not the input is required.
    pub fn required(&self) -> bool {
        self.required
    }
}

/// Represents a task or workflow output.
#[derive(Debug, Clone, Copy)]
pub struct Output {
    /// The type of the output.
    ty: Type,
}

impl Output {
    /// Creates a new output with the given type.
    pub(crate) fn new(ty: Type) -> Self {
        Self { ty }
    }

    /// Gets the type of the output.
    pub fn ty(&self) -> Type {
        self.ty
    }
}

/// Represents a task in a document.
#[derive(Debug)]
pub struct Task {
    /// The span of the task name.
    name_span: Span,
    /// The name of the task.
    name: String,
    /// The scopes contained in the task.
    ///
    /// The first scope will always be the task's scope.
    ///
    /// The scopes will be in sorted order by span start.
    scopes: Vec<Scope>,
    /// The inputs of the task.
    inputs: HashMap<String, Input>,
    /// The outputs of the task.
    outputs: HashMap<String, Output>,
}

impl Task {
    /// Gets the name of the task.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Gets the scope of the task.
    pub fn scope(&self) -> ScopeRef<'_> {
        ScopeRef::new(&self.scopes, ScopeIndex(0))
    }

    /// Gets the inputs of the task.
    pub fn inputs(&self) -> &HashMap<String, Input> {
        &self.inputs
    }

    /// Gets the outputs of the task.
    pub fn outputs(&self) -> &HashMap<String, Output> {
        &self.outputs
    }
}

/// Represents a workflow in a document.
#[derive(Debug)]
pub struct Workflow {
    /// The span of the workflow name.
    name_span: Span,
    /// The name of the workflow.
    name: String,
    /// The scopes contained in the workflow.
    ///
    /// The first scope will always be the workflow's scope.
    ///
    /// The scopes will be in sorted order by span start.
    scopes: Vec<Scope>,
    /// The inputs of the workflow.
    inputs: HashMap<String, Input>,
    /// The outputs of the workflow.
    outputs: HashMap<String, Output>,
}

impl Workflow {
    /// Gets the name of the workflow.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Gets the scope of the workflow.
    pub fn scope(&self) -> ScopeRef<'_> {
        ScopeRef::new(&self.scopes, ScopeIndex(0))
    }

    /// Gets the inputs of the workflow.
    pub fn inputs(&self) -> &HashMap<String, Input> {
        &self.inputs
    }

    /// Gets the outputs of the workflow.
    pub fn outputs(&self) -> &HashMap<String, Output> {
        &self.outputs
    }
}

/// Represents an analyzed WDL document.
#[derive(Debug, Default)]
pub struct Document {
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
    /// The collection of types for the document.
    types: Types,
}

impl Document {
    /// Creates a new analyzed document.
    pub(crate) fn new(
        config: DiagnosticsConfig,
        graph: &DocumentGraph,
        index: NodeIndex,
    ) -> (Self, Vec<Diagnostic>) {
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

        let config =
            config.excepted_for_node(&version.syntax().parent().expect("token should have parent"));

        let document = match document.ast() {
            Ast::Unsupported => Default::default(),
            Ast::V1(ast) => {
                v1::create_document(config, graph, index, &ast, &version, &mut diagnostics)
            }
        };

        // Check for unused imports
        if let Some(severity) = config.unused_import {
            for (name, ns) in document
                .namespaces()
                .filter(|(_, ns)| !ns.used && !ns.excepted)
            {
                diagnostics.push(unused_import(name, ns.span()).with_severity(severity));
            }
        }

        // Sort the diagnostics by start
        diagnostics.sort_by(|a, b| match (a.labels().next(), b.labels().next()) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            (Some(a), Some(b)) => a.span().start().cmp(&b.span().start()),
        });

        // Perform a type check
        (document, diagnostics)
    }

    /// Gets the supported version of the document.
    ///
    /// Returns `None` if the document version is not supported.
    pub fn version(&self) -> Option<SupportedVersion> {
        self.version
    }

    /// Gets the namespaces in the document.
    pub fn namespaces(&self) -> impl Iterator<Item = (&str, &Namespace)> {
        self.namespaces.iter().map(|(n, ns)| (n.as_str(), ns))
    }

    /// Gets a namespace in the document by name.
    pub fn namespace(&self, name: &str) -> Option<&Namespace> {
        self.namespaces.get(name)
    }

    /// Gets the tasks in the document.
    pub fn tasks(&self) -> impl Iterator<Item = &Task> {
        self.tasks.iter().map(|(_, t)| t)
    }

    /// Gets a task in the document by name.
    pub fn task_by_name(&self, name: &str) -> Option<&Task> {
        self.tasks.get(name)
    }

    /// Gets a workflow in the document.
    ///
    /// Returns `None` if the document did not contain a workflow.
    pub fn workflow(&self) -> Option<&Workflow> {
        self.workflow.as_ref()
    }

    /// Gets the structs in the document.
    pub fn structs(&self) -> impl Iterator<Item = (&str, &Struct)> {
        self.structs.iter().map(|(n, s)| (n.as_str(), s))
    }

    /// Gets a struct in the document by name.
    pub fn struct_by_name(&self, name: &str) -> Option<&Struct> {
        self.structs.get(name)
    }

    /// Gets the types of the document.
    pub fn types(&self) -> &Types {
        &self.types
    }

    /// Finds a scope based on a position within the document.
    pub fn find_scope_by_position(&self, position: usize) -> Option<ScopeRef<'_>> {
        /// Finds a scope within a collection of sorted scopes by position.
        fn find_scope(scopes: &[Scope], position: usize) -> Option<ScopeRef<'_>> {
            let mut index = match scopes.binary_search_by_key(&position, |s| s.span.start()) {
                Ok(index) => index,
                Err(index) => {
                    // This indicates that we couldn't find a match and the match would go _before_
                    // the first scope, so there is no containing scope.
                    if index == 0 {
                        return None;
                    }

                    index - 1
                }
            };

            // We now have the index to start looking up the list of scopes
            // We walk up the list to try to find a span that contains the position
            loop {
                let scope = &scopes[index];
                if scope.span.contains(position) {
                    return Some(ScopeRef::new(scopes, ScopeIndex(index)));
                }

                if index == 0 {
                    return None;
                }

                index -= 1;
            }
        }

        // Check to see if the position is contained in the workflow
        if let Some(workflow) = &self.workflow {
            if workflow.scope().span().contains(position) {
                return find_scope(&workflow.scopes, position);
            }
        }

        // Search for a task that might contain the position
        let task = match self
            .tasks
            .binary_search_by_key(&position, |_, t| t.scope().span().start())
        {
            Ok(index) => &self.tasks[index],
            Err(index) => {
                // This indicates that we couldn't find a match and the match would go _before_
                // the first task, so there is no containing task.
                if index == 0 {
                    return None;
                }

                &self.tasks[index - 1]
            }
        };

        if task.scope().span().contains(position) {
            return find_scope(&task.scopes, position);
        }

        None
    }
}
