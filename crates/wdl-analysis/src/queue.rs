//! Implements the analysis queue.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::marker::PhantomData;
use std::ops::Range;
use std::sync::Arc;
use std::time::Instant;

use futures::stream::FuturesUnordered;
use futures::Future;
use futures::StreamExt;
use indexmap::IndexSet;
use parking_lot::RwLock;
use petgraph::graph::NodeIndex;
use petgraph::Direction;
use reqwest::Client;
use tokio::runtime::Handle;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::oneshot;
use url::Url;
use wdl_ast::Ast;
use wdl_ast::AstToken;
use wdl_ast::Validator;

use crate::graph::Analysis;
use crate::graph::DocumentGraph;
use crate::graph::ParseState;
use crate::rayon::RayonHandle;
use crate::AnalysisResult;
use crate::DocumentChange;
use crate::DocumentScope;
use crate::ProgressKind;

/// The minimum number of milliseconds between analysis progress reports.
const MINIMUM_PROGRESS_MILLIS: u128 = 50;

/// Represents a request to the analysis queue.
pub enum Request {
    /// A request to analyze documents.
    Analyze(AnalyzeRequest),
    /// A request to remove documents.
    Remove(RemoveRequest),
}

/// Represents a request to the analyze documents.
pub struct AnalyzeRequest {
    /// The documents to analyze.
    pub documents: Vec<Arc<Url>>,
    /// The context to provide to the progress callback.
    pub context: Option<String>,
    /// The sender for completing the request.
    pub completed: oneshot::Sender<Vec<AnalysisResult>>,
}

/// Represents a request to remove documents from the document graph.
pub struct RemoveRequest {
    /// The URIs to remove.
    pub uris: Vec<Url>,
    /// The sender for completing the request.
    pub completed: oneshot::Sender<()>,
}

/// Represents the analysis queue.
pub struct AnalysisQueue<P, R, V, C> {
    /// The document graph maintained by the analysis queue.
    graph: Arc<RwLock<DocumentGraph>>,
    /// The handle to the tokio runtime for blocking on async tasks.
    tokio: Handle,
    /// The HTTP client to use for fetching documents.
    client: Client,
    /// The progress callback to use.
    progress: Arc<P>,
    /// A marker for the `R` type.
    marker: PhantomData<R>,
    /// The validator callback to use.
    validator: Arc<V>,
    /// The changes callback to use.
    changes: C,
}

impl<P, R, V, C> AnalysisQueue<P, R, V, C>
where
    P: Fn(ProgressKind, usize, usize, Option<String>) -> R + Send + Sync + 'static,
    R: Future<Output = ()>,
    V: Fn() -> Validator + Send + Sync + 'static,
    C: Fn(&Url) -> Option<DocumentChange>,
{
    /// Constructs a new analysis queue.
    pub fn new(tokio: Handle, progress: P, validator: V, changes: C) -> Self {
        Self {
            graph: Default::default(),
            tokio,
            progress: Arc::new(progress),
            marker: PhantomData,
            client: Default::default(),
            validator: Arc::new(validator),
            changes,
        }
    }

    /// Runs the analysis queue.
    pub fn run(&self, mut receiver: UnboundedReceiver<Request>) {
        log::info!("analysis queue has started");

        while let Some(request) = self.tokio.block_on(receiver.recv()) {
            match request {
                Request::Analyze(AnalyzeRequest {
                    documents,
                    context,
                    completed,
                }) => {
                    let start = Instant::now();
                    log::info!(
                        "received request to analyze {count} document(s)",
                        count = documents.len()
                    );

                    self.analyze(documents, context, completed);

                    log::info!(
                        "analysis request completed in {elapsed:?}",
                        elapsed = start.elapsed()
                    );
                }
                Request::Remove(RemoveRequest { uris, completed }) => {
                    let start = Instant::now();
                    log::info!(
                        "received request to remove {count} URI(s)",
                        count = uris.len()
                    );

                    self.remove_documents(uris, completed);

                    log::info!(
                        "removal request completed in {elapsed:?}",
                        elapsed = start.elapsed()
                    );
                }
            }
        }

        log::info!("analysis queue has shut down");
    }

    /// Analyzes the requested documents.
    fn analyze(
        &self,
        documents: Vec<Arc<Url>>,
        context: Option<String>,
        completed: oneshot::Sender<Vec<AnalysisResult>>,
    ) {
        // Analysis works by building a subgraph of what needs to be analyzed.
        // We start with the requested documents, adding them as roots to the graph if
        // not already present. We then perform a breadth-first traversal maintaining
        // the set of nodes that compromises the subgraph. At each step of the
        // traversal, we reparse what has changed. The traversal is complete when no new
        // nodes are added to the subgraph node set.

        // The subgraph being built, populated initially with the requested nodes
        let mut subgraph: IndexSet<NodeIndex> = {
            let mut graph = self.graph.write();
            IndexSet::from_iter(documents.into_iter().map(|uri| graph.add_node(uri, true)))
        };

        // The current starting offset into the subgraph slice to process
        let mut offset = 0;

        loop {
            if completed.is_closed() {
                log::info!("analysis request has been canceled");
                return;
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
            let tasks = slice
                .iter()
                .filter_map(|index: &NodeIndex| {
                    let graph = self.graph.read();
                    let node = graph.get(*index);
                    let change = (self.changes)(node.uri());
                    if node.needs_parse(&change) {
                        Some(self.spawn_parse_task(*index, change))
                    } else {
                        None
                    }
                })
                .collect::<FuturesUnordered<_>>();

            let parsed =
                self.await_with_progress(ProgressKind::Parsing, tasks, &completed, &context);

            // Update the graph, potentially adding more nodes to the subgraph
            let len = slice.len();
            self.update_graphs(parsed, &mut subgraph, offset..offset + len);
            offset += len;
        }

        // Create the actual subgraph from the subgraph nodes
        // Nodes in the subgraph will be removed once analyzed
        let mut subgraph = self.graph.read().subgraph(&subgraph);
        let mut set = Vec::new();
        let mut results = Vec::new();
        while subgraph.node_count() > 0 {
            if completed.is_closed() {
                log::info!("analysis request has been canceled");
                return;
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

            let tasks = set
                .iter()
                .filter_map(|index| {
                    let index = *index;
                    let graph = self.graph.clone();
                    if graph.read().get(index).analysis().is_some() {
                        return None;
                    }

                    Some(RayonHandle::spawn(move || Self::analyze_node(graph, index)))
                })
                .collect::<FuturesUnordered<_>>();

            let analyzed =
                self.await_with_progress(ProgressKind::Analyzing, tasks, &completed, &context);

            let graph = self.graph.read();
            results.extend(analyzed.into_iter().filter_map(|index| {
                // Filter out results from files that either aren't rooted or failed to parse
                let node = graph.get(index);
                if graph.is_rooted(index) || matches!(node.parse_state(), ParseState::Parsed { .. })
                {
                    Some(AnalysisResult::new(node))
                } else {
                    None
                }
            }));
        }

        results.sort_by(|a, b| a.uri().cmp(b.uri()));
        completed.send(results).ok();
    }

    /// Removes documents from the graph.
    ///
    /// If any of the removed documents are roots that have no outgoing edges,
    /// the nodes will be removed from the graph.
    fn remove_documents(&self, uris: Vec<Url>, completed: oneshot::Sender<()>) {
        let mut graph = self.graph.write();

        for uri in uris {
            graph.remove_root(&uri);
        }

        graph.gc();

        completed.send(()).ok();
    }

    /// Awaits the given set of futures while providing progress to the given
    /// callback.
    fn await_with_progress<T, O>(
        &self,
        kind: ProgressKind,
        mut tasks: FuturesUnordered<T>,
        completed: &oneshot::Sender<Vec<AnalysisResult>>,
        context: &Option<String>,
    ) -> Vec<O>
    where
        T: Future<Output = O>,
    {
        if tasks.is_empty() {
            return Default::default();
        }

        let total = tasks.len();
        self.tokio
            .block_on((self.progress)(kind, 0, total, context.clone()));

        let update_progress: Arc<P> = self.progress.clone();
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
                    log::info!("{count} out of {total} {kind} task(s) have completed");
                    last_progress = now;
                    update_progress(kind, count, total, context.clone()).await;
                }
            }

            results
        });

        if results.len() < total {
            log::info!(
                "{count} out of {total} {kind} task(s) have completed; canceled {canceled} tasks",
                count = results.len(),
                canceled = total - results.len()
            );
        } else {
            log::info!(
                "{count} out of {total} {kind} task(s) have completed",
                count = results.len()
            );
        }

        // Report all have completed even if there are cancellations
        self.tokio
            .block_on((self.progress)(kind, total, total, context.clone()));
        results
    }

    /// Spawns a parse task on a rayon thread.
    fn spawn_parse_task(
        &self,
        index: NodeIndex,
        change: Option<DocumentChange>,
    ) -> RayonHandle<(NodeIndex, ParseState)> {
        let graph = self.graph.clone();
        let tokio = self.tokio.clone();
        let client = self.client.clone();
        let validator = self.validator.clone();
        RayonHandle::spawn(move || {
            thread_local! {
                static VALIDATOR: RefCell<Option<Validator>> = const { RefCell::new(None) };
            }

            VALIDATOR.with_borrow_mut(|v| {
                let validator = v.get_or_insert_with(|| validator());
                let graph = graph.read();
                let node = graph.get(index);
                let state = node.parse(&tokio, &client, change, validator);
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
        parsed: Vec<(NodeIndex, ParseState)>,
        subgraph: &mut IndexSet<NodeIndex>,
        range: Range<usize>,
    ) {
        let mut graph = self.graph.write();

        // Start by updating the parsed nodes
        for (index, state) in parsed {
            let node = graph.get_mut(index);
            node.set_parse_state(state);
            node.set_analysis(None);

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
                            Ok(uri) => Arc::new(uri),
                            Err(_) => continue,
                        };

                        // Add a dependency edge to the import
                        let import_index = graph
                            .get_index(&import_uri)
                            .unwrap_or_else(|| graph.add_node(import_uri, false));
                        graph.add_dependency_edge(index, import_index);

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
                    log::debug!(
                        "adding dependent document `{uri}` for analysis",
                        uri = node.uri()
                    );
                    subgraph.insert(dependent);
                }

                node.set_analysis(None);
            });
        }

        // Add the direct dependencies of the subgraph slice to the subgraph
        let mut dependencies = Vec::new();
        for index in subgraph.get_range(range).expect("range should be valid") {
            dependencies.extend(graph.dependencies(*index));
        }

        subgraph.extend(dependencies);
    }

    /// Analyzes a node in the document graph.
    fn analyze_node(graph: Arc<RwLock<DocumentGraph>>, index: NodeIndex) -> NodeIndex {
        let start = Instant::now();
        let (scope, mut diagnostics) = DocumentScope::new(&graph.read(), index);

        diagnostics.sort_by(|a, b| match (a.labels().next(), b.labels().next()) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            (Some(a), Some(b)) => a.span().start().cmp(&b.span().start()),
        });

        let mut graph = graph.write();

        log::info!(
            "analysis of `{uri}` completed in {elapsed:?}",
            uri = graph.get(index).uri(),
            elapsed = start.elapsed()
        );

        let node = graph.get_mut(index);
        node.set_analysis(Some(Analysis::new(scope, diagnostics)));
        index
    }
}
