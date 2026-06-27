//! Implements the analysis queue.

use std::collections::HashMap;
use std::marker::PhantomData;
use std::ops::Range;
use std::panic;
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use futures::Future;
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use indexmap::IndexMap;
use indexmap::IndexSet;
use lsp_types::CallHierarchyIncomingCall;
use lsp_types::CallHierarchyItem;
use lsp_types::CallHierarchyOutgoingCall;
use lsp_types::CompletionResponse;
use lsp_types::DocumentSymbolResponse;
use lsp_types::FoldingRange;
use lsp_types::GotoDefinitionResponse;
use lsp_types::Hover;
use lsp_types::InlayHint;
use lsp_types::Location;
use lsp_types::SemanticTokensResult;
use lsp_types::SignatureHelp;
use lsp_types::SymbolInformation;
use lsp_types::WorkspaceEdit;
use parking_lot::RwLock;
use petgraph::Direction;
use petgraph::graph::NodeIndex;
use reqwest::Client;
use tokio::runtime::Handle;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::oneshot;
use tracing::debug;
use tracing::error;
use tracing::trace;
use url::Url;
use wdl_ast::Ast;
use wdl_ast::Node;
use wdl_ast::Severity;
use wdl_ast::v1::ImportSource;
use wdl_format::Formatter;
use wdl_format::element::node::AstNodeFormatExt as _;
use wdl_modules::module::Module;
use wdl_modules::module::ModuleId;
use wdl_modules::symbolic_path::SymbolicPath;

use crate::AnalysisResult;
use crate::IncrementalChange;
use crate::ProgressKind;
use crate::SourcePosition;
use crate::SourcePositionEncoding;
use crate::config::Config;
use crate::document::Document;
use crate::graph::DfsSpace;
use crate::graph::DocumentGraph;
use crate::graph::EdgeKind;
use crate::graph::ParseState;
use crate::handlers;
use crate::rayon::RayonHandle;

/// The minimum number of milliseconds between analysis progress reports.
const MINIMUM_PROGRESS_MILLIS: u128 = 50;

/// The maximum number of symbolic-import `materialize` calls allowed to run
/// concurrently. This bounds the parallel git clones and fetches a single
/// document's imports can trigger so that a manifest with many dependencies
/// cannot exhaust file descriptors, disk, or network during analysis.
const MAX_CONCURRENT_MATERIALIZATIONS: usize = 8;

/// Represents a request to the analysis queue.
pub enum Request<Context> {
    /// A request to add documents to the graph.
    Add(AddRequest),
    /// A request to analyze documents.
    Analyze(AnalyzeRequest<Context>),
    /// A request to get all callers of a symbol.
    CallHierarchy(CallHierarchyRequest),
    /// A request to remove documents from the graph.
    Remove(RemoveRequest),
    /// A request to process a document's incremental change.
    NotifyIncrementalChange(NotifyIncrementalChangeRequest),
    /// A request to process a document's change.
    NotifyChange(NotifyChangeRequest),
    /// A request to get all folding ranges in a document.
    FoldingRange(FoldingRangeRequest),
    /// A request to format a document.
    Format(FormatRequest),
    /// A request to goto definition of a symbol.
    GotoDefinition(GotoDefinitionRequest),
    /// A request to find all references of a symbol.
    FindAllReferences(FindAllReferencesRequest),
    /// A request to get completions at a position.
    Completion(CompletionRequest<Context>),
    /// A request to get information about a symbol on hover.
    Hover(HoverRequest),
    /// A request to rename a symbol workspace wide.
    Rename(RenameRequest),
    /// A request to get semantic tokens for a document.
    SemanticTokens(SemanticTokenRequest),
    /// A request to get symbols for a document.
    DocumentSymbol(DocumentSymbolRequest),
    /// A request to get symbols for the workspace.
    WorkspaceSymbol(WorkspaceSymbolRequest),
    /// A request to get all incoming calls from a symbol.
    IncomingCalls(IncomingCallsRequest),
    /// A request to get all outgoing calls from a symbol.
    OutgoingCalls(OutgoingCallsRequest),
    /// A request to get signature help.
    SignatureHelp(SignatureHelpRequest),
    /// A request to get inlay hints for a document.
    InlayHints(InlayHintsRequest),
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

/// Represents a request to get the call hierarchy for a symbol.
pub struct CallHierarchyRequest {
    /// The document to search for the symbol definition.
    pub document: Url,
    /// The position of the symbol in the document.
    pub position: SourcePosition,
    /// The encoding used for the position.
    pub encoding: SourcePositionEncoding,
    /// The sender for completing the request.
    pub completed: oneshot::Sender<Option<Vec<CallHierarchyItem>>>,
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

/// Represents a request to get all folding ranges in a document.
pub struct FoldingRangeRequest {
    /// The document to get folding ranges for.
    pub document: Url,
    /// The sender for completing the request.
    pub completed: oneshot::Sender<Option<Vec<FoldingRange>>>,
}

/// Represents a request to format a document.
pub struct FormatRequest {
    /// The document to be formatted.
    pub document: Url,
    /// The sender for completing the request.
    ///
    /// The return type is an option format result, meaning (in order):
    ///
    /// * The line of the last character in the document,
    /// * The column of the last character in the document, and
    /// * The formatted document to replace the entire file with.
    pub completed: oneshot::Sender<Option<(u32, u32, String)>>,
}

/// Represents a request to find the definition of a symbol at a given position.
pub struct GotoDefinitionRequest {
    /// The document to search for the symbol definition.
    pub document: Url,
    /// The position of the symbol in the document.
    pub position: SourcePosition,
    /// The encoding used for the position.
    pub encoding: SourcePositionEncoding,
    /// The sender for completing the request.
    pub completed: oneshot::Sender<Option<GotoDefinitionResponse>>,
}

/// Represents a request to find all references to a symbol at a given position.
pub struct FindAllReferencesRequest {
    /// The document where the request was initiated.
    pub document: Url,
    /// The position of the symbol in the document.
    pub position: SourcePosition,
    /// The encoding used for the position.
    pub encoding: SourcePositionEncoding,
    /// Whether to include the declaration in the results.
    pub include_declaration: bool,
    /// The sender for completing the request.
    pub completed: oneshot::Sender<Vec<Location>>,
}

/// Represents a request to get completions.
pub struct CompletionRequest<Context> {
    /// The document where the request was initiated.
    pub document: Url,
    /// The position of the symbol in the document.
    pub position: SourcePosition,
    /// The encoding used for the position.
    pub encoding: SourcePositionEncoding,
    /// The sender for completing the request.
    pub completed: oneshot::Sender<Option<CompletionResponse>>,
    /// The context to provide to the progress callback.
    pub context: Context,
}

/// Represents a request to get information of a symbol on hover
pub struct HoverRequest {
    /// The document where the request was initiated.
    pub document: Url,
    /// The position of the symbol in the document.
    pub position: SourcePosition,
    /// The encoding used for the position.
    pub encoding: SourcePositionEncoding,
    /// The sender for completing the request.
    pub completed: oneshot::Sender<Option<Hover>>,
}

/// Represents a request to rename a symbol at a given position.
pub struct RenameRequest {
    /// The document where the request was initiated.
    pub document: Url,
    /// The position of the symbol in the document.
    pub position: SourcePosition,
    /// The encoding used for the position.
    pub encoding: SourcePositionEncoding,
    /// The new name of the symbol.
    pub new_name: String,
    /// The sender for completing the request.
    pub completed: oneshot::Sender<Option<WorkspaceEdit>>,
}

/// Represents a request to get the semantic tokens for a document
pub struct SemanticTokenRequest {
    /// The document to get semantic tokens for
    pub document: Url,
    /// The sender for completing the request.
    pub completed: oneshot::Sender<Option<SemanticTokensResult>>,
}

/// Represents a request to get the symbols for a document.
pub struct DocumentSymbolRequest {
    /// The document to get symbols for
    pub document: Url,
    /// The sender for completing the request.
    pub completed: oneshot::Sender<Option<DocumentSymbolResponse>>,
}

/// Represents a request to get symbols for the workspace.
pub struct WorkspaceSymbolRequest {
    /// The query string to filter symbols.
    pub query: String,
    /// The sender for completing the request.
    pub completed: oneshot::Sender<Option<Vec<SymbolInformation>>>,
}

/// Represents a request to get the incoming calls for a symbol.
pub struct IncomingCallsRequest {
    /// The document to search for the symbol definition.
    pub document: Url,
    /// The position of the symbol in the document.
    pub position: SourcePosition,
    /// The encoding used for the position.
    pub encoding: SourcePositionEncoding,
    /// The sender for completing the request.
    pub completed: oneshot::Sender<Option<Vec<CallHierarchyIncomingCall>>>,
}

/// Represents a request to get the outgoing calls for a symbol.
pub struct OutgoingCallsRequest {
    /// The document to search for the symbol definition.
    pub document: Url,
    /// The position of the symbol in the document.
    pub position: SourcePosition,
    /// The encoding used for the position.
    pub encoding: SourcePositionEncoding,
    /// The sender for completing the request.
    pub completed: oneshot::Sender<Option<Vec<CallHierarchyOutgoingCall>>>,
}

/// Represents a request for signature help.
pub struct SignatureHelpRequest {
    /// The document where the request was initiated.
    pub document: Url,
    /// The position of the cursor in the document.
    pub position: SourcePosition,
    /// The encoding used for the position.
    pub encoding: SourcePositionEncoding,
    /// The sender for completing the request.
    pub completed: oneshot::Sender<Option<SignatureHelp>>,
}

/// Represents a request for inlay hints.
pub struct InlayHintsRequest {
    /// The document where the request was initiated.
    pub document: Url,
    /// The visible range for which inlay hints should be computed.
    pub range: lsp_types::Range,
    /// The sender for completing the request.
    pub completed: oneshot::Sender<Option<Vec<InlayHint>>>,
}

/// A simple enumeration to signal a cancellation to the caller.
enum Cancelable<T> {
    /// The operation completed and yielded a value.
    Completed(T),
    /// The operation was canceled.
    Canceled,
}

/// Maps document URIs to the [`Module`] that governs each.
///
/// This is the single place the analysis queue records and looks up module
/// context for a document, so the locking discipline lives behind one API
/// rather than being spread across the queue's methods.
#[derive(Default)]
struct ModuleRegistry {
    /// The URI-to-module map guarded for concurrent access during analysis.
    ///
    /// Modules are stored behind an [`Arc`] so the many documents governed by
    /// the same module share one instance and lookups hand out cheap clones
    /// rather than copying a module's path and scope each time.
    modules: parking_lot::Mutex<HashMap<Url, Arc<Module>>>,
}

impl ModuleRegistry {
    /// Returns the [`Module`] governing the document at `uri`, if recorded.
    fn module_for(&self, uri: &Url) -> Option<Arc<Module>> {
        self.modules.lock().get(uri).cloned()
    }

    /// Records many document-to-module mappings under a single lock
    /// acquisition, avoiding per-item lock churn in tight loops.
    fn record_all(&self, entries: impl IntoIterator<Item = (Url, Arc<Module>)>) {
        self.modules.lock().extend(entries);
    }

    /// Drops mappings whose document is no longer present, keeping the map from
    /// growing without bound across edit and remove cycles.
    fn retain(&self, mut keep: impl FnMut(&Url) -> bool) {
        self.modules.lock().retain(|uri, _| keep(uri));
    }
}

/// Symbolic import work collected for a pass, deduplicated by module identity
/// and symbolic path so each distinct dependency is materialized once.
type SymbolicWorkSet = IndexMap<(ModuleId, SymbolicPath), MaterializeWork>;

/// A deduplicated unit of materialization work shared by every importer that
/// requested the same dependency.
struct MaterializeWork {
    /// The nodes that contain an import of this dependency.
    importers: Vec<NodeIndex>,
    /// The consumer module captured during collection. Reused to build the
    /// child module for the materialized dependency.
    consumer_module: Arc<Module>,
    /// The parsed symbolic path from the import statement.
    symbolic_path: SymbolicPath,
    /// The raw text of the module-path token, used as the edge label and for
    /// error messages.
    path_text: String,
}

/// The result of materializing one deduplicated dependency, carried back to the
/// graph-stitching step.
struct MaterializeOutcome {
    /// The nodes that contain an import of this dependency.
    importers: Vec<NodeIndex>,
    /// The consumer module captured during collection.
    consumer_module: Arc<Module>,
    /// The parsed symbolic path from the import statement.
    symbolic_path: SymbolicPath,
    /// The raw text of the module-path token, used as the edge label and for
    /// error messages.
    path_text: String,
    /// The result of the `materialize` call.
    result: Result<wdl_modules::resolver::MaterializedFile, wdl_modules::resolver::ResolverError>,
}

/// Represents the analysis queue.
pub struct AnalysisQueue<Progress, Context, Return, Validator> {
    /// The document graph maintained by the analysis queue.
    graph: Arc<RwLock<DocumentGraph>>,
    /// The configuration to use.
    config: Config,
    /// The handle to the tokio runtime for blocking on async tasks.
    tokio: Handle,
    /// The HTTP client to use for fetching documents.
    client: Client,
    /// The module resolver used for resolving WDL module imports.
    ///
    /// `None` when module resolution is disabled; in that case no symbolic
    /// import work is ever collected, so the resolver is never needed.
    resolver: Option<Arc<dyn wdl_modules::Resolver>>,
    /// The consumer's [`Module`], if a `module.json` was found.
    consumer_module: Option<Arc<Module>>,
    /// Maps each document URI to the [`Module`] that governs it.
    /// Populated at resolution time so the lookup is direct.
    document_modules: ModuleRegistry,
    /// Caches whether a directory is a module root so repeated ancestry walks
    /// during import scanning do not re-stat the filesystem for the same path.
    module_root_cache: parking_lot::Mutex<HashMap<PathBuf, bool>>,
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
    Validator: Fn() -> crate::Validator + Send + Sync + 'static,
{
    /// Constructs a new analysis queue.
    pub fn new(
        config: Config,
        tokio: Handle,
        resolution: crate::ResolutionContext,
        progress: Progress,
        validator: Validator,
    ) -> Self {
        // The consumer module was loaded once when the resolution context was
        // built, so reuse it here instead of re-reading `module.json`. Wrap it
        // in an `Arc` so the many documents it governs share one instance.
        let (resolver, consumer_module) = resolution.into_parts();
        let consumer_module = consumer_module.map(Arc::new);

        Self {
            graph: Arc::new(RwLock::new(DocumentGraph::new(config.clone()))),
            config,
            tokio,
            resolver,
            consumer_module,
            document_modules: ModuleRegistry::default(),
            module_root_cache: parking_lot::Mutex::new(HashMap::new()),
            progress: Arc::new(progress),
            marker: PhantomData,
            client: Default::default(),
            validator: Arc::new(validator),
        }
    }

    /// Runs the analysis queue.
    pub fn run(&self, mut receiver: UnboundedReceiver<Request<Context>>) {
        debug!("analysis queue has started");

        while let Some(request) = self.tokio.block_on(receiver.recv()) {
            match request {
                Request::Add(AddRequest {
                    documents,
                    completed,
                }) => {
                    let start = Instant::now();
                    debug!(
                        "received request to add {count} document(s) to the graph",
                        count = documents.len()
                    );

                    self.add_documents(documents);

                    debug!(
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
                        debug!("received request to analyze document `{document}`");
                    } else {
                        debug!("received request to analyze all documents");
                    }

                    match self.analyze(document, context, Some(&completed)) {
                        Cancelable::Completed(results) => {
                            debug!(
                                "request to analyze documents completed in {elapsed:?}",
                                elapsed = start.elapsed()
                            );

                            completed.send(results).ok();
                        }
                        Cancelable::Canceled => {
                            debug!(
                                "request to analyze documents was canceled after {elapsed:?}",
                                elapsed = start.elapsed()
                            );
                        }
                    }
                }
                Request::CallHierarchy(CallHierarchyRequest {
                    document,
                    position,
                    encoding,
                    completed,
                }) => {
                    let start = Instant::now();
                    debug!(
                        "received request for call hierarchy at {document}: {line}:{char}",
                        line = position.line,
                        char = position.character
                    );

                    let graph = self.graph.read();
                    match handlers::call_hierarchy(&graph, document, position, encoding) {
                        Ok(result) => {
                            debug!(
                                "call hierarchy request completed in {elapsed:?}",
                                elapsed = start.elapsed()
                            );

                            completed.send(result).ok();
                        }
                        Err(err) => {
                            error!(
                                "error occurred while completing the call hierarchy request: \
                                 {err:?}"
                            );
                            completed.send(None).ok();
                        }
                    }
                }
                Request::Remove(RemoveRequest {
                    documents,
                    completed,
                }) => {
                    let start = Instant::now();
                    debug!(
                        "received request to remove {count} documents(s)",
                        count = documents.len()
                    );

                    self.remove_documents(documents);

                    debug!(
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
                Request::FoldingRange(FoldingRangeRequest {
                    document,
                    completed,
                }) => {
                    let start = Instant::now();

                    let graph = self.graph.read();
                    match handlers::folding_range(&graph, document) {
                        Ok(result) => {
                            debug!(
                                "folding range request completed in {elapsed:?}",
                                elapsed = start.elapsed()
                            );

                            completed.send(Some(result)).ok();
                        }
                        Err(err) => {
                            error!(
                                "error occurred while completing the folding range request: \
                                 {err:?}"
                            );
                            completed.send(None).ok();
                        }
                    }
                }
                Request::Format(FormatRequest {
                    document,
                    completed,
                }) => {
                    let graph = self.graph.read();

                    let result = graph
                        .get_index(&document)
                        .and_then(|index| {
                            graph.get(index).root().and_then(|document| {
                                match graph.get(index).parse_state() {
                                    // NOTE: if we haven't parsed the document yet, then
                                    // we don't have the line lengths of the document,
                                    // so we can't proceed with formatting and we should
                                    // just silently return.
                                    ParseState::NotParsed | ParseState::Error(_) => None,
                                    ParseState::Parsed {
                                        lines, diagnostics, ..
                                    } => {
                                        // If there are any diagnostics that are
                                        // errors, we shouldn't attempt to format the
                                        // document.
                                        if diagnostics
                                            .iter()
                                            .any(|d| d.severity() == Severity::Error)
                                        {
                                            return None;
                                        }

                                        let line_col = lines.line_col(lines.len());
                                        Some((line_col.line, line_col.col, document))
                                    }
                                }
                            })
                        })
                        .and_then(|(line, col, document)| {
                            document
                                .ast_with_version_fallback(self.config.fallback_version())
                                .into_v1()
                                .and_then(|ast| {
                                    let formatter = Formatter::default();
                                    let element = Node::Ast(ast).into_format_element();

                                    formatter
                                        .format(&element)
                                        .ok()
                                        .map(|formatted| (line, col, formatted))
                                })
                        });

                    completed.send(result).ok();
                }
                Request::GotoDefinition(GotoDefinitionRequest {
                    document,
                    position,
                    encoding,
                    completed,
                }) => {
                    let start = Instant::now();
                    debug!(
                        "received request for goto definition at {document}: {line}:{char}",
                        line = position.line,
                        char = position.character
                    );

                    let graph = self.graph.read();
                    match handlers::goto_definition(&graph, &document, position, encoding) {
                        Ok(result) => {
                            debug!(
                                "goto definition request completed in {elapsed:?}",
                                elapsed = start.elapsed()
                            );

                            let location = result.map(GotoDefinitionResponse::Scalar);
                            completed.send(location).ok();
                        }
                        Err(err) => {
                            error!(
                                "error occurred while completing the goto definition request: \
                                 {err:?}"
                            );
                            completed.send(None).ok();
                        }
                    }
                }
                Request::FindAllReferences(FindAllReferencesRequest {
                    document,
                    position,
                    encoding,
                    include_declaration,
                    completed,
                }) => {
                    let start = Instant::now();
                    debug!(
                        "received request for find all references at {document}: {line}:{char}",
                        line = position.line,
                        char = position.character
                    );

                    let graph = self.graph.read();
                    match handlers::find_all_references(
                        &graph,
                        &document,
                        position,
                        encoding,
                        include_declaration,
                    ) {
                        Ok(result) => {
                            debug!(
                                "find all references request completed in {elapsed:?}",
                                elapsed = start.elapsed()
                            );

                            completed.send(result).ok();
                        }
                        Err(err) => {
                            error!("find all references request failed: {err:?}");
                            completed.send(vec![]).ok();
                        }
                    }
                }
                Request::Completion(CompletionRequest {
                    document,
                    position,
                    context,
                    encoding,
                    completed,
                }) => {
                    let start = Instant::now();
                    debug!(
                        "received request for completion at {document}: {line}:{char}",
                        line = position.line,
                        char = position.character
                    );

                    if let Cancelable::Completed(Err(e)) =
                        self.analyze(Some(document.clone()), context, None)
                    {
                        error!("analysis failed before completion could run: {e}");
                        completed.send(None).ok();
                        continue;
                    }

                    let graph = self.graph.read();
                    let result = handlers::completion(&graph, &document, position, encoding)
                        .map(|items| Some(CompletionResponse::Array(items)));

                    debug!(
                        "completion request completed in {elapsed:?}",
                        elapsed = start.elapsed()
                    );

                    match result {
                        Ok(result) => {
                            completed.send(result).ok();
                        }
                        Err(_) => {
                            completed.send(None).ok();
                        }
                    }
                }
                Request::Hover(HoverRequest {
                    document,
                    position,
                    encoding,
                    completed,
                }) => {
                    let start = Instant::now();
                    debug!(
                        "received request for hover at {document}: {line}:{char}",
                        line = position.line,
                        char = position.character
                    );

                    let graph = self.graph.read();

                    match handlers::hover(&graph, &document, position, encoding) {
                        Ok(result) => {
                            debug!(
                                "hover request completed in {elapsed:?}",
                                elapsed = start.elapsed()
                            );

                            completed.send(result).ok();
                        }
                        Err(err) => {
                            error!("hover request failed: {err:?}");
                            completed.send(None).ok();
                        }
                    }
                }

                Request::Rename(RenameRequest {
                    document,
                    position,
                    encoding,
                    new_name,
                    completed,
                }) => {
                    let start = Instant::now();
                    debug!(
                        "received request for rename at {document}: {line}:{char}",
                        line = position.line,
                        char = position.character
                    );

                    let graph = self.graph.read();
                    match handlers::rename(&graph, &document, position, encoding, new_name) {
                        Ok(result) => {
                            debug!(
                                "rename request completed in {elapsed:?}",
                                elapsed = start.elapsed()
                            );
                            completed.send(result).ok();
                        }
                        Err(err) => {
                            error!("rename request failed: {err:?}");
                            completed.send(None).ok();
                        }
                    }
                }

                Request::SemanticTokens(SemanticTokenRequest {
                    document,
                    completed,
                }) => {
                    let start = Instant::now();
                    debug!("received request for semantic tokens for {document}");

                    let graph = self.graph.read();
                    match handlers::semantic_tokens(&graph, &document) {
                        Ok(result) => {
                            debug!(
                                "semantic tokens request completed in {elapsed:?}",
                                elapsed = start.elapsed()
                            );

                            let tokens = result.map(SemanticTokensResult::Tokens);
                            completed.send(tokens).ok();
                        }
                        Err(err) => {
                            error!("semantic tokens request failed: {err:?}");
                            completed.send(None).ok();
                        }
                    }
                }

                Request::DocumentSymbol(DocumentSymbolRequest {
                    document,
                    completed,
                }) => {
                    let start = Instant::now();
                    debug!("received request for document symbols for {document}");

                    let parse_result;
                    {
                        let graph = self.graph.read();
                        let Some(index) = graph.get_index(&document) else {
                            debug!("document '{document}' not found in graph");
                            completed.send(None).ok();
                            continue;
                        };
                        let node = graph.get(index);

                        if node.needs_parse() {
                            parse_result = Some(node.parse(&self.tokio, &self.client));
                        } else {
                            parse_result = None;
                        }
                    }

                    match parse_result {
                        Some(Ok(state)) => {
                            let mut graph = self.graph.write();
                            let index = graph.get_index(&document).unwrap();
                            graph.get_mut(index).parse_completed(state);
                        }
                        Some(Err(e)) => {
                            debug!(
                                "error occurred while parsing document in document symbol \
                                 request: {e:?}"
                            );
                            completed.send(None).ok();
                            continue;
                        }
                        None => {}
                    }

                    let graph = self.graph.read();
                    match handlers::document_symbol(&graph, &document) {
                        Ok(result) => {
                            debug!(
                                "document symbol request completed in {elapsed:?}",
                                elapsed = start.elapsed()
                            );
                            completed.send(result).ok();
                        }
                        Err(err) => {
                            error!("document symbol request failed: {err:?}");
                            completed.send(None).ok();
                        }
                    }
                }
                Request::WorkspaceSymbol(WorkspaceSymbolRequest { query, completed }) => {
                    let start = Instant::now();
                    debug!("received request for workspace symbols with query `{query}`");

                    let graph = self.graph.read();
                    match handlers::workspace_symbol(&graph, &query) {
                        Ok(result) => {
                            debug!(
                                "workspace symbol request completed in {elapsed:?}",
                                elapsed = start.elapsed()
                            );
                            completed.send(result).ok();
                        }
                        Err(err) => {
                            error!("workspace symbol request failed: {err:?}");
                            completed.send(None).ok();
                        }
                    }
                }
                Request::IncomingCalls(IncomingCallsRequest {
                    document,
                    position,
                    encoding,
                    completed,
                }) => {
                    let start = Instant::now();
                    debug!(
                        "received request for incoming calls at {document}: {line}:{char}",
                        line = position.line,
                        char = position.character
                    );

                    let graph = self.graph.read();
                    match handlers::incoming_calls(&graph, &document, position, encoding) {
                        Ok(result) => {
                            debug!(
                                "incoming calls request completed in {elapsed:?}",
                                elapsed = start.elapsed()
                            );

                            completed.send(result).ok();
                        }
                        Err(err) => {
                            error!(
                                "error occurred while completing the incoming calls request: \
                                 {err:?}"
                            );
                            completed.send(None).ok();
                        }
                    }
                }
                Request::OutgoingCalls(OutgoingCallsRequest {
                    document,
                    position,
                    encoding,
                    completed,
                }) => {
                    let start = Instant::now();
                    debug!(
                        "received request for outgoing calls at {document}: {line}:{char}",
                        line = position.line,
                        char = position.character
                    );

                    let graph = self.graph.read();
                    match handlers::outgoing_calls(&graph, &document, position, encoding) {
                        Ok(result) => {
                            debug!(
                                "outgoing calls request completed in {elapsed:?}",
                                elapsed = start.elapsed()
                            );

                            completed.send(result).ok();
                        }
                        Err(err) => {
                            error!(
                                "error occurred while completing the outgoing calls request: \
                                 {err:?}"
                            );
                            completed.send(None).ok();
                        }
                    }
                }
                Request::SignatureHelp(SignatureHelpRequest {
                    document,
                    position,
                    encoding,
                    completed,
                }) => {
                    let start = Instant::now();
                    debug!(
                        "received request for signature help at {document}: {line}:{char}",
                        line = position.line,
                        char = position.character
                    );

                    let graph = self.graph.read();
                    match handlers::signature_help(&graph, &document, position, encoding) {
                        Ok(result) => {
                            debug!(
                                "signature help request completed in {elapsed:?}",
                                elapsed = start.elapsed()
                            );
                            completed.send(result).ok();
                        }
                        Err(err) => {
                            error!("signature help request failed: {err:?}");
                            completed.send(None).ok();
                        }
                    }
                }
                Request::InlayHints(InlayHintsRequest {
                    document,
                    range,
                    completed,
                }) => {
                    let start = Instant::now();
                    debug!("received request for inlay hints at {document}");

                    let graph = self.graph.read();
                    match handlers::inlay_hints(&graph, &document, range) {
                        Ok(result) => {
                            debug!(
                                "inlay hints request completed in {elapsed:?}",
                                elapsed = start.elapsed()
                            );
                            completed.send(result).ok();
                        }
                        Err(err) => {
                            error!("inlay hints request failed: {err:?}");
                            completed.send(None).ok();
                        }
                    }
                }
            }
        }

        debug!("analysis queue has shut down");
    }

    /// Adds a set of documents to the document graph.
    fn add_documents(&self, documents: IndexSet<Url>) {
        let mut graph = self.graph.write();
        let mut modules = Vec::new();
        for document in documents {
            if let Some(module) = self.module_for_root_document(&document) {
                modules.push((document.clone(), module));
            }
            graph.add_node(document, true);
        }
        self.document_modules.record_all(modules);
    }

    /// Analyzes the requested documents.
    fn analyze(
        &self,
        document: Option<Url>,
        context: Context,
        completed: Option<&oneshot::Sender<Result<Vec<AnalysisResult>>>>,
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
            if completed.is_some_and(|c| c.is_closed()) {
                debug!("analysis request has been canceled");
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
            if completed.is_some_and(|c| c.is_closed()) {
                debug!("analysis request has been canceled");
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

                let handles = FuturesUnordered::new();
                for index in set.iter().copied() {
                    let node = graph.get(index);
                    if node.document().is_some() {
                        if graph.include_result(index) {
                            results.push(AnalysisResult::new(node));
                        }
                        continue;
                    }

                    let graph = self.graph.clone();
                    let config = self.config.clone();
                    let validator = self.validator.clone();
                    handles.push(RayonHandle::spawn(move || {
                        let result = panic::catch_unwind(AssertUnwindSafe(|| {
                            Self::analyze_node(&config, graph.clone(), index, &mut (validator)())
                        }));

                        let mut graph = graph.write();
                        let node = graph.get_mut(index);
                        match result {
                            Ok((_, document)) => {
                                node.analysis_completed(document);
                                (index, Ok(()))
                            }
                            Err(payload) => {
                                let error = Arc::new(anyhow!(
                                    "analysis panicked for {uri}: {msg}",
                                    uri = node.uri(),
                                    msg = format_panic_payload(&payload)
                                ));
                                error!("{error}");

                                node.analysis_failed(error.clone());
                                (index, Err(error))
                            }
                        }
                    }))
                }

                handles
            };

            let analyzed =
                match self.await_with_progress(ProgressKind::Analyzing, tasks, completed, &context)
                {
                    Cancelable::Completed(analyzed) => analyzed,
                    Cancelable::Canceled => return Cancelable::Canceled,
                };

            let graph = self.graph.write();
            results.extend(analyzed.into_iter().filter_map(|(index, _)| {
                // the node state was already updated within the Rayon task.
                if graph.include_result(index) {
                    Some(AnalysisResult::new(graph.get(index)))
                } else {
                    None
                }
            }));
        }

        results.sort_by(|a, b| a.document().uri().cmp(b.document().uri()));
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

        // Drop module mappings for any document the collection removed from
        // the graph so the map does not grow unbounded across edit and remove
        // cycles in a long-lived session.
        self.document_modules
            .retain(|uri| graph.get_index(uri).is_some());

        // Clear the module-root probe cache on a remove cycle so it cannot grow
        // without bound across a long-lived session; it refills lazily from the
        // filesystem on the next ancestry walk.
        self.module_root_cache.lock().clear();
    }

    /// Awaits the given set of futures while providing progress to the given
    /// callback.
    fn await_with_progress<Fut, Output>(
        &self,
        kind: ProgressKind,
        mut tasks: FuturesUnordered<Fut>,
        completed: Option<&oneshot::Sender<Result<Vec<AnalysisResult>>>>,
        context: &Context,
    ) -> Cancelable<Vec<Output>>
    where
        Fut: Future<Output = Output>,
    {
        if tasks.is_empty() {
            return Cancelable::Completed(Vec::new());
        }

        let total = tasks.len();
        if completed.is_some() {
            self.tokio
                .block_on((self.progress)(context.clone(), kind, 0, total));
        }

        let update_progress = self.progress.clone();
        let results = self.tokio.block_on(async move {
            let mut count = 0;
            let mut results = Vec::new();
            let mut last_progress = Instant::now();
            while let Some(result) = tasks.next().await {
                if completed.is_some_and(|c| c.is_closed()) {
                    break;
                }

                results.push(result);
                count += 1;

                if completed.is_some() {
                    let now = Instant::now();
                    if count < total && (now - last_progress).as_millis() > MINIMUM_PROGRESS_MILLIS
                    {
                        debug!("{count} out of {total} {kind} task(s) have completed");
                        last_progress = now;
                        update_progress(context.clone(), kind, count, total).await;
                    }
                }
            }

            results
        });

        if completed.is_some() {
            if results.len() < total {
                debug!(
                    "{count} out of {total} {kind} task(s) have completed; canceled {canceled} \
                     tasks",
                    count = results.len(),
                    canceled = total - results.len()
                );
            } else {
                debug!(
                    "{count} out of {total} {kind} task(s) have completed",
                    count = results.len()
                );
            }

            // Report all have completed even if there are cancellations
            self.tokio
                .block_on((self.progress)(context.clone(), kind, total, total));
        }

        if completed.is_some_and(|c| c.is_closed()) {
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
        RayonHandle::spawn(move || {
            let graph = graph.read();
            let node = graph.get(index);
            let state = node.parse(&tokio, &client);
            (index, state)
        })
    }

    /// Updates the graph and subgraphs for the given parsed nodes.
    ///
    /// This runs in three steps; collect the import work for the parsed nodes,
    /// materialize symbolic imports concurrently, then apply the results back
    /// into the graph and queue dependents for reanalysis.
    fn update_graphs(
        &self,
        parsed: Vec<(NodeIndex, Result<ParseState>)>,
        subgraph: &mut IndexSet<NodeIndex>,
        range: Range<usize>,
        space: &mut DfsSpace,
    ) -> Result<()> {
        let (parsed_indices, symbolic_work) = self.collect_import_work(parsed, subgraph, space)?;
        let results = self.materialize_symbolic_imports(symbolic_work);
        self.apply_materialization_results(results, &parsed_indices, subgraph, range, space);
        Ok(())
    }

    /// Collects import work for the parsed nodes under a single graph write.
    ///
    /// URI imports are wired directly into the graph here; symbolic imports are
    /// returned as work items so their I/O can run outside the lock. Returns
    /// the parsed node indices and the symbolic work to materialize.
    fn collect_import_work(
        &self,
        parsed: Vec<(NodeIndex, Result<ParseState>)>,
        subgraph: &mut IndexSet<NodeIndex>,
        space: &mut DfsSpace,
    ) -> Result<(Vec<NodeIndex>, SymbolicWorkSet)> {
        // Handle parse completion and URI imports under graph.write(). Symbolic
        // imports are collected (and deduplicated by module identity plus
        // symbolic path) for concurrent materialization outside the lock, so the
        // write lock is held for as short a time as possible.
        let mut uri_import_modules: Vec<(Url, Arc<Module>)> = Vec::new();
        let (parsed_indices, symbolic_work): (Vec<NodeIndex>, SymbolicWorkSet) = {
            let mut graph = self.graph.write();
            let mut indices = Vec::new();
            let mut work = IndexMap::new();

            for (index, state) in parsed {
                let node = graph.get_mut(index);
                let state = state.with_context(|| {
                    format!("failed to parse document `{uri}`", uri = node.uri())
                })?;
                node.parse_completed(state);

                // Remove all dependency edges from the node as the imports
                // might have changed, and clear any stale
                // failed-symbolic-import diagnostics at the same time so they
                // do not survive into the re-analysis pass.
                graph.remove_dependency_edges(index);
                graph.get_mut(index).clear_failed_symbolic_imports();

                // Add back dependency edges for URI imports; queue symbolic
                // imports for concurrent materialization.
                match graph
                    .get(index)
                    .root()
                    .map(|d| d.ast_with_version_fallback(self.config.fallback_version()))
                {
                    None | Some(Ast::Unsupported) => {}
                    Some(Ast::V1(ast)) => {
                        let symbolic_imports_enabled = graph
                            .get(index)
                            .parse_state()
                            .symbolic_imports_enabled(&self.config);
                        for import in ast.imports() {
                            match import.source() {
                                ImportSource::Uri(uri) => {
                                    let text = match uri.text() {
                                        Some(text) => text,
                                        None => continue,
                                    };

                                    let import_uri = match graph.get(index).uri().join(text.text())
                                    {
                                        Ok(uri) => uri,
                                        Err(_) => continue,
                                    };

                                    // Only probe module ownership when a
                                    // consumer module exists; without one no
                                    // document is governed by a module, so the
                                    // registry lookup would always miss.
                                    if self.consumer_module.is_some()
                                        && let Some(module) = self.module_for_uri_import(
                                            graph.get(index).uri(),
                                            &import_uri,
                                        )
                                    {
                                        uri_import_modules.push((import_uri.clone(), module));
                                    }

                                    let import_index = graph
                                        .get_index(&import_uri)
                                        .unwrap_or_else(|| graph.add_node(import_uri, false));
                                    graph.add_dependency_edge(
                                        index,
                                        import_index,
                                        EdgeKind::Uri,
                                        space,
                                    );
                                    subgraph.insert(import_index);
                                }
                                ImportSource::ModulePath(module_path) => {
                                    if !symbolic_imports_enabled {
                                        continue;
                                    }

                                    let consumer_module = match self
                                        .find_module_for_document(graph.get(index).uri())
                                    {
                                        Some(m) => m,
                                        None => continue,
                                    };

                                    let symbolic_path: SymbolicPath =
                                        match module_path.text().parse() {
                                            Ok(path) => path,
                                            Err(e) => {
                                                // Record the syntax failure so the
                                                // import surfaces a precise diagnostic
                                                // instead of the generic "not in a
                                                // module" message during analysis.
                                                graph.insert_failed_symbolic_import(
                                                    index,
                                                    module_path.text().to_string(),
                                                    e.to_string(),
                                                );
                                                continue;
                                            }
                                        };

                                    // Collapse imports of the same dependency
                                    // from the same module into one
                                    // materialization, keyed on full module
                                    // identity plus the symbolic path, so each
                                    // dependency is resolved once and the result
                                    // fanned out to every importer.
                                    work.entry((consumer_module.id(), symbolic_path.clone()))
                                        .or_insert_with(|| MaterializeWork {
                                            importers: Vec::new(),
                                            consumer_module,
                                            symbolic_path,
                                            path_text: module_path.text().to_string(),
                                        })
                                        .importers
                                        .push(index);
                                }
                            }
                        }
                    }
                }

                indices.push(index);
            }

            (indices, work)
        };
        // graph write lock is released here.

        // Record URI-import module mappings collected above in one batch.
        self.document_modules.record_all(uri_import_modules);

        Ok((parsed_indices, symbolic_work))
    }

    /// Materializes the collected symbolic import work concurrently.
    ///
    /// The work arrives already deduplicated by module identity plus symbolic
    /// path, so each distinct dependency is materialized once and the result is
    /// fanned out to every importer that requested it. The calls run capped at
    /// `MAX_CONCURRENT_MATERIALIZATIONS` in flight so a manifest declaring many
    /// dependencies cannot drive an unbounded number of parallel clones or
    /// fetches.
    fn materialize_symbolic_imports(
        &self,
        unique_work: SymbolicWorkSet,
    ) -> Vec<MaterializeOutcome> {
        if unique_work.is_empty() {
            return Vec::new();
        }

        tracing::debug!(
            count = unique_work.len(),
            "resolving symbolic imports concurrently",
        );
        // SAFETY: symbolic work is only collected when a consumer module governs
        // a document, which only happens when resolution is enabled with a
        // resolver; a disabled context produces no work and returned above.
        let resolver = Arc::clone(self.resolver.as_ref().unwrap());
        let stream = futures::stream::iter(unique_work.into_values().map(|work| {
            let resolver = Arc::clone(&resolver);
            async move {
                tracing::debug!(path = %work.path_text, "resolving symbolic import");
                let result = resolver
                    .materialize(&work.consumer_module, &work.symbolic_path)
                    .await;
                MaterializeOutcome {
                    importers: work.importers,
                    consumer_module: work.consumer_module,
                    symbolic_path: work.symbolic_path,
                    path_text: work.path_text,
                    result,
                }
            }
        }))
        .buffer_unordered(MAX_CONCURRENT_MATERIALIZATIONS);

        self.tokio
            .block_on(stream.collect::<Vec<MaterializeOutcome>>())
    }

    /// Applies materialization results to the graph and queues dependents for
    /// reanalysis under a single graph write.
    ///
    /// Each materialized dependency is added to the graph once and connected to
    /// every importer that requested it. Because WDL implicitly introduces
    /// import names into document scope, every transitive dependent of a
    /// changed document is also queued for reanalysis.
    fn apply_materialization_results(
        &self,
        results: Vec<MaterializeOutcome>,
        parsed_indices: &[NodeIndex],
        subgraph: &mut IndexSet<NodeIndex>,
        range: Range<usize>,
        space: &mut DfsSpace,
    ) {
        let mut symbolic_import_modules: Vec<(Url, Arc<Module>)> = Vec::new();
        {
            let mut graph = self.graph.write();

            for MaterializeOutcome {
                importers,
                consumer_module,
                symbolic_path,
                path_text,
                result,
            } in results
            {
                match result {
                    Ok(materialized) => {
                        let import_uri = match Url::from_file_path(&materialized.path) {
                            Ok(u) => u,
                            Err(()) => {
                                let message = format!(
                                    "materialized path is not absolute: `{}`",
                                    materialized.path.display()
                                );
                                for importer in &importers {
                                    graph.insert_failed_symbolic_import(
                                        *importer,
                                        path_text.clone(),
                                        message.clone(),
                                    );
                                }
                                continue;
                            }
                        };

                        // Ask the resolved file for the module that owns it,
                        // extending the consumer module captured during
                        // collection. The queue does not reassemble module state
                        // from the file's raw manifest and root itself.
                        let import_module = materialized
                            .child_module(&consumer_module, symbolic_path.dep_name().clone());

                        symbolic_import_modules.push((import_uri.clone(), Arc::new(import_module)));

                        // Materialization happens once per dependency, so add
                        // the node once and connect every importer that
                        // requested it.
                        let import_index = graph
                            .get_index(&import_uri)
                            .unwrap_or_else(|| graph.add_node(import_uri.clone(), false));
                        for importer in &importers {
                            graph.add_dependency_edge(
                                *importer,
                                import_index,
                                EdgeKind::Symbolic(path_text.clone()),
                                space,
                            );
                        }
                        subgraph.insert(import_index);
                    }
                    Err(e) => {
                        let message = e.to_string();
                        for importer in &importers {
                            graph.insert_failed_symbolic_import(
                                *importer,
                                path_text.clone(),
                                message.clone(),
                            );
                        }
                    }
                }
            }

            // Because of the way WDL works by implicitly introducing import
            // names into document scope, any change to a file must cause all
            // transitive dependents to be reanalyzed; therefore, do a BFS
            // from each parsed node and add any discovered nodes to the
            // subgraph.
            for index in parsed_indices {
                let index = *index;
                graph.bfs_mut(index, |graph, dependent: NodeIndex| {
                    if index == dependent {
                        return;
                    }

                    let node = graph.get_mut(dependent);
                    if !subgraph.contains(&dependent) {
                        trace!(
                            "adding dependent document `{uri}` for analysis",
                            uri = node.uri()
                        );
                        subgraph.insert(dependent);
                    }

                    node.reanalyze();
                });
            }

            // Add the direct dependencies of the subgraph slice to the
            // subgraph.
            let mut dependencies = Vec::new();
            for index in subgraph.get_range(range).expect("range should be valid") {
                dependencies.extend(graph.dependencies(*index));
            }

            subgraph.extend(dependencies);
        }

        // Record symbolic-import module mappings collected above in one batch
        // now that the graph write lock has been released.
        self.document_modules.record_all(symbolic_import_modules);
    }

    /// Analyzes a node in the document graph.
    fn analyze_node(
        config: &Config,
        graph: Arc<RwLock<DocumentGraph>>,
        index: NodeIndex,
        validator: &mut crate::Validator,
    ) -> (NodeIndex, Document) {
        let start = Instant::now();
        let graph = graph.read();
        let mut document = Document::from_graph_node(config, &graph, index);

        match &graph.get(index).parse_state() {
            ParseState::Parsed { diagnostics, .. }
                if !diagnostics.iter().any(|diag| diag.severity().is_error()) =>
            {
                if let Err(new_diagnostics) = validator.validate(&document, config) {
                    document.extend_diagnostics(new_diagnostics);
                }
            }
            _ => {}
        }
        document.sort_diagnostics();

        debug!(
            "analysis of `{uri}` completed in {elapsed:?}",
            uri = graph.get(index).uri(),
            elapsed = start.elapsed()
        );

        (index, document)
    }

    /// Returns the [`Module`] that governs the document at `uri`.
    fn find_module_for_document(&self, uri: &Url) -> Option<Arc<Module>> {
        self.document_modules.module_for(uri)
    }

    /// Returns the consumer module for a root document governed by it.
    fn module_for_root_document(&self, uri: &Url) -> Option<Arc<Module>> {
        let module = self.consumer_module.clone()?;
        let path = uri.to_file_path().ok()?;
        self.module_if_path_within_root(module, &path)
    }

    /// Returns the importer module for a URI import inside the same module.
    fn module_for_uri_import(&self, importer_uri: &Url, import_uri: &Url) -> Option<Arc<Module>> {
        let module = self.find_module_for_document(importer_uri)?;
        let import_path = import_uri.to_file_path().ok()?;
        self.module_if_path_within_root(module, &import_path)
    }

    /// Returns `module` when `path` is governed by it, that is, when `path`
    /// sits at or below `module.root` without crossing into a nested module
    /// declared by its own `module.json`.
    fn module_if_path_within_root(&self, module: Arc<Module>, path: &Path) -> Option<Arc<Module>> {
        if !path.starts_with(&module.root) {
            return None;
        }

        let mut dir = path.parent();
        while let Some(current) = dir {
            if current == module.root {
                return Some(module);
            }

            if self.is_module_root_cached(current) {
                return None;
            }

            dir = current.parent();
        }

        None
    }

    /// Returns whether `dir` is a module root, caching the filesystem probe so
    /// repeated ancestry walks during one session do not re-stat the same path.
    fn is_module_root_cached(&self, dir: &Path) -> bool {
        if let Some(&cached) = self.module_root_cache.lock().get(dir) {
            return cached;
        }

        let result = wdl_modules::module::is_module_root(dir);
        self.module_root_cache
            .lock()
            .insert(dir.to_path_buf(), result);
        result
    }
}

/// Formats the panic payload for display.
pub(crate) fn format_panic_payload(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}
