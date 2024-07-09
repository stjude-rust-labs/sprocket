//! Representation of the analysis document graph.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use std::mem;
use std::path::absolute;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use path_clean::clean;
use petgraph::graph::NodeIndex;
use petgraph::stable_graph::StableDiGraph;
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use rowan::GreenNode;
use url::Url;
use wdl_ast::Diagnostic;

use crate::AnalysisResult;
use crate::DocumentScope;

/// Represents the identifier of an analyzed document.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DocumentId {
    /// The identifier is by absolute file path.
    Path(PathBuf),
    /// The identifier is by URI.
    Uri(Url),
}

impl DocumentId {
    /// Makes a document identifier relative to another.
    pub(crate) fn relative_to(base: &DocumentId, id: &str) -> Result<Self> {
        if let Ok(uri) = id.parse() {
            return Ok(Self::Uri(uri));
        }

        match base {
            Self::Path(base) => Ok(Self::Path(clean(
                base.parent().expect("expected a parent").join(id),
            ))),
            Self::Uri(base) => Ok(Self::Uri(base.join(id)?)),
        }
    }

    /// Gets the path of the document.
    ///
    /// Returns `None` if the document does not have a local path.
    pub fn path(&self) -> Option<Cow<'_, Path>> {
        match self {
            Self::Path(path) => Some(path.into()),
            Self::Uri(uri) => uri.to_file_path().ok().map(Into::into),
        }
    }

    /// Gets the document identifier as a string.
    pub fn to_str(&self) -> Cow<'_, str> {
        match self {
            DocumentId::Path(p) => p.to_string_lossy(),
            DocumentId::Uri(u) => u.as_str().into(),
        }
    }
}

impl fmt::Display for DocumentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DocumentId::Path(path) => write!(f, "{}", path.display()),
            DocumentId::Uri(uri) => write!(f, "{}", uri),
        }
    }
}

impl TryFrom<&Path> for DocumentId {
    type Error = anyhow::Error;

    fn try_from(value: &Path) -> Result<Self> {
        Ok(Self::Path(clean(absolute(value).with_context(|| {
            format!(
                "failed to determine the absolute path of `{path}`",
                path = value.display()
            )
        })?)))
    }
}

impl TryFrom<&str> for DocumentId {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self> {
        match Url::parse(value) {
            Ok(uri) => Ok(Self::Uri(uri)),
            Err(_) => Self::try_from(Path::new(value)),
        }
    }
}

impl From<Url> for DocumentId {
    fn from(value: Url) -> Self {
        Self::Uri(value)
    }
}

/// Represents the in-progress analysis state for a document.
#[derive(Debug, Default)]
pub(crate) struct InProgressAnalysisState {
    /// The diagnostics of the document.
    pub diagnostics: Vec<Diagnostic>,
    /// The document's scope.
    pub scope: DocumentScope,
}

/// Represents the completed analysis state of a document.
#[derive(Debug)]
pub(crate) struct CompletedAnalysisState {
    /// The diagnostics of the document.
    pub diagnostics: Arc<[Diagnostic]>,
    /// The document's scope.
    pub scope: Arc<DocumentScope>,
}

impl From<InProgressAnalysisState> for CompletedAnalysisState {
    fn from(value: InProgressAnalysisState) -> Self {
        let mut diagnostics = value.diagnostics;
        diagnostics.sort_by(|a, b| match (a.labels().next(), b.labels().next()) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            (Some(a), Some(b)) => a.span().start().cmp(&b.span().start()),
        });
        Self {
            diagnostics: diagnostics.into(),
            scope: value.scope.into(),
        }
    }
}

/// Represents the state of a document's analysis.
#[derive(Debug)]
pub(crate) enum AnalysisState {
    /// The analysis is in-progress and the data is mutable.
    InProgress(InProgressAnalysisState),
    /// The analysis has completed and the data is immutable.
    Completed(CompletedAnalysisState),
}

impl AnalysisState {
    /// Gets the mutable in-progress analysis state.
    ///
    /// # Panics
    ///
    /// Panics if the analysis has completed.
    pub(crate) fn in_progress(&mut self) -> &mut InProgressAnalysisState {
        match self {
            Self::InProgress(state) => state,
            Self::Completed(_) => panic!("analysis has completed"),
        }
    }

    /// Gets the immutable completed analysis state.
    ///
    /// # Panics
    ///
    /// Panics if the analysis has not completed.
    pub(crate) fn completed(&self) -> &CompletedAnalysisState {
        match self {
            Self::InProgress(_) => panic!("analysis has not completed"),
            Self::Completed(state) => state,
        }
    }

    /// Completes the analysis state.
    ///
    /// # Panics
    ///
    /// Panics if the analysis has already completed.
    fn complete(&mut self) {
        match self {
            Self::InProgress(state) => {
                *self = Self::Completed(mem::take(state).into());
            }
            Self::Completed(_) => panic!("analysis has completed"),
        }
    }
}

impl Default for AnalysisState {
    fn default() -> Self {
        Self::InProgress(Default::default())
    }
}

/// Represents an analyzed document.
#[derive(Debug)]
pub(crate) struct Document {
    /// The identifier of the analyzed document.
    pub id: Arc<DocumentId>,
    /// The root node of the document.
    ///
    /// If `None`, it means we failed to read the document's source.
    pub root: Option<GreenNode>,
    /// The error when attempting to read the document's source.
    ///
    /// This is `Some` if we failed to read the document's source.
    pub error: Option<Arc<anyhow::Error>>,
    /// The analysis state of the document.
    pub state: AnalysisState,
    /// Whether or not this document is a GC root in the document graph.
    ///
    /// A GC root won't be removed from the document graph even if there are no
    /// outgoing edges.
    pub gc_root: bool,
}

impl Document {
    /// Creates a new empty document.
    pub fn new(id: Arc<DocumentId>, gc_root: bool) -> Self {
        Self {
            id,
            root: None,
            error: None,
            state: Default::default(),
            gc_root,
        }
    }

    /// Creates a new document from the result of a parse.
    pub fn from_parse(
        id: Arc<DocumentId>,
        root: GreenNode,
        diagnostics: Vec<Diagnostic>,
        gc_root: bool,
    ) -> Self {
        Self {
            id,
            root: Some(root),
            error: None,
            state: AnalysisState::InProgress(InProgressAnalysisState {
                diagnostics,
                ..Default::default()
            }),
            gc_root,
        }
    }

    /// Creates a new document from an error attempting to read the document.
    pub fn from_error(id: Arc<DocumentId>, error: anyhow::Error, gc_root: bool) -> Self {
        Self {
            id,
            root: None,
            error: Some(Arc::new(error)),
            state: Default::default(),
            gc_root,
        }
    }

    /// Called to complete the analysis on the document.
    pub fn complete(&mut self) {
        self.state.complete();
    }
}

/// Represents a document graph.
#[derive(Debug, Default)]
pub(crate) struct DocumentGraph {
    /// The inner graph.
    ///
    /// Each node in the graph represents an analyzed file and edges denote
    /// import dependency relationships.
    pub inner: StableDiGraph<Document, ()>,
    /// Map from document identifier to graph node index.
    pub indexes: HashMap<Arc<DocumentId>, NodeIndex>,
}

impl DocumentGraph {
    /// Gets a document from the graph.
    pub fn document(&self, id: &DocumentId) -> Option<(NodeIndex, &Document)> {
        self.indexes
            .get(id)
            .map(|index| (*index, &self.inner[*index]))
    }

    /// Adds a document to the graph.
    ///
    /// If the document with the same identifier exists in the graph, it is
    /// replaced.
    pub fn add_document(&mut self, document: Document) -> NodeIndex {
        if let Some(index) = self.indexes.get(&document.id) {
            self.inner[*index] = document;
            return *index;
        }

        let id = document.id.clone();
        let index = self.inner.add_node(document);
        let prev = self.indexes.insert(id, index);
        assert!(prev.is_none());
        index
    }

    /// Merges this document graph with the provided one.
    ///
    /// Returns the result of the analysis.
    ///
    /// This also performs a GC on the graph to remove non-rooted nodes that
    /// have no outgoing edges.
    pub fn merge(&mut self, mut other: Self) -> Vec<AnalysisResult> {
        let mut remapped = HashMap::new();
        let mut results = Vec::new();
        for (id, other_index) in other.indexes {
            let Document {
                id: _,
                root,
                error,
                state,
                gc_root,
            } = &mut other.inner[other_index];
            match self.indexes.get(&id) {
                Some(index) => {
                    remapped.insert(other_index, *index);

                    // Existing node, so replace the document contents
                    let existing = &mut self.inner[*index];
                    *existing = Document {
                        id,
                        root: mem::take(root),
                        error: mem::take(error),
                        state: mem::take(state),
                        gc_root: existing.gc_root | *gc_root,
                    };

                    // Add a result for root documents or non-root documents that parsed
                    if *gc_root || existing.root.is_some() {
                        results.push(AnalysisResult::new(existing));
                    }

                    // Remove all edges to this node in self; we'll add the latest edges below.
                    for edge in self.inner.edges(*index).map(|e| e.id()).collect::<Vec<_>>() {
                        self.inner.remove_edge(edge);
                    }
                }
                None => {
                    let document = Document {
                        id: id.clone(),
                        root: mem::take(root),
                        error: mem::take(error),
                        state: mem::take(state),
                        gc_root: *gc_root,
                    };

                    // Add a result for root documents or non-root documents that parsed
                    if *gc_root || document.root.is_some() {
                        results.push(AnalysisResult::new(&document));
                    }

                    // New node, insert it into the graph
                    let index = self.inner.add_node(document);

                    remapped.insert(other_index, index);
                    self.indexes.insert(id, index);
                }
            }
        }

        // Now add the edges for the remapped nodes
        for edge in other.inner.edge_indices() {
            let (from, to) = other.inner.edge_endpoints(edge).expect("edge should exist");
            let from = remapped[&from];
            let to = remapped[&to];
            self.inner.add_edge(from, to, ());
        }

        // Finally, GC any non-gc-root nodes that have no outgoing edges
        let mut gc = Vec::new();
        for node in self.inner.node_indices() {
            if self.inner[node].gc_root {
                continue;
            }

            if self
                .inner
                .edges_directed(node, Direction::Outgoing)
                .next()
                .is_none()
            {
                gc.push(node);
            }
        }

        for node in gc {
            self.inner.remove_node(node);
        }

        results.sort_by(|a, b| a.id().cmp(b.id()));
        results
    }
}
