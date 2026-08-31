//! Representation of analyzed WDL documents.

use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::hash_map::Entry;
use std::path::Path;
use std::sync::Arc;

use arrayvec::ArrayString;
use indexmap::IndexMap;
use indexmap::IndexSet;
use itertools::Itertools;
use petgraph::graph::NodeIndex;
use rowan::GreenNode;
use rowan::TextRange;
use rowan::TextSize;
use url::Url;
use uuid::Uuid;
use wdl_ast::Ast;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Severity;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxNode;

use crate::AnalysisCache;
use crate::Diagnostics;
use crate::EnumRef;
use crate::StructRef;
use crate::TaskRef;
use crate::WorkflowRef;
use crate::config::Config;
use crate::diagnostics::Context;
use crate::diagnostics::no_common_type;
use crate::graph::DocumentGraph;
use crate::graph::ParseState;
use crate::types::CallType;
use crate::types::EnumChoiceCacheKey;
use crate::types::Optional;
use crate::types::Type;

pub mod cache;
pub mod v1;

/// The `task` variable name available in task command sections and outputs in
/// WDL 1.2.
pub const TASK_VAR_NAME: &str = "task";

/// A successfully resolved namespace introduced by an import.
#[derive(Debug, Clone, PartialEq)]
pub struct Namespace {
    /// The name of the namespace.
    name: String,
    /// The span of the import that introduced the namespace.
    pub(crate) span: Span,
    /// The URI of the imported document that introduced the namespace.
    source: Arc<Url>,
    /// The namespace's document.
    document: Document,
    /// Whether or not the namespace is used (i.e. referenced) in the document.
    pub(crate) used: bool,
    /// Structs imported from this namespace, keyed by their local name.
    ///
    /// NOTE: While this is separated from the [`Document`], imported
    /// structs/enums are copied into the document's scope and should
    /// be treated as though they were defined in the document.
    pub(in crate::document) imported_structs: IndexMap<String, ImportedStruct>,
    /// Enums imported from this namespace, keyed by their local name.
    pub(in crate::document) imported_enums: IndexMap<String, ImportedEnum>,
}

impl Namespace {
    /// Gets the name of the namespace.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Gets the span of the import that introduced the namespace.
    pub fn span(&self) -> Span {
        self.span
    }

    /// Gets the URI of the imported document that introduced the namespace.
    pub fn source(&self) -> Arc<Url> {
        self.source.clone()
    }

    /// Gets the imported document.
    pub fn document(&self) -> &Document {
        &self.document
    }
}

/// Represents a struct in a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Struct {
    /// The name of the struct.
    name: String,
    /// The span that introduced the struct.
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

    /// Reconstructs the AST definition from the stored green node.
    pub fn definition(&self) -> wdl_ast::v1::StructDefinition {
        wdl_ast::v1::StructDefinition::cast(wdl_ast::SyntaxNode::new_root(self.node.clone()))
            .expect("stored node should be a valid struct definition")
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enum {
    /// The name of the enum.
    name: String,
    /// The span that introduced the enum.
    name_span: Span,
    /// The offset of the CST node from the start of the document.
    ///
    /// This is used to adjust diagnostics resulting from traversing the enum
    /// node as if it were the root of the CST.
    offset: usize,
    /// Stores the CST node of the enum.
    ///
    /// This is used to calculate type equivalence for imports and can be
    /// reconstructed into an AST node to access choice expressions.
    node: rowan::GreenNode,
    /// The type of the enum.
    ///
    /// Initially this is `None` until types are populated for the document.
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
    /// This provides access to choice expressions and other AST details.
    pub fn definition(&self) -> wdl_ast::v1::EnumDefinition {
        wdl_ast::v1::EnumDefinition::cast(wdl_ast::SyntaxNode::new_root(self.node.clone()))
            .expect("stored node should be a valid enum definition")
    }

    /// Gets the type of the enum.
    pub fn ty(&self) -> Option<&Type> {
        self.ty.as_ref()
    }
}

/// Represents information about a name in a scope.
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
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

                // If this name is not in the current clause's scope, mark as
                // optional
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    /// The span of the task name.
    pub(in crate::document) name_span: Span,
    /// The name of the task.
    pub(in crate::document) name: String,
    /// The span of the task definition.
    pub(in crate::document) span: Span,
    /// The scopes contained in the task.
    ///
    /// The first scope will always be the task's scope.
    ///
    /// The scopes will be in sorted order by span start.
    pub(in crate::document) scopes: Vec<Scope>,
    /// The inputs of the task.
    pub(in crate::document) inputs: Arc<IndexMap<String, Input>>,
    /// The outputs of the task.
    pub(in crate::document) outputs: Arc<IndexMap<String, Output>>,
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

    /// Gets the span of the workflow definition.
    pub fn span(&self) -> Span {
        self.span
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workflow {
    /// The span of the workflow name.
    pub(in crate::document) name_span: Span,
    /// The name of the workflow.
    pub(in crate::document) name: String,
    /// The span of the workflow definition.
    pub(in crate::document) span: Span,
    /// The scopes contained in the workflow.
    ///
    /// The first scope will always be the workflow's scope.
    ///
    /// The scopes will be in sorted order by span start.
    pub(in crate::document) scopes: Vec<Scope>,
    /// The inputs of the workflow.
    pub(in crate::document) inputs: Arc<IndexMap<String, Input>>,
    /// The outputs of the workflow.
    pub(in crate::document) outputs: Arc<IndexMap<String, Output>>,
    /// The calls made by the workflow.
    pub(in crate::document) calls: HashMap<String, CallType>,
    /// Whether or not nested inputs are allowed for the workflow.
    pub(in crate::document) allows_nested_inputs: bool,
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

    /// Gets the span of the workflow definition.
    pub fn span(&self) -> Span {
        self.span
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

/// A struct imported into scope.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedStruct {
    /// The aliased name of the struct in the dependent document.
    pub local_name: String,
    /// The offset of the CST node from the start of the document.
    ///
    /// This is used to adjust diagnostics resulting from traversing the struct
    /// node as if it were the root of the CST.
    offset: usize,
    /// Stores the CST node of the struct.
    ///
    /// This is used to calculate type equivalence for imports.
    node: rowan::GreenNode,
    /// The span of the import statement that introduced this struct.
    pub span: Span,
    /// The source document that defines the struct.
    pub document: Document,
    /// The type of the struct.
    ///
    /// Initially this is `None` until a type check/coercion occurs.
    ty: Option<Type>,
}

impl ImportedStruct {
    /// Gets the node of the struct.
    pub fn node(&self) -> &rowan::GreenNode {
        &self.node
    }

    /// Gets the offset of the struct in the source document's CST.
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Gets the URI of the document this struct was imported from.
    pub fn source(&self) -> Arc<Url> {
        self.document.uri()
    }

    /// Reconstructs the AST definition from the stored green node.
    ///
    /// This provides access to choice expressions and other AST details.
    pub fn definition(&self) -> wdl_ast::v1::StructDefinition {
        wdl_ast::v1::StructDefinition::cast(wdl_ast::SyntaxNode::new_root(self.node.clone()))
            .expect("stored node should be a valid struct definition")
    }

    /// Gets the type of the struct.
    ///
    /// A value of `None` indicates that the type could not be determined for
    /// the struct; this may happen if the struct definition is recursive.
    pub fn ty(&self) -> Option<&Type> {
        self.ty.as_ref()
    }
}

/// An enum imported into scope.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedEnum {
    /// The aliased name of the enum in the dependent document.
    pub local_name: String,
    /// The offset of the CST node from the start of the document.
    ///
    /// This is used to adjust diagnostics resulting from traversing the enum
    /// node as if it were the root of the CST.
    offset: usize,
    /// Stores the CST node of the enum.
    ///
    /// This is used to calculate type equivalence for imports and can be
    /// reconstructed into an AST node to access choice expressions.
    node: rowan::GreenNode,
    /// The span of the import statement.
    pub span: Span,
    /// The source document that defines the enum.
    pub document: Document,
    /// The type of the enum.
    ///
    /// Initially this is `None` until a type check/coercion occurs.
    ty: Option<Type>,
}

impl ImportedEnum {
    /// Gets the node of the enum.
    pub fn node(&self) -> &rowan::GreenNode {
        &self.node
    }

    /// Gets the offset of the enum in the source document's CST.
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Gets the URI of the document this enum was imported from.
    pub fn source(&self) -> Arc<Url> {
        self.document.uri()
    }

    /// Reconstructs the AST definition from the stored green node.
    ///
    /// This provides access to choice expressions and other AST details.
    pub fn definition(&self) -> wdl_ast::v1::EnumDefinition {
        wdl_ast::v1::EnumDefinition::cast(wdl_ast::SyntaxNode::new_root(self.node.clone()))
            .expect("stored node should be a valid enum definition")
    }

    /// Gets the type of the enum.
    pub fn ty(&self) -> Option<&Type> {
        self.ty.as_ref()
    }
}

/// A task imported into scope by a wildcard or selected-member import.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedTask {
    /// The aliased name of the task in the dependent document.
    pub local_name: String,
    /// The task name in the source document.
    pub name: String,
    /// The span of the import statement that introduced this task.
    pub span: Span,
    /// The source document that defines the task.
    pub document: Document,
    /// The inputs of the task.
    pub inputs: Arc<IndexMap<String, Input>>,
    /// The outputs of the task.
    pub outputs: Arc<IndexMap<String, Output>>,
}

impl ImportedTask {
    /// Gets the task name in its source document.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Gets the source document that defines the task.
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// Gets the source URI the task came from.
    pub(crate) fn source(&self) -> Arc<Url> {
        self.document.uri()
    }
}

/// A workflow imported into scope by a wildcard or selected-member import.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedWorkflow {
    /// The aliased name of the workflow in the dependent document.
    pub local_name: String,
    /// The workflow name in the source document.
    pub name: String,
    /// The span of the import statement.
    pub span: Span,
    /// The source document that defines the task.
    pub document: Document,
    /// The inputs of the workflow.
    pub inputs: Arc<IndexMap<String, Input>>,
    /// The outputs of the workflow.
    pub outputs: Arc<IndexMap<String, Output>>,
}

impl ImportedWorkflow {
    /// Gets the workflow name in its source document.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Gets the source document that defines the workflow.
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// Gets the source URI the workflow came from.
    pub(crate) fn source(&self) -> Arc<Url> {
        self.document.uri()
    }
}

/// A callable item.
#[derive(Copy, Clone, Debug)]
pub enum Callable<'a> {
    /// A workflow.
    Workflow(WorkflowRef<'a>),
    /// A task.
    Task(TaskRef<'a>),
}

impl Callable<'_> {
    /// Get the name of this callable.
    pub fn name(&self) -> &str {
        match self {
            Callable::Workflow(w) => w.name(),
            Callable::Task(t) => t.name(),
        }
    }

    /// Get the [`Span`] of the callable's name.
    pub fn name_span(&self) -> Span {
        match self {
            Callable::Workflow(w) => w.name_span(),
            Callable::Task(t) => t.name_span(),
        }
    }

    /// Whether this callable represents a workflow.
    pub fn is_workflow(&self) -> bool {
        matches!(self, Callable::Workflow(_))
    }

    /// Whether this callable represents a task.
    pub fn is_task(&self) -> bool {
        matches!(self, Callable::Task(_))
    }

    /// Get the inputs of the callable.
    pub fn inputs(&self) -> Arc<IndexMap<String, Input>> {
        match self {
            Callable::Workflow(w) => w.inputs(),
            Callable::Task(t) => t.inputs(),
        }
    }

    /// Get the outputs of the callable.
    pub fn outputs(&self) -> Arc<IndexMap<String, Output>> {
        match self {
            Callable::Workflow(w) => w.outputs(),
            Callable::Task(t) => t.outputs(),
        }
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
    /// The names of imports that failed to resolve, keyed by name, each with
    /// the span of the failing import. Kept so that downstream references to
    /// the imported name (e.g., `import spellbook` followed by
    /// `call spellbook.fireball`) don't produce cascading "unknown namespace"
    /// diagnostics.
    failed_imports: IndexMap<String, Span>,
    /// The analysis cache for the document.
    cache: Arc<AnalysisCache>,
    /// Whether a wildcard import failed to resolve.
    ///
    /// Unknown unqualified calls are suppressed in this case because they may
    /// have come from the missing import.
    failed_wildcard_import: bool,
    /// Selected task or workflow imports that failed to resolve.
    failed_selected_imports: IndexSet<String>,
    /// The diagnostics from parsing.
    parse_diagnostics: Vec<Diagnostic>,
    /// The diagnostics from analysis.
    pub(crate) analysis_diagnostics: Diagnostics,
}

impl PartialEq for DocumentData {
    fn eq(&self, other: &Self) -> bool {
        let Self {
            config,
            root,
            id: _,
            uri,
            version,
            failed_imports,
            cache,
            failed_wildcard_import,
            failed_selected_imports,
            parse_diagnostics,
            analysis_diagnostics,
        } = self;

        config == &other.config
            && root == &other.root
            && uri == &other.uri
            && version == &other.version
            && failed_imports == &other.failed_imports
            && cache == &other.cache
            && failed_wildcard_import == &other.failed_wildcard_import
            && failed_selected_imports == &other.failed_selected_imports
            && parse_diagnostics == &other.parse_diagnostics
            && analysis_diagnostics == &other.analysis_diagnostics
    }
}

impl DocumentData {
    /// Constructs a new analysis document data.
    fn new(
        config: Config,
        uri: Arc<Url>,
        root: Option<GreenNode>,
        version: Option<SupportedVersion>,
        parse_diagnostics: Vec<Diagnostic>,
    ) -> Self {
        Self {
            config,
            root,
            id: Uuid::new_v4().to_string().into(),
            uri,
            version,
            failed_imports: Default::default(),
            cache: Default::default(), // Populated
            failed_wildcard_import: false,
            failed_selected_imports: Default::default(),
            parse_diagnostics,
            analysis_diagnostics: Default::default(),
        }
    }

    /// Gets the context of the given name.
    ///
    /// The name may be for a namespace, task, workflow, struct, or enum.
    ///
    /// Returns `None` if there is no context for the given name.
    fn context(&self, cache: &AnalysisCache, name: &str) -> Option<Context> {
        // Look through the various data structures for the name
        if let Some((_hash, ns)) = cache.namespace_by_name(name) {
            Some(Context::Namespace(ns.span))
        } else if let Some(span) = self.failed_imports.get(name) {
            Some(Context::Namespace(*span))
        } else if let Some((_idx, _hash, task)) = cache.local_task_by_name(name) {
            Some(Context::Task(task.name_span()))
        } else if let Some(wf) = cache.workflow().filter(|w| w.name() == name) {
            Some(Context::Workflow(wf.name_span()))
        } else if let Some((_idx, _hash, s)) = cache.local_struct_by_name(name) {
            Some(Context::Struct(s.name_span()))
        } else {
            // Finally, check the enums and failing that return `None`
            cache
                .local_enum_by_name(name)
                .map(|(_idx, _hash, e)| Context::Enum(e.name_span()))
        }
    }
}

/// Represents an analyzed WDL document.
///
/// This type is cheaply cloned.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    /// The document data for the document.
    data: Arc<DocumentData>,
}

impl Document {
    /// Gets the internal document data.
    #[cfg(test)]
    pub(crate) fn data(&self) -> &Arc<DocumentData> {
        &self.data
    }
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
        graph: &mut DocumentGraph,
        index: NodeIndex,
    ) -> Self {
        let node = graph.get_mut(index);
        let (wdl_version, parse_diagnostics, edits) = match node.parse_state() {
            ParseState::NotParsed => panic!("node should have been parsed"),
            ParseState::Error(_) => {
                return Self::default_from_uri(node.uri().clone());
            }
            ParseState::Parsed {
                wdl_version,
                diagnostics,
                edits,
                ..
            } => (*wdl_version, diagnostics.clone(), edits.clone()),
        };

        let root = node.root().expect("node should have been parsed");
        let config = if let Some(stmt) = root.version_statement() {
            config.with_diagnostics_config(
                config.diagnostics_config().excepted_for_node(stmt.inner()),
            )
        } else {
            config.clone()
        };

        let old_cache = node.take_cache();

        let mut data = DocumentData::new(
            config.clone(),
            node.uri().clone(),
            Some(root.inner().green().into()),
            wdl_version,
            parse_diagnostics,
        );

        let _ = node;
        match root.ast_with_version_fallback(config.fallback_version()) {
            Ast::Unsupported => {
                // Don't process a document with a missing version statement or
                // an unsupported version unless a fallback
                // version is configured
            }
            Ast::V1(ast) => {
                v1::populate_document(&mut data, old_cache, &config, graph, index, &ast, &edits)
            }
        };

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
    pub fn uri(&self) -> Arc<Url> {
        self.data.uri.clone()
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

    /// Computes the `blake3` hash of the document's source text over the
    /// given span and returns the hex form.
    ///
    /// Uses `rowan::SyntaxText::for_each_chunk` so the span's text is never
    /// materialized as a `String`.
    ///
    /// Returns `None` if `span` falls outside the document's source text.
    pub fn hash_span(&self, span: Span) -> Option<ArrayString<64>> {
        let text = self.root().inner().text();
        let text_len = usize::from(text.len());
        if span.end() > text_len {
            return None;
        }
        let range = TextRange::new(
            TextSize::new(span.start() as u32),
            TextSize::new(span.end() as u32),
        );
        let slice = text.slice(range);
        let mut hasher = blake3::Hasher::new();
        slice.for_each_chunk(|chunk| {
            hasher.update(chunk.as_bytes());
        });
        Some(hasher.finalize().to_hex())
    }

    /// Gets the supported version of the document.
    ///
    /// Returns `None` if the document could not be parsed or contains an
    /// unsupported version.
    pub fn version(&self) -> Option<SupportedVersion> {
        self.data.version
    }

    /// Gets the analysis cache.
    pub(crate) fn cache(&self) -> Arc<AnalysisCache> {
        self.data.cache.clone()
    }

    /// Gets the successfully resolved namespaces in the document.
    pub fn namespaces(&self) -> impl Iterator<Item = &Namespace> {
        self.data.cache.namespaces().map(|(_, ns)| ns)
    }

    /// Gets a successfully resolved namespace in the document by name.
    pub fn namespace(&self, name: &str) -> Option<&Namespace> {
        self.data.cache.namespace_by_name(name).map(|(_, ns)| ns)
    }

    /// Gets the tasks in the document.
    pub fn tasks(&self) -> impl Iterator<Item = TaskRef<'_>> {
        self.data.cache.tasks()
    }

    /// Gets the tasks in the document.
    pub(crate) fn local_tasks(&self) -> impl Iterator<Item = &Task> {
        self.data.cache.local_tasks().map(|(_, _, task)| task)
    }

    /// Gets a locally defined task by name.
    pub fn local_task_by_name(&self, name: &str) -> Option<&Task> {
        self.data
            .cache
            .local_task_by_name(name)
            .map(|(_idx, _hash, task)| task)
    }

    /// Gets a task in the document by name.
    pub fn task_by_name(&self, name: &str) -> Option<TaskRef<'_>> {
        self.data.cache.task_by_name(name).map(|(_hash, task)| task)
    }

    /// Gets an imported task in the document by local name.
    ///
    /// NOTE: This only includes tasks in the current document's scope (e.g.,
    /// those from select/wildcard imports).
    pub fn imported_task_by_name(&self, name: &str) -> Option<&ImportedTask> {
        self.data.cache.imported_task_by_name(name).map(|(_, t)| t)
    }

    /// Gets a workflow in the document.
    ///
    /// Returns `None` if the document did not contain a workflow.
    pub fn workflow(&self) -> Option<&Workflow> {
        self.data.cache.workflow()
    }

    /// Gets an imported workflow in the document by local name.
    ///
    /// NOTE: This only includes workflows in the current document's scope
    /// (e.g., those from select/wildcard imports).
    pub fn imported_workflow_by_name(&self, name: &str) -> Option<&ImportedWorkflow> {
        self.data
            .cache
            .imported_workflow_by_name(name)
            .map(|(_, w)| w)
    }

    /// Gets a workflow in the document by name.
    pub fn workflow_by_name(&self, name: &str) -> Option<WorkflowRef<'_>> {
        self.data.cache.workflow_by_name(name).map(|(_, w)| w)
    }

    /// Gets a [`Callable`] in the document by name.
    ///
    /// Returns `None` if the document did not contain a callable definition
    /// with the given name.
    ///
    /// NOTE: This includes imports, see also:
    /// [`Self::local_callable_by_name()`].
    pub fn callable_by_name(&self, name: &str) -> Option<Callable<'_>> {
        if let Some(workflow) = self.workflow_by_name(name) {
            return Some(Callable::Workflow(workflow));
        }

        if let Some(task) = self.task_by_name(name) {
            return Some(Callable::Task(task));
        }

        None
    }

    /// Get all callable targets in the document, including imports.
    ///
    /// See also: [`Self::local_callables()`]
    pub fn callables(&self) -> impl Iterator<Item = Callable<'_>> {
        self.local_callables()
            .chain(
                self.data
                    .cache
                    .imported_workflows()
                    .map(|(_hash, w)| Callable::Workflow(WorkflowRef::Imported(w))),
            )
            .chain(
                self.data
                    .cache
                    .imported_tasks()
                    .map(|(_hash, t)| Callable::Task(TaskRef::Imported(t))),
            )
    }

    /// Gets a [`Callable`] in the document by name.
    ///
    /// Returns `None` if the document did not contain a callable definition
    /// with the given name.
    ///
    /// NOTE: Unlike [`Self::callable_by_name()`], this only searches callables
    /// defined in this document.
    pub fn local_callable_by_name(&self, name: &str) -> Option<Callable<'_>> {
        if let Some(workflow) = self.workflow()
            && workflow.name == name
        {
            return Some(Callable::Workflow(WorkflowRef::Local(workflow)));
        }

        if let Some(task) = self.local_task_by_name(name) {
            return Some(Callable::Task(TaskRef::Local(task)));
        }

        None
    }

    /// Get all locally defined callable targets in the document.
    ///
    /// See also: [`Self::callables()`]
    pub fn local_callables(&self) -> impl Iterator<Item = Callable<'_>> {
        self.workflow()
            .map(WorkflowRef::Local)
            .map(Callable::Workflow)
            .into_iter()
            .chain(self.local_tasks().map(TaskRef::Local).map(Callable::Task))
    }

    /// Gets the structs in the document.
    pub fn structs(&self) -> impl Iterator<Item = StructRef<'_>> {
        self.data.cache.structs()
    }

    /// Gets a locally defined struct in the document by name.
    pub fn local_struct_by_name(&self, name: &str) -> Option<&Struct> {
        self.data
            .cache
            .local_struct_by_name(name)
            .map(|(_idx, _hash, s)| s)
    }

    /// Gets an imported struct in the document by local name.
    pub fn imported_struct_by_name(&self, name: &str) -> Option<&ImportedStruct> {
        self.data
            .cache
            .imported_struct_by_name(name)
            .map(|(_hash, s)| s)
    }

    /// Gets a struct in the document by name.
    pub fn struct_by_name(&self, name: &str) -> Option<StructRef<'_>> {
        self.data.cache.struct_by_name(name).map(|(_hash, s)| s)
    }

    /// Gets the enums in the document.
    pub fn local_enums(&self) -> impl Iterator<Item = &Enum> {
        self.data.cache.local_enums().map(|(_idx, _hash, e)| e)
    }

    /// Gets a locally defined enum in the document by name.
    pub fn local_enum_by_name(&self, name: &str) -> Option<&Enum> {
        self.data
            .cache
            .local_enum_by_name(name)
            .map(|(_idx, _hash, e)| e)
    }

    /// Gets the enums in the document.
    pub fn enums(&self) -> impl Iterator<Item = EnumRef<'_>> {
        self.data.cache.enums()
    }

    /// Gets an imported enum in the document by local name.
    pub fn imported_enum_by_name(&self, name: &str) -> Option<&ImportedEnum> {
        self.data
            .cache
            .imported_enum_by_name(name)
            .map(|(_hash, e)| e)
    }

    /// Gets an enum in the document by name.
    pub fn enum_by_name(&self, name: &str) -> Option<EnumRef<'_>> {
        self.data.cache.enum_by_name(name).map(|(_hash, e)| e)
    }

    /// Gets the custom type by name.
    pub fn get_custom_type(&self, name: &str) -> Option<&Type> {
        if let Some(s) = self.struct_by_name(name) {
            return s.ty();
        }

        if let Some(e) = self.enum_by_name(name) {
            return e.ty();
        }

        None
    }

    /// Gets a cache key for an enum choice lookup.
    pub fn get_choice_cache_key(&self, name: &str, choice: &str) -> Option<EnumChoiceCacheKey> {
        let (source_uri, enum_index, r#enum) =
            if let Some((enum_index, _, r#enum)) = self.data.cache.local_enum_by_name(name) {
                (self.data.uri.clone(), enum_index, r#enum)
            } else {
                let (_, imported) = self.data.cache.imported_enum_by_name(name)?;
                let (enum_index, _, r#enum) = imported
                    .document
                    .data
                    .cache
                    .local_enum_by_name(imported.definition().name().text())?;
                (imported.document.uri(), enum_index, r#enum)
            };

        let enum_ty = r#enum.ty()?.as_enum()?;
        let choice_index = enum_ty.choices().iter().position(|v| v == choice)?;
        Some(EnumChoiceCacheKey::new(
            source_uri,
            enum_index,
            choice_index,
        ))
    }

    /// Gets the parse diagnostics for the document.
    pub fn parse_diagnostics(&self) -> &[Diagnostic] {
        &self.data.parse_diagnostics
    }

    /// Gets the analysis diagnostics for the document.
    pub fn analysis_diagnostics(&self) -> &Diagnostics {
        &self.data.analysis_diagnostics
    }

    /// Gets all diagnostics for the document (both from parsing and analysis).
    pub fn diagnostics(&self) -> impl Iterator<Item = &Diagnostic> {
        self.data
            .parse_diagnostics
            .iter()
            .chain(self.data.analysis_diagnostics.diagnostics.iter())
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
    pub fn extend_diagnostics(&mut self, diagnostics: Diagnostics) -> Self {
        let data = &mut self.data;
        let inner = Arc::get_mut(data).expect("should only have one reference");
        inner.analysis_diagnostics.extend(diagnostics.diagnostics);
        Self { data: data.clone() }
    }

    /// Finds a scope based on a position within the document.
    pub fn find_scope_by_position(&self, position: usize) -> Option<ScopeRef<'_>> {
        /// Finds a scope within a collection of sorted scopes by position.
        fn find_scope(scopes: &[Scope], position: usize) -> Option<ScopeRef<'_>> {
            let mut index = match scopes.binary_search_by_key(&position, |s| s.span.start()) {
                Ok(index) => index,
                Err(index) => {
                    // This indicates that we couldn't find a match and the
                    // match would go _before_
                    // the first scope, so there is no containing scope.
                    if index == 0 {
                        return None;
                    }

                    index - 1
                }
            };

            // We now have the index to start looking up the list of scopes
            // We walk up the list to try to find a span that contains the
            // position
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
        if let Some(workflow) = self.data.cache.workflow()
            && workflow.scope().span().contains(position)
        {
            return find_scope(&workflow.scopes, position);
        }

        // Search for a task that might contain the position
        let task = self
            .data
            .cache
            .local_tasks()
            .filter_map(|(_idx, _hash, t)| {
                if t.scope().span().start() <= position {
                    Some(t)
                } else {
                    None
                }
            })
            .find_or_last(|t| t.scope().span().start() == position)?;

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
        for ns in self.namespaces() {
            if ns.document().has_errors() {
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
