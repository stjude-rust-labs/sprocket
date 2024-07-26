//! Implementation of scopes for WDL documents.

use std::fmt;
use std::sync::Arc;

use indexmap::IndexMap;
use petgraph::graph::NodeIndex;
use rowan::GreenNode;
use url::Url;
use wdl_ast::support::token;
use wdl_ast::v1;
use wdl_ast::v1::ImportStatement;
use wdl_ast::v1::StringPart;
use wdl_ast::v1::WorkflowStatement;
use wdl_ast::Ast;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Ident;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_ast::ToSpan;
use wdl_ast::Version;

use crate::graph::DocumentGraph;
use crate::graph::ParseState;

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

/// Creates an "empty import" diagnostic
fn empty_import(span: Span) -> Diagnostic {
    Diagnostic::error("import URI cannot be empty").with_highlight(span)
}

/// Creates a "placeholder in import" diagnostic
fn placeholder_in_import(span: Span) -> Diagnostic {
    Diagnostic::error("import URI cannot contain placeholders")
        .with_label("remove this placeholder", span)
}

/// Creates a "name conflict" diagnostic
fn name_conflict(name: &str, conflicting: NameContext, first: NameContext) -> Diagnostic {
    Diagnostic::error(format!("conflicting {conflicting} name `{name}`"))
        .with_label(
            format!("this conflicts with a {first} of the same name"),
            conflicting.span(),
        )
        .with_label(
            format!("the {first} with the conflicting name is here"),
            first.span(),
        )
}

/// Creates a "namespace conflict" diagnostic
fn namespace_conflict(name: &str, conflicting: Span, first: Span, suggest_fix: bool) -> Diagnostic {
    let diagnostic = Diagnostic::error(format!("conflicting import namespace `{name}`"))
        .with_label("this conflicts with another import namespace", conflicting)
        .with_label(
            "the conflicting import namespace was introduced here",
            first,
        );

    if suggest_fix {
        diagnostic.with_fix("add an `as` clause to the import to specify a namespace")
    } else {
        diagnostic
    }
}

/// Creates an "invalid import namespace" diagnostic
fn invalid_import_namespace(span: Span) -> Diagnostic {
    Diagnostic::error("import namespace is not a valid WDL identifier")
        .with_label("a namespace cannot be derived from this import path", span)
        .with_fix("add an `as` clause to the import to specify a namespace")
}

/// Creates an "import cycle" diagnostic
fn import_cycle(span: Span) -> Diagnostic {
    Diagnostic::error("import introduces a dependency cycle")
        .with_label("this import has been skipped to break the cycle", span)
}

/// Creates an "import failure" diagnostic
fn import_failure(uri: &str, error: &anyhow::Error, span: Span) -> Diagnostic {
    Diagnostic::error(format!("failed to import `{uri}`: {error:?}")).with_highlight(span)
}

/// Creates an "incompatible import" diagnostic
fn incompatible_import(
    import_version: &str,
    import_span: Span,
    importer_version: &Version,
) -> Diagnostic {
    Diagnostic::error("imported document has incompatible version")
        .with_label(
            format!("the imported document is version `{import_version}`"),
            import_span,
        )
        .with_label(
            format!(
                "the importing document is version `{version}`",
                version = importer_version.as_str()
            ),
            importer_version.span(),
        )
}

/// Creates an "import missing version" diagnostic
fn import_missing_version(span: Span) -> Diagnostic {
    Diagnostic::error("imported document is missing a version statement").with_highlight(span)
}

/// Creates an "invalid relative import" diagnostic
fn invalid_relative_import(error: &url::ParseError, span: Span) -> Diagnostic {
    Diagnostic::error(format!("{error:?}")).with_highlight(span)
}

/// Creates a "struct not in scope" diagnostic
fn struct_not_in_scope(name: &Ident) -> Diagnostic {
    Diagnostic::error(format!(
        "a struct named `{name}` does not exist in the imported document",
        name = name.as_str()
    ))
    .with_label("this struct does not exist", name.span())
}

/// Creates an "imported struct conflict" diagnostic
fn imported_struct_conflict(
    name: &str,
    conflicting: Span,
    first: Span,
    suggest_fix: bool,
) -> Diagnostic {
    let diagnostic = Diagnostic::error(format!("conflicting struct name `{name}`"))
        .with_label(
            "this import introduces a conflicting definition",
            conflicting,
        )
        .with_label("the first definition was introduced by this import", first);

    if suggest_fix {
        diagnostic.with_fix("add an `alias` clause to the import to specify a different name")
    } else {
        diagnostic
    }
}

/// Creates a "struct conflicts with import" diagnostic
fn struct_conflicts_with_import(name: &str, conflicting: Span, import: Span) -> Diagnostic {
    Diagnostic::error(format!("conflicting struct name `{name}`"))
        .with_label("this name conflicts with an imported struct", conflicting)
        .with_label("the import that introduced the struct is here", import)
        .with_fix(
            "either rename the struct or use an `alias` clause on the import with a different name",
        )
}

/// Creates a "duplicate workflow" diagnostic
fn duplicate_workflow(name: &Ident, first: Span) -> Diagnostic {
    Diagnostic::error(format!(
        "cannot define workflow `{name}` as only one workflow is allowed per source file",
        name = name.as_str(),
    ))
    .with_label("consider moving this workflow to a new file", name.span())
    .with_label("first workflow is defined here", first)
}

/// Creates a "call conflict" diagnostic
fn call_conflict(name: &Ident, first: NameContext, suggest_fix: bool) -> Diagnostic {
    let diagnostic = Diagnostic::error(format!(
        "conflicting call name `{name}`",
        name = name.as_str()
    ))
    .with_label(
        format!("this conflicts with a {first} of the same name"),
        name.span(),
    )
    .with_label(
        format!("the {first} with the conflicting name is here"),
        first.span(),
    );

    if suggest_fix {
        diagnostic.with_fix("add an `as` clause to the call to specify a different name")
    } else {
        diagnostic
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
    /// Whether or not the name was implicitly introduced.
    ///
    /// This is true for names introduced in outer scopes from workflow scatter
    /// and conditional statements.
    implicit: bool,
}

impl ScopedName {
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
#[derive(Debug)]
pub struct Struct {
    /// The span that introduced the struct.
    ///
    /// This is either the name of a struct definition (local) or an import's
    /// URI or alias (imported).
    span: Span,
    /// The source document that defines the struct.
    ///
    /// This is `Some` only for imported structs.
    source: Option<Arc<Url>>,
    /// The CST node of the struct definition.
    node: GreenNode,
    /// The members of the struct.
    members: Arc<IndexMap<String, (Span, GreenNode)>>,
}

impl Struct {
    /// Gets the CST node of the struct definition.
    pub fn node(&self) -> &GreenNode {
        &self.node
    }

    /// Gets the source document that defines this struct.
    ///
    /// Returns `None` for structs defined in the containing scope or `Some` for
    /// a struct introduced by an import.
    pub fn source(&self) -> Option<&Arc<Url>> {
        self.source.as_ref()
    }

    /// Gets the members of the struct.
    pub fn members(&self) -> impl Iterator<Item = (&String, &GreenNode)> {
        self.members.iter().map(|(name, (_, node))| (name, node))
    }

    /// Gets a member of the struct by name.
    pub fn get_member(&self, name: &str) -> Option<&GreenNode> {
        self.members.get(name).map(|(_, n)| n)
    }

    /// Compares two structs for structural equality.
    fn is_equal(&self, other: &Self) -> bool {
        for ((a_name, a_node), (b_name, b_node)) in self.members().zip(other.members()) {
            if a_name != b_name {
                return false;
            }

            let adecl = v1::UnboundDecl::cast(SyntaxNode::new_root(a_node.clone()))
                .expect("node should cast");
            let bdecl = v1::UnboundDecl::cast(SyntaxNode::new_root(b_node.clone()))
                .expect("node should cast");
            if adecl.ty() != bdecl.ty() {
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
    pub fn get_name(&self, name: &str) -> Option<&ScopedName> {
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
    /// The scope of the task.
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
}

impl DocumentScope {
    /// Creates a new document scope for a given document.
    pub(crate) fn new(graph: &DocumentGraph, index: NodeIndex) -> (Self, Vec<Diagnostic>) {
        let mut scope = Self::default();
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
                return (scope, diagnostics);
            }
        };

        match document.ast() {
            Ast::Unsupported => {}
            Ast::V1(ast) => {
                for item in ast.items() {
                    match item {
                        v1::DocumentItem::Import(import) => {
                            scope.add_namespace(graph, &import, index, &version, &mut diagnostics);
                        }
                        v1::DocumentItem::Struct(s) => {
                            scope.add_struct(&s, &mut diagnostics);
                        }
                        v1::DocumentItem::Task(task) => {
                            scope.add_task_scope(&task, &mut diagnostics);
                        }
                        v1::DocumentItem::Workflow(workflow) => {
                            scope.add_workflow_scope(&workflow, &mut diagnostics);
                        }
                    }
                }
            }
        }

        (scope, diagnostics)
    }

    /// Gets the namespaces in the document scope.
    pub fn namespaces(&self) -> impl Iterator<Item = (&String, &Namespace)> {
        self.namespaces.iter()
    }

    /// Gets a namespace in the document scope by name.
    pub fn get_namespace(&self, name: &str) -> Option<&Namespace> {
        self.namespaces.get(name)
    }

    /// Gets the task scopes in the document scope.
    pub fn task_scopes(&self) -> impl Iterator<Item = (&String, &Scope)> {
        self.tasks.iter().map(|(n, s)| (n, &s.scope))
    }

    /// Gets a task scope in the document scope by name.
    pub fn get_task_scope(&self, name: &str) -> Option<&Scope> {
        self.tasks.get(name).map(|s| &s.scope)
    }

    /// Gets the workflow scope in the document scope.
    pub fn get_workflow_scope(&self) -> Option<&Scope> {
        self.workflow.as_ref().map(|s| &s.scope)
    }

    /// Gets the structs in the document scope.
    pub fn structs(&self) -> impl Iterator<Item = (&String, &Struct)> {
        self.structs.iter()
    }

    /// Gets a struct in the document scope by name.
    pub fn get_struct(&self, name: &str) -> Option<&Struct> {
        self.structs.get(name)
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

    /// Adds a namespace to the document scope.
    fn add_namespace(
        &mut self,
        graph: &DocumentGraph,
        import: &ImportStatement,
        importer_index: NodeIndex,
        importer_version: &Version,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Start by resolving the import to its document scope
        let (uri, scope) =
            match Self::resolve_import(graph, import, importer_index, importer_version) {
                Ok(scope) => scope,
                Err(diagnostic) => {
                    diagnostics.push(diagnostic);
                    return;
                }
            };

        // Check for conflicting namespaces
        let span = import.uri().syntax().text_range().to_span();
        match import.namespace() {
            Some((ns, span)) => {
                if let Some(prev) = self.namespaces.get(&ns) {
                    diagnostics.push(namespace_conflict(
                        &ns,
                        span,
                        prev.span,
                        import.explicit_namespace().is_none(),
                    ));
                    return;
                } else {
                    self.namespaces.insert(
                        ns,
                        Namespace {
                            span,
                            node: import.syntax().green().into(),
                            source: uri.clone(),
                            scope: scope.clone(),
                        },
                    );
                }
            }
            None => {
                diagnostics.push(invalid_import_namespace(span));
                return;
            }
        }

        // Get the alias map for the structs in the document
        let aliases = import
            .aliases()
            .filter_map(|a| {
                let (from, to) = a.names();
                if !scope.structs.contains_key(from.as_str()) {
                    diagnostics.push(struct_not_in_scope(&from));
                    return None;
                }

                Some((from.as_str().to_string(), to))
            })
            .collect::<IndexMap<_, _>>();

        // Insert the scope's struct definitions
        for (name, scope) in &scope.structs {
            let (aliased_name, span, aliased) = aliases
                .get(name)
                .map(|a| (a.as_str(), a.span(), true))
                .unwrap_or_else(|| (name, span, false));
            match self.structs.get(aliased_name) {
                Some(prev) => {
                    // Import conflicts with a struct defined in this document
                    if prev.source.is_none() {
                        diagnostics.push(struct_conflicts_with_import(
                            aliased_name,
                            prev.span,
                            span,
                        ));
                        continue;
                    }

                    if !prev.is_equal(scope) {
                        diagnostics.push(imported_struct_conflict(
                            aliased_name,
                            span,
                            prev.span,
                            !aliased,
                        ));
                        continue;
                    }
                }
                None => {
                    self.structs.insert(
                        aliased_name.to_string(),
                        Struct {
                            span,
                            source: Some(scope.source.clone().unwrap_or(uri.clone())),
                            node: scope.node.clone(),
                            members: scope.members.clone(),
                        },
                    );
                }
            }
        }
    }

    /// Adds a struct to the document scope.
    fn add_struct(&mut self, definition: &v1::StructDefinition, diagnostics: &mut Vec<Diagnostic>) {
        let name = definition.name();
        if let Some(prev) = self.structs.get(name.as_str()) {
            if prev.source.is_some() {
                diagnostics.push(struct_conflicts_with_import(
                    name.as_str(),
                    name.span(),
                    prev.span,
                ))
            } else {
                diagnostics.push(name_conflict(
                    name.as_str(),
                    NameContext::Struct(name.span()),
                    NameContext::Struct(prev.span),
                ));
            }
        } else {
            let mut members = IndexMap::new();
            for decl in definition.members() {
                let name = decl.name();
                if let Some((prev_span, _)) = members.get(name.as_str()) {
                    diagnostics.push(name_conflict(
                        name.as_str(),
                        NameContext::StructMember(name.span()),
                        NameContext::StructMember(*prev_span),
                    ));
                } else {
                    members.insert(
                        name.as_str().to_string(),
                        (name.span(), decl.syntax().green().into()),
                    );
                }
            }

            self.structs.insert(
                name.as_str().to_string(),
                Struct {
                    span: name.span(),
                    source: None,
                    node: definition.syntax().green().into(),
                    members: Arc::new(members),
                },
            );
        }
    }

    /// Adds inputs to a names collection.
    fn add_inputs(
        names: &mut IndexMap<String, ScopedName>,
        section: &v1::InputSection,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for decl in section.declarations() {
            let name = decl.name();
            let context = ScopedNameContext::Input(name.span());
            if let Some(prev) = names.get(name.as_str()) {
                diagnostics.push(name_conflict(
                    name.as_str(),
                    context.into(),
                    prev.context().into(),
                ));
                continue;
            }

            names.insert(
                name.as_str().to_string(),
                ScopedName {
                    context,
                    node: decl.syntax().green().into(),
                    implicit: false,
                },
            );
        }
    }

    /// Adds outputs to a names collection.
    fn add_outputs(
        names: &mut IndexMap<String, ScopedName>,
        section: &v1::OutputSection,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for decl in section.declarations() {
            let name = decl.name();
            let context = ScopedNameContext::Output(name.span());
            if let Some(prev) = names.get(name.as_str()) {
                diagnostics.push(name_conflict(
                    name.as_str(),
                    context.into(),
                    prev.context().into(),
                ));
                continue;
            }

            names.insert(
                name.as_str().to_string(),
                ScopedName {
                    context,
                    node: decl.syntax().green().into(),
                    implicit: false,
                },
            );
        }
    }

    /// Adds a task scope to the document's scope.
    fn add_task_scope(&mut self, task: &v1::TaskDefinition, diagnostics: &mut Vec<Diagnostic>) {
        // Check for a conflict with another task or workflow
        let name = task.name();
        if let Some(s) = self.tasks.get(name.as_str()) {
            diagnostics.push(name_conflict(
                name.as_str(),
                NameContext::Task(name.span()),
                NameContext::Task(s.name_span),
            ));
            return;
        } else if let Some(s) = &self.workflow {
            if s.name == name.as_str() {
                diagnostics.push(name_conflict(
                    name.as_str(),
                    NameContext::Task(name.span()),
                    NameContext::Workflow(s.name_span),
                ));
                return;
            }
        }

        // Populate the scope's names
        let mut names: IndexMap<_, ScopedName> = IndexMap::new();
        let mut saw_input = false;
        let mut saw_output = false;
        for item in task.items() {
            match item {
                v1::TaskItem::Input(section) if !saw_input => {
                    saw_input = true;
                    Self::add_inputs(&mut names, &section, diagnostics);
                }
                v1::TaskItem::Output(section) if !saw_output => {
                    saw_output = true;
                    Self::add_outputs(&mut names, &section, diagnostics);
                }
                v1::TaskItem::Declaration(decl) => {
                    let name = decl.name();
                    let context = ScopedNameContext::Decl(name.span());
                    if let Some(prev) = names.get(name.as_str()) {
                        diagnostics.push(name_conflict(
                            name.as_str(),
                            context.into(),
                            prev.context().into(),
                        ));
                        continue;
                    }

                    names.insert(
                        name.as_str().to_string(),
                        ScopedName {
                            context,
                            node: decl.syntax().green().into(),
                            implicit: false,
                        },
                    );
                }
                v1::TaskItem::Input(_)
                | v1::TaskItem::Output(_)
                | v1::TaskItem::Command(_)
                | v1::TaskItem::Requirements(_)
                | v1::TaskItem::Hints(_)
                | v1::TaskItem::Runtime(_)
                | v1::TaskItem::Metadata(_)
                | v1::TaskItem::ParameterMetadata(_) => continue,
            }
        }

        let span = Self::scope_span(task.syntax());
        let (index, _) = self.tasks.insert_full(
            name.as_str().to_string(),
            TaskScope {
                name_span: name.span(),
                scope: Scope {
                    span,
                    node: task.syntax().green().into(),
                    names,
                    children: Default::default(),
                },
            },
        );

        self.scopes.push((span, ScopeContext::Task(index)));
    }

    /// Adds a workflow scope to the document scope.
    fn add_workflow_scope(
        &mut self,
        workflow: &v1::WorkflowDefinition,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Check for conflicts with task names or an existing workspace
        let name = workflow.name();
        if let Some(s) = self.tasks.get(name.as_str()) {
            diagnostics.push(name_conflict(
                name.as_str(),
                NameContext::Workflow(name.span()),
                NameContext::Task(s.name_span),
            ));
            return;
        } else if let Some(s) = &self.workflow {
            diagnostics.push(duplicate_workflow(&name, s.name_span));
            return;
        }

        // First populate the "root" scope
        let mut scopes = vec![Scope {
            span: Self::scope_span(workflow.syntax()),
            node: workflow.syntax().green().into(),
            names: Default::default(),
            children: Default::default(),
        }];

        let mut saw_input = false;
        let mut saw_output = false;
        for item in workflow.items() {
            match item {
                v1::WorkflowItem::Input(section) if !saw_input => {
                    saw_input = true;
                    let scope = scopes.last_mut().unwrap();
                    Self::add_inputs(&mut scope.names, &section, diagnostics);
                }
                v1::WorkflowItem::Output(section) if !saw_output => {
                    saw_output = true;
                    let scope = scopes.last_mut().unwrap();
                    Self::add_outputs(&mut scope.names, &section, diagnostics);
                }
                v1::WorkflowItem::Declaration(decl) => {
                    Self::add_workflow_statement_decls(
                        &WorkflowStatement::Declaration(decl),
                        &mut scopes,
                        diagnostics,
                    );
                }
                v1::WorkflowItem::Conditional(stmt) => {
                    Self::add_workflow_statement_decls(
                        &WorkflowStatement::Conditional(stmt),
                        &mut scopes,
                        diagnostics,
                    );
                }
                v1::WorkflowItem::Scatter(stmt) => {
                    Self::add_workflow_statement_decls(
                        &WorkflowStatement::Scatter(stmt),
                        &mut scopes,
                        diagnostics,
                    );
                }
                v1::WorkflowItem::Call(stmt) => {
                    Self::add_workflow_statement_decls(
                        &WorkflowStatement::Call(stmt),
                        &mut scopes,
                        diagnostics,
                    );
                }
                v1::WorkflowItem::Input(_)
                | v1::WorkflowItem::Output(_)
                | v1::WorkflowItem::Metadata(_)
                | v1::WorkflowItem::ParameterMetadata(_)
                | v1::WorkflowItem::Hints(_) => continue,
            }
        }

        let scope = scopes.pop().unwrap();
        let span = scope.span;
        self.workflow = Some(WorkflowScope {
            name_span: name.span(),
            name: name.as_str().to_string(),
            scope,
        });
        self.scopes.push((span, ScopeContext::Workflow));
    }

    /// Adds declarations from workflow statements.
    fn add_workflow_statement_decls(
        stmt: &v1::WorkflowStatement,
        scopes: &mut Vec<Scope>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        /// Finds a name by walking up the scope stack
        fn find_name<'a>(name: &str, scopes: &'a [Scope]) -> Option<&'a ScopedName> {
            for scope in scopes.iter().rev() {
                if let Some(name) = scope.names.get(name) {
                    return Some(name);
                }
            }

            None
        }

        match stmt {
            WorkflowStatement::Conditional(stmt) => {
                scopes.push(Scope {
                    span: Self::scope_span(stmt.syntax()),
                    node: stmt.syntax().green().into(),
                    names: Default::default(),
                    children: Default::default(),
                });

                for stmt in stmt.statements() {
                    Self::add_workflow_statement_decls(&stmt, scopes, diagnostics);
                }

                let scope = scopes.pop().unwrap();
                let parent = scopes.last_mut().unwrap();
                for (name, descendant) in &scope.names {
                    parent.names.insert(
                        name.clone(),
                        ScopedName {
                            context: descendant.context,
                            node: descendant.node.clone(),
                            implicit: true,
                        },
                    );
                }

                parent.children.push(scope);
            }
            WorkflowStatement::Scatter(stmt) => {
                let variable = stmt.variable();
                let context = ScopedNameContext::ScatterVariable(variable.span());
                let mut names = IndexMap::new();
                if let Some(prev) = find_name(variable.as_str(), scopes) {
                    diagnostics.push(name_conflict(
                        variable.as_str(),
                        context.into(),
                        prev.context().into(),
                    ));
                } else {
                    names.insert(
                        variable.as_str().to_string(),
                        ScopedName {
                            context,
                            node: stmt.syntax().green().into(),
                            implicit: false,
                        },
                    );
                }

                scopes.push(Scope {
                    span: Self::scope_span(stmt.syntax()),
                    node: stmt.syntax().green().into(),
                    names,
                    children: Default::default(),
                });

                for stmt in stmt.statements() {
                    Self::add_workflow_statement_decls(&stmt, scopes, diagnostics);
                }

                let scope = scopes.pop().unwrap();
                let parent = scopes.last_mut().unwrap();
                for (name, descendant) in &scope.names {
                    // Don't add an implicit name to the parent for the scatter variable
                    if descendant.is_scatter_variable() {
                        continue;
                    }

                    parent.names.insert(
                        name.clone(),
                        ScopedName {
                            context: descendant.context,
                            node: descendant.node.clone(),
                            implicit: true,
                        },
                    );
                }

                parent.children.push(scope);
            }
            WorkflowStatement::Call(stmt) => {
                let name = stmt.alias().map(|a| a.name()).unwrap_or_else(|| {
                    stmt.target()
                        .names()
                        .last()
                        .expect("expected a last call target name")
                });
                if let Some(prev) = find_name(name.as_str(), scopes) {
                    diagnostics.push(call_conflict(
                        &name,
                        prev.context().into(),
                        stmt.alias().is_none(),
                    ));

                    // Define the name in this scope if it conflicted with a scatter variable
                    if !prev.is_scatter_variable() {
                        return;
                    }
                }

                scopes.last_mut().unwrap().names.insert(
                    name.as_str().to_string(),
                    ScopedName {
                        context: ScopedNameContext::Call(name.span()),
                        node: stmt.syntax().green().into(),
                        implicit: false,
                    },
                );
            }
            WorkflowStatement::Declaration(decl) => {
                let name = decl.name();
                let context = ScopedNameContext::Decl(name.span());
                if let Some(prev) = find_name(name.as_str(), scopes) {
                    diagnostics.push(name_conflict(
                        name.as_str(),
                        context.into(),
                        prev.context().into(),
                    ));

                    // Define the name in this scope if it conflicted with a scatter variable
                    if !prev.is_scatter_variable() {
                        return;
                    }
                }

                scopes.last_mut().unwrap().names.insert(
                    name.as_str().to_string(),
                    ScopedName {
                        context,
                        node: decl.syntax().green().into(),
                        implicit: false,
                    },
                );
            }
        }
    }

    /// Resolves an import to its document scope.
    fn resolve_import(
        graph: &DocumentGraph,
        stmt: &v1::ImportStatement,
        importer_index: NodeIndex,
        importer_version: &Version,
    ) -> Result<(Arc<Url>, Arc<DocumentScope>), Diagnostic> {
        let uri = stmt.uri();
        let span = uri.syntax().text_range().to_span();
        let text = match uri.text() {
            Some(text) => text,
            None => {
                if uri.is_empty() {
                    return Err(empty_import(span));
                }

                let span = uri
                    .parts()
                    .find_map(|p| match p {
                        StringPart::Text(_) => None,
                        StringPart::Placeholder(p) => Some(p),
                    })
                    .expect("should contain a placeholder")
                    .syntax()
                    .text_range()
                    .to_span();
                return Err(placeholder_in_import(span));
            }
        };

        let uri = match graph.get(importer_index).uri().join(text.as_str()) {
            Ok(uri) => uri,
            Err(e) => return Err(invalid_relative_import(&e, span)),
        };

        let import_index = graph.get_index(&uri).expect("missing import node in graph");
        let import_node = graph.get(import_index);

        // Check for an import cycle to report
        if graph.contains_cycle(importer_index, import_index) {
            return Err(import_cycle(span));
        }

        // Check for a failure to load the import
        if let ParseState::Error(e) = import_node.parse_state() {
            return Err(import_failure(text.as_str(), e, span));
        }

        // Ensure the import has a matching WDL version
        let import_document = import_node.document().expect("import should have parsed");
        let import_scope = import_node
            .analysis()
            .map(|a| a.scope().clone())
            .expect("import should have been analyzed");

        // Check for compatible imports
        match import_document.version_statement() {
            Some(stmt) => {
                let our_version = stmt.version();
                if matches!((our_version.as_str().split('.').next(), importer_version.as_str().split('.').next()), (Some(our_major), Some(their_major)) if our_major != their_major)
                {
                    return Err(incompatible_import(
                        our_version.as_str(),
                        span,
                        importer_version,
                    ));
                }
            }
            None => {
                return Err(import_missing_version(span));
            }
        }

        Ok((import_node.uri().clone(), import_scope))
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
