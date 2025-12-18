//! Representation of analyzed WDL documents.

use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::hash_map::Entry;
use std::path::Path;
use std::sync::Arc;

use indexmap::IndexMap;
use petgraph::graph::NodeIndex;
use rowan::GreenNode;
use url::Url;
use uuid::Uuid;
use wdl_ast::Ast;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Severity;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxNode;

use crate::config::Config;
use crate::diagnostics::no_common_type;
use crate::diagnostics::unused_import;
use crate::graph::DocumentGraph;
use crate::graph::ParseState;
use crate::types::CallType;
use crate::types::Optional;
use crate::types::Type;

pub mod v1;

/// The `task` variable name available in task command sections and outputs in
/// WDL 1.2.
pub const TASK_VAR_NAME: &str = "task";

/// Represents a namespace introduced by an import.
#[derive(Debug)]
pub struct Namespace {
    /// The span of the import that introduced the namespace.
    span: Span,
    /// The URI of the imported document that introduced the namespace.
    source: Arc<Url>,
    /// The namespace's document.
    document: Document,
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
    /// The name of the struct.
    name: String,
    /// The span that introduced the struct.
    ///
    /// This is either the name of a struct definition (local) or an import's
    /// URI or alias (imported).
    name_span: Span,
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
    /// Gets the name of the struct.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Gets the span of the name.
    pub fn name_span(&self) -> Span {
        self.name_span
    }

    /// Gets the offset of the struct
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Gets the node of the struct.
    pub fn node(&self) -> &rowan::GreenNode {
        &self.node
    }

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
    pub fn ty(&self) -> Option<&Type> {
        self.ty.as_ref()
    }
}

/// Represents an enum in a document.
#[derive(Debug, Clone)]
pub struct Enum {
    /// The name of the enum.
    name: String,
    /// The span that introduced the enum.
    ///
    /// This is either the name of an enum definition (local) or an import's
    /// URI or alias (imported).
    name_span: Span,
    /// The offset of the CST node from the start of the document.
    ///
    /// This is used to adjust diagnostics resulting from traversing the enum
    /// node as if it were the root of the CST.
    offset: usize,
    /// Stores the CST node of the enum.
    ///
    /// This is used to calculate type equivalence for imports and can be
    /// reconstructed into an AST node to access variant expressions.
    node: rowan::GreenNode,
    /// The namespace that defines the enum.
    ///
    /// This is `Some` only for imported enums.
    namespace: Option<String>,
    /// The type of the enum.
    ///
    /// Initially this is `None` until a type check/coercion occurs.
    ty: Option<Type>,
}

impl Enum {
    /// Gets the name of the enum.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Gets the span of the name.
    pub fn name_span(&self) -> Span {
        self.name_span
    }

    /// Gets the offset of the enum.
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Gets the green node of the enum.
    pub fn node(&self) -> &rowan::GreenNode {
        &self.node
    }

    /// Reconstructs the AST definition from the stored green node.
    ///
    /// This provides access to variant expressions and other AST details.
    pub fn definition(&self) -> wdl_ast::v1::EnumDefinition {
        wdl_ast::v1::EnumDefinition::cast(wdl_ast::SyntaxNode::new_root(self.node.clone()))
            .expect("stored node should be a valid enum definition")
    }

    /// Gets the namespace that defines this enum.
    pub fn namespace(&self) -> Option<&str> {
        self.namespace.as_deref()
    }

    /// Gets the type of the enum.
    pub fn ty(&self) -> Option<&Type> {
        self.ty.as_ref()
    }
}

/// Represents information about a name in a scope.
#[derive(Debug, Clone)]
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
    pub fn ty(&self) -> &Type {
        &self.ty
    }
}

/// Represents an index of a scope in a collection of scopes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeIndex(usize);

/// Represents a scope in a WDL document.
#[derive(Debug)]
pub struct Scope {
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
}

impl<'a> ScopeRef<'a> {
    /// Creates a new scope reference given the scope index.
    fn new(scopes: &'a [Scope], index: ScopeIndex) -> Self {
        Self { scopes, index }
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
        })
    }

    /// Gets all of the names available at this scope.
    pub fn names(&self) -> impl Iterator<Item = (&str, &Name)> + use<'_> {
        self.scopes[self.index.0]
            .names
            .iter()
            .map(|(name, n)| (name.as_str(), n))
    }

    /// Gets a name local to this scope.
    ///
    /// Returns `None` if a name local to this scope was not found.
    pub fn local(&self, name: &str) -> Option<&Name> {
        self.scopes[self.index.0].names.get(name)
    }

    /// Lookups a name in the scope.
    ///
    /// Returns `None` if the name is not available in the scope.
    pub fn lookup(&self, name: &str) -> Option<&Name> {
        let mut current = Some(self.index);

        while let Some(index) = current {
            if let Some(name) = self.scopes[index.0].names.get(name) {
                return Some(name);
            }

            current = self.scopes[index.0].parent;
        }

        None
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
    pub fn lookup(&self, name: &str) -> Option<&Name> {
        let mut current = Some(self.index);

        while let Some(index) = current {
            if let Some(name) = self.scopes[index.0].names.get(name) {
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
    pub fn as_scope_ref(&'a self) -> ScopeRef<'a> {
        ScopeRef {
            scopes: self.scopes,
            index: self.index,
        }
    }
}

/// A scope union takes the union of names within a number of given scopes and
/// computes a set of common output names for a presumed parent scope. This is
/// useful when calculating common elements from, for example, an `if`
/// statement within a workflow.
#[derive(Debug)]
pub struct ScopeUnion<'a> {
    /// The scope references to process.
    scope_refs: Vec<(ScopeRef<'a>, bool)>,
}

impl<'a> ScopeUnion<'a> {
    /// Creates a new scope union.
    pub fn new() -> Self {
        Self {
            scope_refs: Vec::new(),
        }
    }

    /// Adds a scope to the union.
    pub fn insert(&mut self, scope_ref: ScopeRef<'a>, exhaustive: bool) {
        self.scope_refs.push((scope_ref, exhaustive));
    }

    /// Resolves the scope union to names and types that should be accessible
    /// from the parent scope.
    ///
    /// Returns an error if any issues are encountered during resolving.
    pub fn resolve(self) -> Result<HashMap<String, Name>, Vec<Diagnostic>> {
        let mut errors = Vec::new();
        let mut ignored: HashSet<String> = HashSet::new();

        // Gather all declaration names and reconcile types
        let mut names: HashMap<String, Name> = HashMap::new();
        for (scope_ref, _) in &self.scope_refs {
            for (name, info) in scope_ref.names() {
                if ignored.contains(name) {
                    continue;
                }

                match names.entry(name.to_string()) {
                    Entry::Vacant(entry) => {
                        entry.insert(info.clone());
                    }
                    Entry::Occupied(mut entry) => {
                        let Some(ty) = entry.get().ty.common_type(&info.ty) else {
                            errors.push(no_common_type(
                                &entry.get().ty,
                                entry.get().span,
                                &info.ty,
                                info.span,
                            ));
                            names.remove(name);
                            ignored.insert(name.to_string());
                            continue;
                        };

                        entry.get_mut().ty = ty;
                    }
                }
            }
        }

        // Mark types as optional if not present in all clauses
        for (scope_ref, _) in &self.scope_refs {
            for (name, info) in &mut names {
                if ignored.contains(name) {
                    continue;
                }

                // If this name is not in the current clause's scope, mark as optional
                if scope_ref.local(name).is_none() {
                    info.ty = info.ty.optional();
                }
            }
        }

        // If there's no `else` clause, mark all types as optional
        let has_exhaustive = self.scope_refs.iter().any(|(_, exhaustive)| *exhaustive);
        if !has_exhaustive {
            for info in names.values_mut() {
                info.ty = info.ty.optional();
            }
        }

        if !errors.is_empty() {
            return Err(errors);
        }

        Ok(names)
    }
}

/// Represents a task or workflow input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    /// The type of the input.
    ty: Type,
    /// Whether or not the input is required.
    ///
    /// A required input is one that has a non-optional type and no default
    /// expression.
    required: bool,
}

impl Input {
    /// Gets the type of the input.
    pub fn ty(&self) -> &Type {
        &self.ty
    }

    /// Whether or not the input is required.
    pub fn required(&self) -> bool {
        self.required
    }
}

/// Represents a task or workflow output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// The type of the output.
    ty: Type,
    /// The span of the output name.
    name_span: Span,
}

impl Output {
    /// Creates a new output with the given type.
    pub(crate) fn new(ty: Type, name_span: Span) -> Self {
        Self { ty, name_span }
    }

    /// Gets the type of the output.
    pub fn ty(&self) -> &Type {
        &self.ty
    }

    /// Gets the span of output's name.
    pub fn name_span(&self) -> Span {
        self.name_span
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
    inputs: Arc<IndexMap<String, Input>>,
    /// The outputs of the task.
    outputs: Arc<IndexMap<String, Output>>,
}

impl Task {
    /// Gets the name of the task.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Gets the span of the name.
    pub fn name_span(&self) -> Span {
        self.name_span
    }

    /// Gets the scope of the task.
    pub fn scope(&self) -> ScopeRef<'_> {
        ScopeRef::new(&self.scopes, ScopeIndex(0))
    }

    /// Gets the inputs of the task.
    pub fn inputs(&self) -> &IndexMap<String, Input> {
        &self.inputs
    }

    /// Gets the outputs of the task.
    pub fn outputs(&self) -> &IndexMap<String, Output> {
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
    inputs: Arc<IndexMap<String, Input>>,
    /// The outputs of the workflow.
    outputs: Arc<IndexMap<String, Output>>,
    /// The calls made by the workflow.
    calls: HashMap<String, CallType>,
    /// Whether or not nested inputs are allowed for the workflow.
    allows_nested_inputs: bool,
}

impl Workflow {
    /// Gets the name of the workflow.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Gets the span of the name.
    pub fn name_span(&self) -> Span {
        self.name_span
    }

    /// Gets the scope of the workflow.
    pub fn scope(&self) -> ScopeRef<'_> {
        ScopeRef::new(&self.scopes, ScopeIndex(0))
    }

    /// Gets the inputs of the workflow.
    pub fn inputs(&self) -> &IndexMap<String, Input> {
        &self.inputs
    }

    /// Gets the outputs of the workflow.
    pub fn outputs(&self) -> &IndexMap<String, Output> {
        &self.outputs
    }

    /// Gets the calls made by the workflow.
    pub fn calls(&self) -> &HashMap<String, CallType> {
        &self.calls
    }

    /// Determines if the workflow allows nested inputs.
    pub fn allows_nested_inputs(&self) -> bool {
        self.allows_nested_inputs
    }
}

/// Represents analysis data about a WDL document.
#[derive(Debug)]
pub(crate) struct DocumentData {
    /// The configuration under which this document was analyzed.
    config: Config,
    /// The root CST node of the document.
    ///
    /// This is `None` when the document could not be parsed.
    root: Option<GreenNode>,
    /// The document identifier.
    ///
    /// The identifier changes every time the document is analyzed.
    id: Arc<String>,
    /// The URI of the analyzed document.
    uri: Arc<Url>,
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
    /// The enums in the document.
    enums: IndexMap<String, Enum>,
    /// The diagnostics from parsing.
    parse_diagnostics: Vec<Diagnostic>,
    /// The diagnostics from analysis.
    analysis_diagnostics: Vec<Diagnostic>,
}

impl DocumentData {
    /// Constructs a new analysis document data.
    fn new(
        config: Config,
        uri: Arc<Url>,
        root: Option<GreenNode>,
        version: Option<SupportedVersion>,
        diagnostics: Vec<Diagnostic>,
    ) -> Self {
        Self {
            config,
            root,
            id: Uuid::new_v4().to_string().into(),
            uri,
            version,
            namespaces: Default::default(),
            tasks: Default::default(),
            workflow: Default::default(),
            structs: Default::default(),
            enums: Default::default(),
            parse_diagnostics: diagnostics,
            analysis_diagnostics: Default::default(),
        }
    }
}

/// Represents an analyzed WDL document.
///
/// This type is cheaply cloned.
#[derive(Debug, Clone)]
pub struct Document {
    /// The document data for the document.
    data: Arc<DocumentData>,
}

impl Document {
    /// Creates a new default document from a URI.
    pub(crate) fn default_from_uri(uri: Arc<Url>) -> Self {
        Self {
            data: Arc::new(DocumentData::new(
                Default::default(),
                uri,
                None,
                None,
                Default::default(),
            )),
        }
    }

    /// Creates a new analyzed document from a document graph node.
    pub(crate) fn from_graph_node(
        config: &Config,
        graph: &DocumentGraph,
        index: NodeIndex,
    ) -> Self {
        let node = graph.get(index);

        let (wdl_version, diagnostics) = match node.parse_state() {
            ParseState::NotParsed => panic!("node should have been parsed"),
            ParseState::Error(_) => return Self::default_from_uri(node.uri().clone()),
            ParseState::Parsed {
                wdl_version,
                diagnostics,
                ..
            } => (wdl_version, diagnostics),
        };

        let root = node.root().expect("node should have been parsed");
        let (config, wdl_version) = match (root.version_statement(), wdl_version) {
            (Some(stmt), Some(wdl_version)) => (
                config.with_diagnostics_config(
                    config.diagnostics_config().excepted_for_node(stmt.inner()),
                ),
                *wdl_version,
            ),
            _ => {
                // Don't process a document with a missing version statement or an unsupported
                // version unless a fallback version is configured
                return Self {
                    data: Arc::new(DocumentData::new(
                        config.clone(),
                        node.uri().clone(),
                        Some(root.inner().green().into()),
                        None,
                        diagnostics.to_vec(),
                    )),
                };
            }
        };

        let mut data = DocumentData::new(
            config.clone(),
            node.uri().clone(),
            Some(root.inner().green().into()),
            Some(wdl_version),
            diagnostics.to_vec(),
        );
        match root.ast_with_version_fallback(config.fallback_version()) {
            Ast::Unsupported => {}
            Ast::V1(ast) => v1::populate_document(&mut data, &config, graph, index, &ast),
        }

        // Check for unused imports
        if let Some(severity) = config.diagnostics_config().unused_import {
            let DocumentData {
                namespaces,
                analysis_diagnostics,
                ..
            } = &mut data;

            analysis_diagnostics.extend(
                namespaces
                    .iter()
                    .filter(|(_, ns)| !ns.used && !ns.excepted)
                    .map(|(name, ns)| unused_import(name, ns.span()).with_severity(severity)),
            );
        }

        Self {
            data: Arc::new(data),
        }
    }

    /// Gets the analysis configuration.
    pub fn config(&self) -> &Config {
        &self.data.config
    }

    /// Gets the root AST document node.
    ///
    /// # Panics
    ///
    /// Panics if the document was not parsed.
    pub fn root(&self) -> wdl_ast::Document {
        wdl_ast::Document::cast(SyntaxNode::new_root(
            self.data.root.clone().expect("should have a root"),
        ))
        .expect("should cast")
    }

    /// Gets the identifier of the document.
    ///
    /// This value changes when a document is reanalyzed.
    pub fn id(&self) -> &Arc<String> {
        &self.data.id
    }

    /// Gets the URI of the document.
    pub fn uri(&self) -> &Arc<Url> {
        &self.data.uri
    }

    /// Gets the path to the document.
    ///
    /// If the scheme of the document's URI is not `file`, this will return the
    /// URI as a string. Otherwise, this will attempt to return the path
    /// relative to the current working directory, or the absolute path
    /// failing that.
    pub fn path(&self) -> Cow<'_, str> {
        if let Ok(path) = self.data.uri.to_file_path() {
            if let Some(path) = std::env::current_dir()
                .ok()
                .and_then(|cwd| path.strip_prefix(cwd).ok().and_then(Path::to_str))
            {
                return path.to_string().into();
            }

            if let Ok(path) = path.into_os_string().into_string() {
                return path.into();
            }
        }

        self.data.uri.as_str().into()
    }

    /// Gets the supported version of the document.
    ///
    /// Returns `None` if the document could not be parsed or contains an
    /// unsupported version.
    pub fn version(&self) -> Option<SupportedVersion> {
        self.data.version
    }

    /// Gets the namespaces in the document.
    pub fn namespaces(&self) -> impl Iterator<Item = (&str, &Namespace)> {
        self.data.namespaces.iter().map(|(n, ns)| (n.as_str(), ns))
    }

    /// Gets a namespace in the document by name.
    pub fn namespace(&self, name: &str) -> Option<&Namespace> {
        self.data.namespaces.get(name)
    }

    /// Gets the tasks in the document.
    pub fn tasks(&self) -> impl Iterator<Item = &Task> {
        self.data.tasks.iter().map(|(_, t)| t)
    }

    /// Gets a task in the document by name.
    pub fn task_by_name(&self, name: &str) -> Option<&Task> {
        self.data.tasks.get(name)
    }

    /// Gets a workflow in the document.
    ///
    /// Returns `None` if the document did not contain a workflow.
    pub fn workflow(&self) -> Option<&Workflow> {
        self.data.workflow.as_ref()
    }

    /// Gets the structs in the document.
    pub fn structs(&self) -> impl Iterator<Item = (&str, &Struct)> {
        self.data.structs.iter().map(|(n, s)| (n.as_str(), s))
    }

    /// Gets a struct in the document by name.
    pub fn struct_by_name(&self, name: &str) -> Option<&Struct> {
        self.data.structs.get(name)
    }

    /// Gets the enums in the document.
    pub fn enums(&self) -> impl Iterator<Item = (&str, &Enum)> {
        self.data.enums.iter().map(|(n, e)| (n.as_str(), e))
    }

    /// Gets an enum in the document by name.
    pub fn enum_by_name(&self, name: &str) -> Option<&Enum> {
        self.data.enums.get(name)
    }

    /// Gets the parse diagnostics for the document.
    pub fn parse_diagnostics(&self) -> &[Diagnostic] {
        &self.data.parse_diagnostics
    }

    /// Gets the analysis diagnostics for the document.
    pub fn analysis_diagnostics(&self) -> &[Diagnostic] {
        &self.data.analysis_diagnostics
    }

    /// Gets all diagnostics for the document (both from parsing and analysis).
    pub fn diagnostics(&self) -> impl Iterator<Item = &Diagnostic> {
        self.data
            .parse_diagnostics
            .iter()
            .chain(self.data.analysis_diagnostics.iter())
    }

    /// Sorts the diagnostics for the document.
    ///
    /// # Panics
    ///
    /// Panics if there is more than one reference to the document.
    pub fn sort_diagnostics(&mut self) -> Self {
        let data = &mut self.data;
        let inner = Arc::get_mut(data).expect("should only have one reference");
        inner.parse_diagnostics.sort();
        inner.analysis_diagnostics.sort();
        Self { data: data.clone() }
    }

    /// Extends the analysis diagnostics for the document.
    ///
    /// # Panics
    ///
    /// Panics if there is more than one reference to the document.
    pub fn extend_diagnostics(&mut self, diagnostics: Vec<Diagnostic>) -> Self {
        let data = &mut self.data;
        let inner = Arc::get_mut(data).expect("should only have one reference");
        inner.analysis_diagnostics.extend(diagnostics);
        Self { data: data.clone() }
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
        if let Some(workflow) = &self.data.workflow
            && workflow.scope().span().contains(position)
        {
            return find_scope(&workflow.scopes, position);
        }

        // Search for a task that might contain the position
        let task = match self
            .data
            .tasks
            .binary_search_by_key(&position, |_, t| t.scope().span().start())
        {
            Ok(index) => &self.data.tasks[index],
            Err(index) => {
                // This indicates that we couldn't find a match and the match would go _before_
                // the first task, so there is no containing task.
                if index == 0 {
                    return None;
                }

                &self.data.tasks[index - 1]
            }
        };

        if task.scope().span().contains(position) {
            return find_scope(&task.scopes, position);
        }

        None
    }

    /// Determines if the document, or any documents transitively imported by
    /// this document, has errors.
    ///
    /// Returns `true` if the document, or one of its transitive imports, has at
    /// least one error diagnostic.
    ///
    /// Returns `false` if the document, and all of its transitive imports, have
    /// no error diagnostics.
    pub fn has_errors(&self) -> bool {
        // Check this document for errors
        if self.diagnostics().any(|d| d.severity() == Severity::Error) {
            return true;
        }

        // Check every imported document for errors
        for (_, ns) in self.namespaces() {
            if ns.document.has_errors() {
                return true;
            }
        }

        false
    }

    /// Visits the document with a pre-order traversal using the provided
    /// visitor to visit each element in the document.
    pub fn visit<V: crate::Visitor>(&self, diagnostics: &mut crate::Diagnostics, visitor: &mut V) {
        crate::visit(self, diagnostics, visitor)
    }
}
