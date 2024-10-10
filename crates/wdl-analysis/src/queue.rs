//! Implements the analysis queue.

use std::cell::RefCell;
use std::marker::PhantomData;
use std::ops::Range;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;
use futures::Future;
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use indexmap::IndexSet;
use parking_lot::RwLock;
use petgraph::Direction;
use petgraph::graph::NodeIndex;
use reqwest::Client;
use tokio::runtime::Handle;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::oneshot;
use tracing::debug;
use tracing::info;
use url::Url;
use wdl_ast::Ast;
use wdl_ast::AstToken;

use crate::AnalysisResult;
use crate::DiagnosticsConfig;
use crate::IncrementalChange;
use crate::ProgressKind;
use crate::graph::Analysis;
use crate::graph::DfsSpace;
use crate::graph::DocumentGraph;
use crate::graph::ParseState;
use crate::rayon::RayonHandle;
use crate::scope::DocumentScope;

/// The minimum number of milliseconds between analysis progress reports.
const MINIMUM_PROGRESS_MILLIS: u128 = 50;

/// Represents a request to the analysis queue.
pub enum Request<Context> {
    /// A request to add documents to the graph.
    Add(AddRequest),
    /// A request to analyze documents.
    Analyze(AnalyzeRequest<Context>),
    /// A request to remove documents from the graph.
    Remove(RemoveRequest),
    /// A request to process a document's incremental change.
    NotifyIncrementalChange(NotifyIncrementalChangeRequest),
    /// A request to process a document's change.
    NotifyChange(NotifyChangeRequest),
}

/// Represents a request to add documents to the graph.
pub struct AddRequest {
    /// The documents to add to the graph.
    pub documents: IndexSet<Url>,
    /// The sender for completing the request.
    pub completed: oneshot::Sender<()>,
}

/// Represents a request to analyze documents.
pub struct AnalyzeRequest<Context> {
    /// The specific document to analyze.
    ///
    /// If this is `None`, all rooted documents will be analyzed.
    pub document: Option<Url>,
    /// The context to provide to the progress callback.
    pub context: Context,
    /// The sender for completing the request.
    pub completed: oneshot::Sender<Result<Vec<AnalysisResult>>>,
}

/// Represents a request to remove documents from the document graph.
pub struct RemoveRequest {
    /// The documents to remove.
    pub documents: Vec<Url>,
    /// The sender for completing the request.
    pub completed: oneshot::Sender<()>,
}

/// Represents a request to process an incremental change to a document.
pub struct NotifyIncrementalChangeRequest {
    /// The document that has changed.
    pub document: Url,
    /// The incremental change to the document.
    pub change: IncrementalChange,
}

/// Represents a request to process a change to a document.
pub struct NotifyChangeRequest {
    /// The document that has changed.
    pub document: Url,
    /// Whether or not any existing incremental change should be discarded.
    pub discard_pending: bool,
}

/// A simple enumeration to signal a cancellation to the caller.
enum Cancelable<T> {
    /// The operation completed and yielded a value.
    Completed(T),
    /// The operation was canceled.
    Canceled,
}

/// Represents the analysis queue.
pub struct AnalysisQueue<Progress, Context, Return, Validator> {
    /// The document graph maintained by the analysis queue.
    graph: Arc<RwLock<DocumentGraph>>,
    /// The diagnostics configuration to use.
    config: DiagnosticsConfig,
    /// The handle to the tokio runtime for blocking on async tasks.
    tokio: Handle,
    /// The HTTP client to use for fetching documents.
    client: Client,
    /// The progress callback to use.
    progress: Arc<Progress>,
    /// The validator callback to use.
    validator: Arc<Validator>,
    /// A marker for the `Context` and `Return` types.
    marker: PhantomData<(Context, Return)>,
}

impl<Progress, Context, Return, Validator> AnalysisQueue<Progress, Context, Return, Validator>
where
    Progress: Fn(Context, ProgressKind, usize, usize) -> Return + Send + 'static,
    Context: Send + Clone,
    Return: Future<Output = ()>,
    Validator: Fn() -> wdl_ast::Validator + Send + Sync + 'static,
{
    /// Constructs a new analysis queue.
    pub fn new(
        config: DiagnosticsConfig,
        tokio: Handle,
        progress: Progress,
        validator: Validator,
    ) -> Self {
        Self {
            graph: Default::default(),
            config,
            tokio,
            progress: Arc::new(progress),
            marker: PhantomData,
            client: Default::default(),
            validator: Arc::new(validator),
        }
    }

    /// Runs the analysis queue.
    pub fn run(&self, mut receiver: UnboundedReceiver<Request<Context>>) {
        info!("analysis queue has started");

        while let Some(request) = self.tokio.block_on(receiver.recv()) {
            match request {
                Request::Add(AddRequest {
                    documents,
                    completed,
                }) => {
                    let start = Instant::now();
                    info!(
                        "received request to add {count} document(s) to the graph",
                        count = documents.len()
                    );

                    self.add_documents(documents);

                    info!(
                        "request to add documents completed in {elapsed:?}",
                        elapsed = start.elapsed()
                    );

                    completed.send(()).ok();
                }
                Request::Analyze(AnalyzeRequest {
                    document,
                    context,
                    completed,
                }) => {
                    let start = Instant::now();
                    if let Some(document) = &document {
                        info!("received request to document `{document}`");
                    } else {
                        info!("received request to analyze all documents");
                    }

                    match self.analyze(document, context, &completed) {
                        Cancelable::Completed(results) => {
                            info!(
                                "request to analyze documents completed in {elapsed:?}",
                                elapsed = start.elapsed()
                            );

                            completed.send(results).ok();
                        }
                        Cancelable::Canceled => {
                            info!(
                                "request to analyze documents was canceled after {elapsed:?}",
                                elapsed = start.elapsed()
                            );
                        }
                    }
                }
                Request::Remove(RemoveRequest {
                    documents,
                    completed,
                }) => {
                    let start = Instant::now();
                    info!(
                        "received request to remove {count} documents(s)",
                        count = documents.len()
                    );

                    self.remove_documents(documents);

                    info!(
                        "request to remove documents completed in {elapsed:?}",
                        elapsed = start.elapsed()
                    );

                    completed.send(()).ok();
                }
                Request::NotifyIncrementalChange(NotifyIncrementalChangeRequest {
                    document,
                    change,
                }) => {
                    let mut graph = self.graph.write();
                    if let Some(node) = graph.get_index(&document) {
                        graph.get_mut(node).notify_incremental_change(change);
                    }
                }
                Request::NotifyChange(NotifyChangeRequest {
                    document,
                    discard_pending,
                }) => {
                    let mut graph = self.graph.write();
                    if let Some(node) = graph.get_index(&document) {
                        graph.get_mut(node).notify_change(discard_pending);
                    }
                }
            }
        }

        info!("analysis queue has shut down");
    }

    /// Adds a set of documents to the document graph.
    fn add_documents(&self, documents: IndexSet<Url>) {
        let mut graph = self.graph.write();
        for document in documents {
            graph.add_node(document, true);
        }
    }

    /// Analyzes the requested documents.
    fn analyze(
        &self,
        document: Option<Url>,
        context: Context,
        completed: &oneshot::Sender<Result<Vec<AnalysisResult>>>,
    ) -> Cancelable<Result<Vec<AnalysisResult>>> {
        // Analysis works by building a subgraph of what needs to be analyzed.
        // We start with the requested node or all roots. We then perform a
        // breadth-first traversal maintaining the set of nodes that compromises the
        // subgraph. At each step of the traversal, we reparse what has changed. The
        // traversal is complete when no new nodes are added to the subgraph node set.

        let mut subgraph = {
            let graph = self.graph.read();
            match document {
                Some(document) => {
                    // Check to see if the document is a rooted node
                    let index = match graph.get_index(&document) {
                        Some(index) if graph.is_rooted(index) => index,
                        _ => return Cancelable::Completed(Ok(Vec::new())),
                    };

                    let mut nodes = IndexSet::new();
                    nodes.insert(index);
                    nodes
                }
                None => graph.roots().clone(),
            }
        };

        // The current starting offset into the subgraph slice to process
        let mut offset = 0;
        let mut space = Default::default();

        loop {
            if completed.is_closed() {
                info!("analysis request has been canceled");
                return Cancelable::Canceled;
            }

            let slice = subgraph
                .as_slice()
                .get_range(offset..)
                .expect("offset should be valid");

            // If there's no more nodes to process, we're done building the subgraph
            if slice.is_empty() {
                break;
            }

            // Spawn parse tasks for nodes that need to be reparsed
            let tasks = {
                let graph = self.graph.read();
                slice
                    .iter()
                    .filter_map(|index| {
                        let node = graph.get(*index);
                        if node.needs_parse() {
                            Some(self.spawn_parse_task(*index))
                        } else {
                            None
                        }
                    })
                    .collect::<FuturesUnordered<_>>()
            };

            let parsed =
                match self.await_with_progress(ProgressKind::Parsing, tasks, completed, &context) {
                    Cancelable::Completed(parsed) => parsed,
                    Cancelable::Canceled => return Cancelable::Canceled,
                };

            // Update the graph, potentially adding more nodes to the subgraph
            let len = slice.len();
            if let Err(e) =
                self.update_graphs(parsed, &mut subgraph, offset..offset + len, &mut space)
            {
                return Cancelable::Completed(Err(e));
            }

            offset += len;
        }

        // Create the actual subgraph from the subgraph nodes
        // Nodes in the subgraph will be removed once analyzed
        let mut subgraph = self.graph.read().subgraph(&subgraph);
        let mut set = Vec::new();
        let mut results: Vec<AnalysisResult> = Vec::new();
        while subgraph.node_count() > 0 {
            if completed.is_closed() {
                info!("analysis request has been canceled");
                return Cancelable::Canceled;
            }

            // Build a set of nodes with no incoming edges (i.e. no unanalyzed dependencies)
            set.clear();
            for node in subgraph.node_indices() {
                if subgraph
                    .edges_directed(node, Direction::Incoming)
                    .next()
                    .is_none()
                {
                    set.push(node);
                }
            }

            assert!(!set.is_empty(), "the set cannot be empty");

            // Remove the nodes we're about to analyze from the subgraph
            // This also removes the outgoing edges from those nodes
            for index in &set {
                subgraph.remove_node(*index);
            }

            let tasks = {
                let graph = self.graph.read();
                set.iter()
                    .filter_map(|index| {
                        let index = *index;
                        let node = graph.get(index);
                        if node.analysis().is_some() {
                            if graph.include_result(index) {
                                results.push(AnalysisResult::new(node));
                            }
                            return None;
                        }

                        let graph = self.graph.clone();
                        let config = self.config;
                        Some(RayonHandle::spawn(move || {
                            Self::analyze_node(config, graph, index)
                        }))
                    })
                    .collect::<FuturesUnordered<_>>()
            };

            let analyzed =
                match self.await_with_progress(ProgressKind::Analyzing, tasks, completed, &context)
                {
                    Cancelable::Completed(analyzed) => analyzed,
                    Cancelable::Canceled => return Cancelable::Canceled,
                };

            let mut graph = self.graph.write();
            results.extend(analyzed.into_iter().filter_map(|(index, analysis)| {
                let node = graph.get_mut(index);
                node.analysis_completed(analysis);

                if graph.include_result(index) {
                    Some(AnalysisResult::new(graph.get(index)))
                } else {
                    None
                }
            }));
        }

        results.sort_by(|a, b| a.uri().cmp(b.uri()));
        Cancelable::Completed(Ok(results))
    }

    /// Removes documents from the graph.
    ///
    /// If any of the removed documents are roots that have no outgoing edges,
    /// the nodes will be removed from the graph.
    fn remove_documents(&self, uris: Vec<Url>) {
        let mut graph = self.graph.write();

        for uri in uris {
            graph.remove_root(&uri);
        }

        graph.gc();
    }

    /// Awaits the given set of futures while providing progress to the given
    /// callback.
    fn await_with_progress<Fut, Output>(
        &self,
        kind: ProgressKind,
        mut tasks: FuturesUnordered<Fut>,
        completed: &oneshot::Sender<Result<Vec<AnalysisResult>>>,
        context: &Context,
    ) -> Cancelable<Vec<Output>>
    where
        Fut: Future<Output = Output>,
    {
        if tasks.is_empty() {
            return Cancelable::Completed(Vec::new());
        }

        let total = tasks.len();
        self.tokio
            .block_on((self.progress)(context.clone(), kind, 0, total));

        let update_progress = self.progress.clone();
        let results = self.tokio.block_on(async move {
            let mut count = 0;
            let mut results = Vec::new();
            let mut last_progress = Instant::now();
            while let Some(result) = tasks.next().await {
                if completed.is_closed() {
                    break;
                }

                results.push(result);
                count += 1;

                let now = Instant::now();
                if count < total && (now - last_progress).as_millis() > MINIMUM_PROGRESS_MILLIS {
                    info!("{count} out of {total} {kind} task(s) have completed");
                    last_progress = now;
                    update_progress(context.clone(), kind, count, total).await;
                }
            }

            results
        });

        if results.len() < total {
            info!(
                "{count} out of {total} {kind} task(s) have completed; canceled {canceled} tasks",
                count = results.len(),
                canceled = total - results.len()
            );
        } else {
            info!(
                "{count} out of {total} {kind} task(s) have completed",
                count = results.len()
            );
        }

        // Report all have completed even if there are cancellations
        self.tokio
            .block_on((self.progress)(context.clone(), kind, total, total));

        if completed.is_closed() {
            Cancelable::Canceled
        } else {
            Cancelable::Completed(results)
        }
    }

    /// Spawns a parse task on a rayon thread.
    fn spawn_parse_task(&self, index: NodeIndex) -> RayonHandle<(NodeIndex, Result<ParseState>)> {
        let graph = self.graph.clone();
        let tokio = self.tokio.clone();
        let client = self.client.clone();
        let validator = self.validator.clone();
        RayonHandle::spawn(move || {
            thread_local! {
                static VALIDATOR: RefCell<Option<wdl_ast::Validator>> = const { RefCell::new(None) };
            }

            VALIDATOR.with_borrow_mut(|v| {
                let validator = v.get_or_insert_with(|| validator());
                let graph = graph.read();
                let node = graph.get(index);
                let state = node.parse(&tokio, &client, validator);
                (index, state)
            })
        })
    }

    /// Updates the graph and subgraphs.
    ///
    /// This processes parsed nodes and also adding the direct dependencies of
    /// nodes added to the subgraph.
    fn update_graphs(
        &self,
        parsed: Vec<(NodeIndex, Result<ParseState>)>,
        subgraph: &mut IndexSet<NodeIndex>,
        range: Range<usize>,
        space: &mut DfsSpace,
    ) -> Result<()> {
        let mut graph = self.graph.write();

        // Start by updating the parsed nodes
        for (index, state) in parsed {
            let node = graph.get_mut(index);
            let state = state
                .with_context(|| format!("failed to parse document `{uri}`", uri = node.uri()))?;
            node.parse_completed(state);

            // Remove all dependency edges from the node as the imports might have changed
            graph.remove_dependency_edges(index);

            // Add back dependency edges for the document's imports
            match graph.get(index).document().map(|d| d.ast()) {
                None | Some(Ast::Unsupported) => {}
                Some(Ast::V1(ast)) => {
                    for import in ast.imports() {
                        let text = match import.uri().text() {
                            Some(text) => text,
                            None => continue,
                        };

                        let import_uri = match graph.get(index).uri().join(text.as_str()) {
                            Ok(uri) => uri,
                            Err(_) => continue,
                        };

                        // Add a dependency edge to the import
                        let import_index = graph
                            .get_index(&import_uri)
                            .unwrap_or_else(|| graph.add_node(import_uri, false));
                        graph.add_dependency_edge(index, import_index, space);

                        // Add the import to the subgraph
                        subgraph.insert(import_index);
                    }
                }
            }

            // Because of the way WDL works by implicitly introducing import names into
            // document scope, any change to a file must cause all transitive dependencies
            // to be reanalyzed; therefore, do a BFS from the parsed node and add any
            // discovered nodes to the subgraph
            graph.bfs_mut(index, |graph, dependent: NodeIndex| {
                if index == dependent {
                    return;
                }

                let node = graph.get_mut(dependent);
                if !subgraph.contains(&dependent) {
                    debug!(
                        "adding dependent document `{uri}` for analysis",
                        uri = node.uri()
                    );
                    subgraph.insert(dependent);
                }

                node.reanalyze();
            });
        }

        // Add the direct dependencies of the subgraph slice to the subgraph
        let mut dependencies = Vec::new();
        for index in subgraph.get_range(range).expect("range should be valid") {
            dependencies.extend(graph.dependencies(*index));
        }

        subgraph.extend(dependencies);
        Ok(())
    }

    /// Analyzes a node in the document graph.
    fn analyze_node(
        config: DiagnosticsConfig,
        graph: Arc<RwLock<DocumentGraph>>,
        index: NodeIndex,
    ) -> (NodeIndex, Analysis) {
        let start = Instant::now();
        let graph = graph.read();
        let (scope, diagnostics) = DocumentScope::new(config, &graph, index);

        info!(
            "analysis of `{uri}` completed in {elapsed:?}",
            uri = graph.get(index).uri(),
            elapsed = start.elapsed()
        );

        (index, Analysis::new(scope, diagnostics))
    }
}
