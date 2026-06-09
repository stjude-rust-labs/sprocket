//! Implementation of the LSP server.

use std::ffi::OsStr;
use std::fmt::Formatter;
use std::mem;
use std::ops::ControlFlow;
use std::path::Component;
use std::path::PathBuf;
use std::path::Prefix;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::OnceLock;

use async_lsp::ClientSocket;
use async_lsp::ErrorCode;
use async_lsp::LanguageClient;
use async_lsp::LanguageServer;
use async_lsp::ResponseError;
use async_lsp::client_monitor::ClientProcessMonitorLayer;
use async_lsp::concurrency::ConcurrencyLayer;
use async_lsp::lsp_types::notification::Progress;
use async_lsp::lsp_types::request::WorkDoneProgressCreate;
use async_lsp::lsp_types::request::WorkspaceConfiguration;
use async_lsp::lsp_types::*;
use async_lsp::panic::CatchUnwindLayer;
use async_lsp::router::Router;
use async_lsp::server::LifecycleLayer;
use async_lsp::tracing::TracingLayer;
use futures::future::BoxFuture;
use serde::Deserialize;
use serde::Deserializer;
use serde_json::to_value;
use struct_patch::Patch;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::oneshot::Sender;
use tower::ServiceBuilder;
use tracing::debug;
use tracing::debug_span;
use tracing::error;
use tracing::info;
use tracing::warn;
use tracing_subscriber::EnvFilter;
use url::Url;
use uuid::Uuid;
use wdl_analysis::Analyzer;
use wdl_analysis::Config as AnalysisConfig;
use wdl_analysis::DiagnosticsConfig;
use wdl_analysis::FeatureFlags;
use wdl_analysis::IncrementalChange;
use wdl_analysis::SourceEdit;
use wdl_analysis::SourcePosition;
use wdl_analysis::SourcePositionEncoding;
use wdl_analysis::Validator;
use wdl_analysis::handlers::WDL_SEMANTIC_TOKEN_MODIFIERS;
use wdl_analysis::handlers::WDL_SEMANTIC_TOKEN_TYPES;
use wdl_analysis::path_to_uri;
use wdl_lint::Linter;
use wdl_lint::Rule;

use crate::proto;

/// Normalizes the path of a URI.
///
/// If the path contains percent encoded sequences, the sequences are decoded.
///
/// Additionally, on Windows, this will normalize the drive letter to uppercase.
fn normalize_uri_path(uri: &mut Url) {
    if uri.scheme() != "file" {
        return;
    }

    // Call `to_file_path` which will automatically decode any encoded sequences
    if let Ok(path) = uri.to_file_path() {
        // On windows we need to normalize any drive letter prefixes to uppercase
        let path = if cfg!(windows) {
            let mut comps = path.components();
            match comps.next() {
                Some(Component::Prefix(prefix)) => match prefix.kind() {
                    Prefix::Disk(d) => {
                        let mut path = PathBuf::new();
                        path.push(format!("{}:", d.to_ascii_uppercase() as char));
                        path.extend(comps);
                        path
                    }
                    Prefix::VerbatimDisk(d) => {
                        let mut path = PathBuf::new();
                        path.push(format!(r"\\?\{}:", d.to_ascii_uppercase() as char));
                        path.extend(comps);
                        path
                    }
                    _ => path,
                },
                _ => path,
            }
        } else {
            path
        };

        if let Ok(u) = Url::from_file_path(path) {
            *uri = u;
        }
    }
}

/// LSP features supported by the client.
#[derive(Clone, Copy, Debug, Default)]
struct ClientSupport {
    /// Whether or not the client supports dynamic registration of watched
    /// files.
    pub watched_files: bool,
    /// Whether or not the client supports pull diagnostics (workspace and text
    /// document).
    pub pull_diagnostics: bool,
    /// Whether or not the client supports registering work done progress
    /// tokens.
    pub work_done_progress: bool,
    /// Whether or not the client supports configuration change notifications.
    pub did_change_configuration: bool,
}

impl ClientSupport {
    /// Creates a new client features from the given client capabilities.
    pub fn new(capabilities: &ClientCapabilities) -> Self {
        Self {
            watched_files: capabilities
                .workspace
                .as_ref()
                .map(|c| {
                    c.did_change_watched_files
                        .as_ref()
                        .map(|c| c.dynamic_registration == Some(true))
                        .unwrap_or(false)
                })
                .unwrap_or(false),
            pull_diagnostics: capabilities
                .text_document
                .as_ref()
                .map(|c| c.diagnostic.is_some())
                .unwrap_or(false),
            work_done_progress: capabilities
                .window
                .as_ref()
                .map(|c| c.work_done_progress == Some(true))
                .unwrap_or(false),
            did_change_configuration: capabilities
                .workspace
                .as_ref()
                .map(|c| {
                    c.did_change_configuration
                        .as_ref()
                        .map(|c| c.dynamic_registration == Some(true))
                        .unwrap_or(false)
                })
                .unwrap_or(false),
        }
    }
}

/// Represents a progress token for displaying work progress in the client.
#[derive(Debug, Clone, Default)]
struct ProgressToken(Option<String>);

impl ProgressToken {
    /// Constructs a new progress token.
    ///
    /// If progress tokens aren't supported by the client, this will return a
    /// no-op token.
    pub async fn new(client: &ClientSocket, client_supported: bool) -> Self {
        if !client_supported {
            return Self(None);
        }

        let token = Uuid::new_v4().to_string();
        if client
            .request::<WorkDoneProgressCreate>(WorkDoneProgressCreateParams {
                token: NumberOrString::String(token.clone()),
            })
            .await
            .is_err()
        {
            return Self(None);
        }

        Self(Some(token))
    }

    /// Starts the work progress.
    pub fn start(
        &self,
        client: &ClientSocket,
        title: impl Into<String>,
        message: impl Into<String>,
    ) -> async_lsp::Result<()> {
        let Some(token) = &self.0 else {
            return Ok(());
        };

        client.notify::<Progress>(ProgressParams {
            token: NumberOrString::String(token.clone()),
            value: ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(WorkDoneProgressBegin {
                title: title.into(),
                cancellable: None,
                message: Some(message.into()),
                percentage: Some(0),
            })),
        })
    }

    /// Updates the work progress.
    pub fn update(
        &self,
        client: &ClientSocket,
        message: impl Into<String>,
        percentage: u32,
    ) -> async_lsp::Result<()> {
        let Some(token) = &self.0 else {
            return Ok(());
        };

        client.notify::<Progress>(ProgressParams {
            token: NumberOrString::String(token.clone()),
            value: ProgressParamsValue::WorkDone(WorkDoneProgress::Report(
                WorkDoneProgressReport {
                    cancellable: None,
                    message: Some(message.into()),
                    percentage: Some(percentage),
                },
            )),
        })
    }

    /// Completes the work progress.
    pub fn complete(
        self,
        client: &ClientSocket,
        message: impl Into<String>,
    ) -> async_lsp::Result<()> {
        let Some(token) = self.0 else {
            return Ok(());
        };

        client.notify::<Progress>(ProgressParams {
            token: NumberOrString::String(token),
            value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(WorkDoneProgressEnd {
                message: Some(message.into()),
            })),
        })
    }
}

// NOTE: Renamed camelCase to make it play nicely with the vscode extension.
/// Represents options for running the LSP server.
#[derive(Debug, Clone)]
pub struct ServerOptions {
    /// The name of the server.
    ///
    /// Defaults to `wdl-lsp` crate name.
    pub name: String,

    /// The version of the server.
    ///
    /// Defaults to the version of the `wdl-lsp` crate.
    pub version: String,

    /// Feature flags for enabling experimental features.
    pub feature_flags: FeatureFlags,

    /// Analysis or lint rule IDs to except (ignore).
    pub exceptions: Vec<String>,

    /// Basename for any ignorefiles which should be respected.
    pub ignore_filename: Option<String>,

    /// The diagnostic baseline for suppressing known diagnostics.
    pub baseline: Option<wdl_lint::Baseline>,
}

impl ServerOptions {
    /// Get info about the server.
    fn info(&self) -> ServerInfo {
        ServerInfo {
            name: self.name.clone(),
            version: Some(self.version.clone()),
        }
    }
}

impl Default for ServerOptions {
    fn default() -> Self {
        Self {
            name: String::from(env!("CARGO_CRATE_NAME")),
            version: String::from(env!("CARGO_PKG_VERSION")),
            exceptions: Vec::new(),
            ignore_filename: None,
            feature_flags: Default::default(),
            baseline: None,
        }
    }
}

/// User-controlled options for the server.
#[derive(Debug, Clone, Patch)]
#[patch(attribute(derive(Debug, Default, Deserialize)))]
#[patch(attribute(serde(rename_all = "camelCase")))]
#[patch(attribute(allow(missing_docs)))]
pub struct UserOptions {
    /// The verbosity level of the server.
    pub log_level: LevelFilter,

    /// The options for linting.
    #[patch(nesting)]
    pub lint: LintOptions,
}

impl Default for UserOptions {
    fn default() -> Self {
        Self {
            log_level: LevelFilter(tracing::metadata::LevelFilter::ERROR),
            lint: Default::default(),
        }
    }
}

/// Options for the external linter.
#[derive(Debug, Default, Clone, PartialEq, Patch)]
#[patch(attribute(derive(Debug, Default, Deserialize)))]
#[patch(attribute(allow(missing_docs)))]
pub struct LintOptions {
    /// Whether or not linting is enabled.
    pub enabled: bool,
    /// The lint rule configuration.
    #[patch(skip)]
    pub config: Arc<wdl_lint::Config>,
}

/// Wrapper for [`tracing::metadata::LevelFilter`] to support deserialization.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct LevelFilter(
    #[serde(deserialize_with = "deserialize_level_filter")] tracing::metadata::LevelFilter,
);

impl From<tracing::metadata::LevelFilter> for LevelFilter {
    fn from(level: tracing::metadata::LevelFilter) -> Self {
        Self(level)
    }
}

/// Deserializer for [`tracing::metadata::LevelFilter`].
fn deserialize_level_filter<'de, D>(
    deserializer: D,
) -> Result<tracing::metadata::LevelFilter, D::Error>
where
    D: Deserializer<'de>,
{
    struct LevelFilterVisitor;

    impl<'de> serde::de::Visitor<'de> for LevelFilterVisitor {
        type Value = tracing::metadata::LevelFilter;

        fn expecting(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
            write!(formatter, "a level filter string")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            tracing::metadata::LevelFilter::from_str(v).map_err(serde::de::Error::custom)
        }
    }

    deserializer.deserialize_str(LevelFilterVisitor)
}

/// Reload handle for dynamic level filter setting.
pub type FilterReloadHandle<S> = tracing_subscriber::reload::Handle<EnvFilter, S>;

/// Mutable server state.
#[derive(Debug)]
struct ServerState<S> {
    /// The current set of workspace folders.
    folders: Vec<WorkspaceFolder>,
    /// Mutable configuration fields.
    config: ServerConfig,
    /// Level filter reload handle.
    log_handle: Option<FilterReloadHandle<S>>,
}

impl<S> ServerState<S> {
    /// Patch the config with the new values from the client.
    fn apply_config_patch(
        &mut self,
        client: ClientSocket,
        options: &ServerOptions,
        patch: UserOptionsPatch,
    ) {
        if let Some(log_level) = patch.log_level
            && let Some(reload_handle) = self.log_handle.as_ref()
            && let Err(e) = reload_handle.modify(|filter| {
                let current_directives = filter.to_string();
                *filter = EnvFilter::builder()
                    .parse_lossy(format!("{},{}", current_directives, log_level.0));
            })
        {
            error!("failed to set log level: {e:?}");
        }

        self.config.options.apply(patch);
        self.config.analyzer = options.analyzer(client.clone(), &self.config.options.lint);
    }
}

/// Represents an LSP server for analyzing WDL documents.
#[derive(Debug)]
pub struct Server<S> {
    /// The LSP client connected to the server.
    client: ClientSocket,
    /// The features supported by the LSP client.
    client_support: Arc<OnceLock<ClientSupport>>,
    /// Static server options.
    options: Arc<ServerOptions>,
    /// Mutable server state.
    state: Arc<tokio::sync::RwLock<ServerState<S>>>,
    /// Sender for client [`Message`]s.
    message_tx: Option<tokio::sync::mpsc::UnboundedSender<Message>>,
    /// Task handle for the message loop.
    _message_loop_handle: Option<tokio::task::JoinHandle<()>>,
}

/// The server config and dependent fields.
#[derive(Debug)]
struct ServerConfig {
    /// User-controlled options for the server.
    options: UserOptions,
    /// The analyzer used to analyze documents.
    analyzer: Analyzer<ProgressToken>,
}

impl ServerOptions {
    /// Create an [`Analyzer`] based on this config.
    fn analyzer(
        &self,
        client: ClientSocket,
        lint_options: &LintOptions,
    ) -> Analyzer<ProgressToken> {
        let linting_enabled = lint_options.enabled;
        let exceptions = self.exceptions.clone();
        let ignore_name = self.ignore_filename.clone();
        let analyzer_client = client.clone();

        let mut all_rules: Vec<_> = wdl_analysis::ALL_RULE_IDS
            .iter()
            .chain(wdl_lint::ALL_RULE_IDS.iter())
            .map(ToString::to_string)
            .collect();
        all_rules.sort_unstable();
        all_rules.dedup();

        // TODO ACF 2025-07-07: add configurability around the fallback behavior; see
        // https://github.com/stjude-rust-labs/wdl/issues/517
        let analyzer_config = AnalysisConfig::default()
            .with_fallback_version(Some(Default::default()))
            .with_diagnostics_config(DiagnosticsConfig::new(
                wdl_analysis::rules()
                    .iter()
                    .filter(|r| !exceptions.contains(&r.id().into())),
            ))
            .with_ignore_filename(ignore_name)
            .with_all_rules(all_rules)
            .with_feature_flags(self.feature_flags);

        let wdl_lint_config = lint_options.config.clone();
        Analyzer::<ProgressToken>::new_with_validator(
            analyzer_config,
            move |token, kind, current, total| {
                let client = analyzer_client.clone();
                async move {
                    let message = format!(
                        "{kind} {current}/{total} file{s}",
                        s = if total > 1 { "s" } else { "" }
                    );
                    let percentage = ((current * 100) as f64 / total as f64) as u32;
                    let _ = token.update(&client, message, percentage);
                }
            },
            move || {
                let mut validator = Validator::default();
                if linting_enabled {
                    validator.add_visitor(Linter::new(
                        wdl_lint::rules(&wdl_lint_config)
                            .into_iter()
                            .filter(|r| !exceptions.contains(&r.id().into()))
                            .map(|r| r as Box<dyn Rule>),
                    ));
                }
                validator
            },
        )
    }
}

/// LSP notifications sent from the client.
#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
enum Notification {
    /// A document was opened.
    DidOpen(DidOpenTextDocumentParams),
    /// A document changed.
    DidChange(DidChangeTextDocumentParams),
    /// A document was closed.
    DidClose(DidCloseTextDocumentParams),
    /// The workspace folders changed.
    DidChangeWorkspaceFolders(DidChangeWorkspaceFoldersParams),
    /// The [`UserOptions`] changed.
    DidChangeConfiguration(DidChangeConfigurationParams),
    /// A watched file/folder was changed.
    DidChangeWatchedFiles(DidChangeWatchedFilesParams),
}

/// LSP requests sent from the client.
#[derive(Debug)]
#[allow(clippy::enum_variant_names, clippy::missing_docs_in_private_items)]
enum Request {
    /// `textDocument/completion`
    Completion {
        params: CompletionParams,
        tx: RequestResponseSender<Option<CompletionResponse>>,
    },
    /// `textDocument/definition`
    Definition {
        params: GotoDefinitionParams,
        tx: RequestResponseSender<Option<GotoDefinitionResponse>>,
    },
    /// `textDocument/diagnostic`
    DocumentDiagnostic {
        params: DocumentDiagnosticParams,
        tx: RequestResponseSender<DocumentDiagnosticReportResult>,
    },
    /// `textDocument/symbol`
    DocumentSymbol {
        params: DocumentSymbolParams,
        tx: RequestResponseSender<Option<DocumentSymbolResponse>>,
    },
    /// `textDocument/foldingRange`
    FoldingRange {
        params: FoldingRangeParams,
        tx: RequestResponseSender<Option<Vec<FoldingRange>>>,
    },
    /// `textDocument/formatting`
    Formatting {
        params: DocumentFormattingParams,
        tx: RequestResponseSender<Option<Vec<TextEdit>>>,
    },
    /// `textDocument/hover`
    Hover {
        params: HoverParams,
        tx: RequestResponseSender<Option<Hover>>,
    },
    /// `callHierarchy/incomingCalls`
    IncomingCalls {
        params: CallHierarchyIncomingCallsParams,
        tx: RequestResponseSender<Option<Vec<CallHierarchyIncomingCall>>>,
    },
    /// `textDocument/inlayHint`
    InlayHint {
        params: InlayHintParams,
        tx: RequestResponseSender<Option<Vec<InlayHint>>>,
    },
    /// `callHierarchy/outgoingCalls`
    OutgoingCalls {
        params: CallHierarchyOutgoingCallsParams,
        tx: RequestResponseSender<Option<Vec<CallHierarchyOutgoingCall>>>,
    },
    /// `textDocument/prepareCallHierarchy`
    PrepareCallHierarchy {
        params: CallHierarchyPrepareParams,
        tx: RequestResponseSender<Option<Vec<CallHierarchyItem>>>,
    },
    /// `textDocument/references`
    References {
        params: ReferenceParams,
        tx: RequestResponseSender<Option<Vec<Location>>>,
    },
    /// `textDocument/rename`
    Rename {
        params: RenameParams,
        tx: RequestResponseSender<Option<WorkspaceEdit>>,
    },
    /// `textDocument/semanticTokens/full`
    SemanticTokensFull {
        params: SemanticTokensParams,
        tx: RequestResponseSender<Option<SemanticTokensResult>>,
    },
    /// `textDocument/signatureHelp`
    SignatureHelp {
        params: SignatureHelpParams,
        tx: RequestResponseSender<Option<SignatureHelp>>,
    },
    /// `workspace/symbol`
    Symbol {
        params: WorkspaceSymbolParams,
        tx: RequestResponseSender<Option<WorkspaceSymbolResponse>>,
    },
    /// `workspace/diagnostic`
    WorkspaceDiagnostic {
        params: WorkspaceDiagnosticParams,
        tx: RequestResponseSender<WorkspaceDiagnosticReportResult>,
    },
}

/// A message from the client.
enum Message {
    /// A notification.
    Notification(Notification),
    /// A request.
    Request(Request),
}

impl<S: 'static> Server<S> {
    /// Creates a new WDL language server.
    ///
    /// `log_handle` can be provided to enable dynamic log level setting.
    pub fn new(
        client: ClientSocket,
        options: ServerOptions,
        user_options: UserOptions,
        log_handle: Option<FilterReloadHandle<S>>,
    ) -> Self {
        let analyzer = options.analyzer(client.clone(), &user_options.lint);
        let (message_tx, message_rx) = tokio::sync::mpsc::unbounded_channel();

        let client_support = Arc::new(OnceLock::new());
        let options = Arc::new(options);
        let state = Arc::new(tokio::sync::RwLock::new(ServerState {
            folders: Default::default(),
            config: ServerConfig {
                options: user_options,
                analyzer,
            },
            log_handle,
        }));

        let state_clone = state.clone();
        let options_clone = options.clone();
        let client_clone = client.clone();
        let client_support_clone = client_support.clone();
        let message_loop_handle = tokio::task::spawn(async move {
            Self::message_loop(
                message_rx,
                state_clone,
                options_clone,
                client_clone,
                client_support_clone,
            )
            .await;
        });

        Self {
            client,
            client_support,
            options,
            state,
            message_tx: Some(message_tx),
            _message_loop_handle: Some(message_loop_handle),
        }
    }

    /// Runs the server until a request is received to shut down.
    ///
    /// See also: [`Self::new()`]
    pub async fn run(
        options: ServerOptions,
        user_options: UserOptions,
        log_handle: Option<FilterReloadHandle<S>>,
    ) -> anyhow::Result<()> {
        debug!("running LSP server: {options:#?}; user options: {user_options:#?}");

        let (server, _) = async_lsp::MainLoop::new_server(|client| {
            ServiceBuilder::new()
                .layer(
                    TracingLayer::new()
                        .notification(|notif| {
                            let span = debug_span!("notification", method = notif.method);
                            span.in_scope(|| {
                                debug!(
                                    "received notification with parameters: {:#?}",
                                    notif.params
                                );
                            });
                            span
                        })
                        .request(|request| {
                            let span = debug_span!("request", method = request.method);
                            span.in_scope(|| {
                                debug!("received request with parameters: {:#?}", request.params);
                            });
                            span
                        }),
                )
                .layer(LifecycleLayer::default())
                .layer(CatchUnwindLayer::default())
                .layer(ConcurrencyLayer::default())
                .layer(ClientProcessMonitorLayer::new(client.clone()))
                .service(Router::from_language_server(Self::new(
                    client,
                    options,
                    user_options,
                    log_handle,
                )))
        });

        // Prefer truly asynchronous piped stdin/stdout without blocking tasks.
        #[cfg(unix)]
        let (stdin, stdout) = (
            async_lsp::stdio::PipeStdin::lock_tokio()?,
            async_lsp::stdio::PipeStdout::lock_tokio()?,
        );

        // Fallback to spawn blocking read/write otherwise.
        #[cfg(not(unix))]
        let (stdin, stdout) = (
            tokio_util::compat::TokioAsyncReadCompatExt::compat(tokio::io::stdin()),
            tokio_util::compat::TokioAsyncWriteCompatExt::compat_write(tokio::io::stdout()),
        );

        server.run_buffered(stdin, stdout).await.map_err(Into::into)
    }
}

/// Sender type for request results.
type RequestResponseSender<T> = Sender<Result<T, ResponseError>>;

// Message handlers
impl<S: 'static> Server<S> {
    /// Send a message to the queue.
    fn queue(&self, message: Message) -> ControlFlow<async_lsp::Result<()>> {
        match self
            .message_tx
            .as_ref()
            .and_then(|tx| tx.send(message).ok())
        {
            Some(()) => ControlFlow::Continue(()),
            None => ControlFlow::Break(Err(async_lsp::Error::ServiceStopped)),
        }
    }

    /// Queue a client request.
    fn request<R, F>(&self, make_request: F) -> BoxFuture<'static, Result<R, ResponseError>>
    where
        R: Send + 'static,
        F: FnOnce(Sender<Result<R, ResponseError>>) -> Message + Send + 'static,
    {
        let message_tx = self.message_tx.clone();

        Box::pin(async move {
            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
            let msg = make_request(reply_tx);

            if message_tx.and_then(|tx| tx.send(msg).ok()).is_none() {
                return Err(ResponseError::new(
                    ErrorCode::INTERNAL_ERROR,
                    "Server message queue closed",
                ));
            }

            reply_rx.await.unwrap_or_else(|_| {
                Err(ResponseError::new(
                    ErrorCode::INTERNAL_ERROR,
                    "Background task dropped the request without replying",
                ))
            })
        })
    }

    /// Incoming [`Message`] handler loop.
    async fn message_loop(
        mut rx: UnboundedReceiver<Message>,
        state: Arc<tokio::sync::RwLock<ServerState<S>>>,
        options: Arc<ServerOptions>,
        client: ClientSocket,
        client_support: Arc<OnceLock<ClientSupport>>,
    ) {
        while let Some(message) = rx.recv().await {
            match message {
                Message::Notification(notification) => match notification {
                    Notification::DidOpen(params) => {
                        let state = state.read().await;
                        Self::did_open(params, &state).await;
                    }
                    Notification::DidChange(params) => {
                        let state = state.read().await;
                        Self::did_change(params, &state).await;
                    }
                    Notification::DidClose(params) => {
                        let state = state.read().await;
                        Self::did_close(params, &state).await;
                    }
                    Notification::DidChangeWorkspaceFolders(params) => {
                        let state = state.read().await;
                        Self::did_change_workspace_folders(params, &state).await;
                    }
                    Notification::DidChangeConfiguration(params) => {
                        let mut state = state.write().await;
                        Self::did_change_configuration(
                            params,
                            &mut state,
                            client.clone(),
                            &options,
                        )
                        .await;
                    }
                    Notification::DidChangeWatchedFiles(params) => {
                        let state = state.read().await;
                        Self::did_change_watched_files(params, &state).await;
                    }
                },
                Message::Request(request) => match request {
                    Request::Completion { params, tx } => {
                        let state = state.read().await;
                        Self::completion(params, tx, &state).await
                    }
                    Request::Definition { params, tx } => {
                        let state = state.read().await;
                        Self::definition(params, tx, &state).await
                    }
                    Request::DocumentDiagnostic { params, tx } => {
                        let state = state.read().await;
                        Self::document_diagnostic(params, tx, &state, &options).await
                    }
                    Request::DocumentSymbol { params, tx } => {
                        let state = state.read().await;
                        Self::document_symbol(params, tx, &state).await
                    }
                    Request::FoldingRange { params, tx } => {
                        let state = state.read().await;
                        Self::folding_range(params, tx, &state).await
                    }
                    Request::Formatting { params, tx } => {
                        let state = state.read().await;
                        Self::formatting(params, tx, &state).await
                    }
                    Request::Hover { params, tx } => {
                        let state = state.read().await;
                        Self::hover(params, tx, &state).await
                    }
                    Request::IncomingCalls { params, tx } => {
                        let state = state.read().await;
                        Self::incoming_calls(params, tx, &state).await
                    }
                    Request::InlayHint { params, tx } => {
                        let state = state.read().await;
                        Self::inlay_hint(params, tx, &state).await
                    }
                    Request::OutgoingCalls { params, tx } => {
                        let state = state.read().await;
                        Self::outgoing_calls(params, tx, &state).await
                    }
                    Request::PrepareCallHierarchy { params, tx } => {
                        let state = state.read().await;
                        Self::prepare_call_hierarchy(params, tx, &state).await
                    }
                    Request::References { params, tx } => {
                        let state = state.read().await;
                        Self::references(params, tx, &state).await
                    }
                    Request::Rename { params, tx } => {
                        let state = state.read().await;
                        Self::rename(params, tx, &state).await
                    }
                    Request::SemanticTokensFull { params, tx } => {
                        let state = state.read().await;
                        Self::semantic_tokens_full(params, tx, &state).await
                    }
                    Request::SignatureHelp { params, tx } => {
                        let state = state.read().await;
                        Self::signature_help(params, tx, &state).await
                    }
                    Request::Symbol { params, tx } => {
                        let state = state.read().await;
                        Self::symbol(params, tx, &state).await
                    }
                    Request::WorkspaceDiagnostic { params, tx } => {
                        let state = state.read().await;
                        let client_support = client_support.get().expect("should be initialized");
                        Self::workspace_diagnostic(
                            params,
                            tx,
                            &state,
                            &options,
                            client.clone(),
                            client_support,
                        )
                        .await
                    }
                },
            }
        }
    }

    /// `textDocument/completion` request handler.
    async fn completion(
        mut params: CompletionParams,
        tx: RequestResponseSender<Option<CompletionResponse>>,
        state: &ServerState<S>,
    ) {
        normalize_uri_path(&mut params.text_document_position.text_document.uri);

        let position = SourcePosition::new(
            params.text_document_position.position.line,
            params.text_document_position.position.character,
        );

        let result = state
            .config
            .analyzer
            .completion(
                ProgressToken::default(),
                params.text_document_position.text_document.uri,
                position,
                SourcePositionEncoding::UTF16,
            )
            .await
            .map_err(|e| ResponseError::new(ErrorCode::INTERNAL_ERROR, e));

        let _ = tx.send(result);
    }

    /// `textDocument/definition` request handler.
    async fn definition(
        mut params: GotoDefinitionParams,
        tx: RequestResponseSender<Option<GotoDefinitionResponse>>,
        state: &ServerState<S>,
    ) {
        normalize_uri_path(&mut params.text_document_position_params.text_document.uri);

        let position = SourcePosition::new(
            params.text_document_position_params.position.line,
            params.text_document_position_params.position.character,
        );

        let result = state
            .config
            .analyzer
            .goto_definition(
                params.text_document_position_params.text_document.uri,
                position,
                SourcePositionEncoding::UTF16,
            )
            .await
            .map_err(|e| ResponseError::new(ErrorCode::INTERNAL_ERROR, e));

        let _ = tx.send(result);
    }

    /// `textDocument/documentSymbol` request handler.
    async fn document_symbol(
        mut params: DocumentSymbolParams,
        tx: RequestResponseSender<Option<DocumentSymbolResponse>>,
        state: &ServerState<S>,
    ) {
        normalize_uri_path(&mut params.text_document.uri);

        let result = state
            .config
            .analyzer
            .document_symbol(params.text_document.uri)
            .await
            .map_err(|e| ResponseError::new(ErrorCode::INTERNAL_ERROR, e));

        let _ = tx.send(result);
    }

    /// `textDocument/diagnostic` request handler.
    async fn document_diagnostic(
        mut params: DocumentDiagnosticParams,
        tx: RequestResponseSender<DocumentDiagnosticReportResult>,
        state: &ServerState<S>,
        options: &ServerOptions,
    ) {
        normalize_uri_path(&mut params.text_document.uri);

        let results = state
            .config
            .analyzer
            .analyze_document(ProgressToken::default(), params.text_document.uri.clone())
            .await
            .map_err(|e| ResponseError::new(ErrorCode::INTERNAL_ERROR, e))
            .and_then(|results| {
                let mut matcher = options.baseline.as_ref().map(|b| b.matcher());
                proto::document_diagnostic_report(params, results, &options.name, matcher.as_mut())
                    .ok_or_else(|| {
                        ResponseError::new(
                            ErrorCode::REQUEST_FAILED,
                            "no diagnostic report produced",
                        )
                    })
            });

        let _ = tx.send(results);
    }

    /// `textDocument/foldingRange` request handler.
    async fn folding_range(
        mut params: FoldingRangeParams,
        tx: RequestResponseSender<Option<Vec<FoldingRange>>>,
        state: &ServerState<S>,
    ) {
        normalize_uri_path(&mut params.text_document.uri);

        let result = state
            .config
            .analyzer
            .folding_range(params.text_document.uri)
            .await
            .map_err(|e| ResponseError::new(ErrorCode::INTERNAL_ERROR, e));

        let _ = tx.send(result);
    }

    /// `textDocument/formatting` request handler.
    async fn formatting(
        mut params: DocumentFormattingParams,
        tx: RequestResponseSender<Option<Vec<TextEdit>>>,
        state: &ServerState<S>,
    ) {
        normalize_uri_path(&mut params.text_document.uri);

        let result = state
            .config
            .analyzer
            .format_document(params.text_document.uri)
            .await
            .map_err(|e| ResponseError::new(ErrorCode::INTERNAL_ERROR, e))
            .map(|res| {
                res.map(|(end_line, end_col, formatted)| {
                    vec![TextEdit {
                        range: Range {
                            // NOTE: always replace the full set of text starting at the
                            // very first position.
                            start: Position {
                                line: 0,
                                character: 0,
                            },
                            end: Position {
                                line: end_line,
                                character: end_col,
                            },
                        },
                        new_text: formatted,
                    }]
                })
            });

        let _ = tx.send(result);
    }

    /// `textDocument/hover` request handler.
    async fn hover(
        mut params: HoverParams,
        tx: RequestResponseSender<Option<Hover>>,
        state: &ServerState<S>,
    ) {
        normalize_uri_path(&mut params.text_document_position_params.text_document.uri);

        let position = SourcePosition::new(
            params.text_document_position_params.position.line,
            params.text_document_position_params.position.character,
        );

        let result = state
            .config
            .analyzer
            .hover(
                params.text_document_position_params.text_document.uri,
                position,
                SourcePositionEncoding::UTF16,
            )
            .await
            .map_err(|e| ResponseError::new(ErrorCode::INTERNAL_ERROR, e));

        let _ = tx.send(result);
    }

    /// `callHierarchy/incomingCalls` request handler.
    async fn incoming_calls(
        mut params: CallHierarchyIncomingCallsParams,
        tx: RequestResponseSender<Option<Vec<CallHierarchyIncomingCall>>>,
        state: &ServerState<S>,
    ) {
        normalize_uri_path(&mut params.item.uri);

        let position = SourcePosition::new(
            params.item.selection_range.start.line,
            params.item.selection_range.start.character,
        );

        let result = state
            .config
            .analyzer
            .incoming_calls(params.item.uri, position, SourcePositionEncoding::UTF16)
            .await
            .map_err(|e| ResponseError::new(ErrorCode::INTERNAL_ERROR, e));

        let _ = tx.send(result);
    }

    /// `textDocument/inlayHint` request handler.
    async fn inlay_hint(
        mut params: InlayHintParams,
        tx: RequestResponseSender<Option<Vec<InlayHint>>>,
        state: &ServerState<S>,
    ) {
        normalize_uri_path(&mut params.text_document.uri);

        // Analyze the document first to ensure we have up-to-date information
        if let Err(e) = state.config.analyzer.analyze(ProgressToken(None)).await {
            let _ = tx.send(Err(ResponseError::new(ErrorCode::INTERNAL_ERROR, e)));
            return;
        }

        let result = state
            .config
            .analyzer
            .inlay_hints(params.text_document.uri, params.range)
            .await
            .map_err(|e| ResponseError::new(ErrorCode::INTERNAL_ERROR, e));

        let _ = tx.send(result);
    }

    /// `callHierarchy/outgoingCalls` request handler.
    async fn outgoing_calls(
        mut params: CallHierarchyOutgoingCallsParams,
        tx: RequestResponseSender<Option<Vec<CallHierarchyOutgoingCall>>>,
        state: &ServerState<S>,
    ) {
        normalize_uri_path(&mut params.item.uri);

        let position = SourcePosition::new(
            params.item.selection_range.start.line,
            params.item.selection_range.start.character,
        );

        let result = state
            .config
            .analyzer
            .outgoing_calls(params.item.uri, position, SourcePositionEncoding::UTF16)
            .await
            .map_err(|e| ResponseError::new(ErrorCode::INTERNAL_ERROR, e));

        let _ = tx.send(result);
    }

    /// `textDocument/prepareCallHierarchy` request handler.
    async fn prepare_call_hierarchy(
        mut params: CallHierarchyPrepareParams,
        tx: RequestResponseSender<Option<Vec<CallHierarchyItem>>>,
        state: &ServerState<S>,
    ) {
        normalize_uri_path(&mut params.text_document_position_params.text_document.uri);

        let position = SourcePosition::new(
            params.text_document_position_params.position.line,
            params.text_document_position_params.position.character,
        );

        let result = state
            .config
            .analyzer
            .call_hierarchy(
                params.text_document_position_params.text_document.uri,
                position,
                SourcePositionEncoding::UTF16,
            )
            .await
            .map_err(|e| ResponseError::new(ErrorCode::INTERNAL_ERROR, e));

        let _ = tx.send(result);
    }

    /// `textDocument/references` request handler.
    async fn references(
        mut params: ReferenceParams,
        tx: RequestResponseSender<Option<Vec<Location>>>,
        state: &ServerState<S>,
    ) {
        normalize_uri_path(&mut params.text_document_position.text_document.uri);

        let position = SourcePosition::new(
            params.text_document_position.position.line,
            params.text_document_position.position.character,
        );

        let result = state
            .config
            .analyzer
            .find_all_references(
                params.text_document_position.text_document.uri,
                position,
                SourcePositionEncoding::UTF16,
                params.context.include_declaration,
            )
            .await
            .map_err(|e| ResponseError::new(ErrorCode::INTERNAL_ERROR, e));

        let _ = tx.send(result.map(Some));
    }

    /// `textDocument/rename` request handler.
    async fn rename(
        mut params: RenameParams,
        tx: RequestResponseSender<Option<WorkspaceEdit>>,
        state: &ServerState<S>,
    ) {
        normalize_uri_path(&mut params.text_document_position.text_document.uri);

        let position = SourcePosition::new(
            params.text_document_position.position.line,
            params.text_document_position.position.character,
        );

        let result = state
            .config
            .analyzer
            .rename(
                params.text_document_position.text_document.uri,
                position,
                SourcePositionEncoding::UTF16,
                params.new_name,
            )
            .await
            .map_err(|e| ResponseError::new(ErrorCode::INTERNAL_ERROR, e));

        let _ = tx.send(result);
    }

    /// `textDocument/semanticTokens/full` request handler.
    async fn semantic_tokens_full(
        mut params: SemanticTokensParams,
        tx: RequestResponseSender<Option<SemanticTokensResult>>,
        state: &ServerState<S>,
    ) {
        normalize_uri_path(&mut params.text_document.uri);

        let result = state
            .config
            .analyzer
            .semantic_tokens(params.text_document.uri)
            .await
            .map_err(|e| ResponseError::new(ErrorCode::INTERNAL_ERROR, e));

        let _ = tx.send(result);
    }

    /// `textDocument/signatureHelp` request handler.
    async fn signature_help(
        mut params: SignatureHelpParams,
        tx: RequestResponseSender<Option<SignatureHelp>>,
        state: &ServerState<S>,
    ) {
        normalize_uri_path(&mut params.text_document_position_params.text_document.uri);

        let position = SourcePosition::new(
            params.text_document_position_params.position.line,
            params.text_document_position_params.position.character,
        );

        let result = state
            .config
            .analyzer
            .signature_help(
                params.text_document_position_params.text_document.uri,
                position,
                SourcePositionEncoding::UTF16,
            )
            .await
            .map_err(|e| ResponseError::new(ErrorCode::INTERNAL_ERROR, e));

        let _ = tx.send(result);
    }

    /// `workspace/symbol` request handler.
    async fn symbol(
        params: WorkspaceSymbolParams,
        tx: RequestResponseSender<Option<WorkspaceSymbolResponse>>,
        state: &ServerState<S>,
    ) {
        let result = state
            .config
            .analyzer
            .workspace_symbol(params.query)
            .await
            .map(|opt| opt.map(WorkspaceSymbolResponse::Flat))
            .map_err(|e| ResponseError::new(ErrorCode::INTERNAL_ERROR, e));

        let _ = tx.send(result);
    }

    /// `workspace/diagnostic` request handler.
    async fn workspace_diagnostic(
        params: WorkspaceDiagnosticParams,
        tx: RequestResponseSender<WorkspaceDiagnosticReportResult>,
        state: &ServerState<S>,
        options: &ServerOptions,
        client: ClientSocket,
        client_support: &ClientSupport,
    ) {
        let progress = ProgressToken::new(&client, client_support.work_done_progress).await;
        let _ = progress.start(&client, options.name.clone(), "analyzing...");
        let results = state
            .config
            .analyzer
            .analyze(progress.clone())
            .await
            .map_err(|e| ResponseError::new(ErrorCode::INTERNAL_ERROR, e))
            .map(|results| {
                let _ = progress.complete(&client, "analysis complete");

                let mut matcher = options.baseline.as_ref().map(|b| b.matcher());
                proto::workspace_diagnostic_report(params, results, &options.name, matcher.as_mut())
            });

        let _ = tx.send(results);
    }

    /// `textDocument/didOpen` notification handler.
    async fn did_open(mut params: DidOpenTextDocumentParams, state: &ServerState<S>) {
        normalize_uri_path(&mut params.text_document.uri);

        if let Err(e) = state
            .config
            .analyzer
            .add_document(params.text_document.uri.clone())
            .await
        {
            error!(
                "failed to add document {uri}: {e}",
                uri = params.text_document.uri
            );
        }

        if let Err(e) = state.config.analyzer.notify_incremental_change(
            params.text_document.uri,
            IncrementalChange {
                version: params.text_document.version,
                start: Some(params.text_document.text),
                edits: Vec::new(),
            },
        ) {
            error!("failed to notify incremental change: {e}");
        }
    }

    /// `textDocument/didClose` notification handler.
    async fn did_close(mut params: DidCloseTextDocumentParams, state: &ServerState<S>) {
        normalize_uri_path(&mut params.text_document.uri);

        if let Err(e) = state
            .config
            .analyzer
            .notify_change(params.text_document.uri, true)
        {
            error!("failed to notify change: {e}");
        }
    }

    /// `textDocument/didChange` notification handler.
    async fn did_change(mut params: DidChangeTextDocumentParams, state: &ServerState<S>) {
        normalize_uri_path(&mut params.text_document.uri);

        debug!(
            "document `{uri}` is now client version {version}",
            uri = params.text_document.uri,
            version = params.text_document.version
        );

        // Look for the last full change (one without a range) and start there
        let (start, changes) = match params
            .content_changes
            .iter()
            .rposition(|change| change.range.is_none())
        {
            Some(idx) => (
                Some(mem::take(&mut params.content_changes[idx].text)),
                &mut params.content_changes[idx + 1..],
            ),
            None => (None, &mut params.content_changes[..]),
        };

        if let Err(e) = state.config.analyzer.notify_incremental_change(
            params.text_document.uri,
            IncrementalChange {
                version: params.text_document.version,
                start,
                edits: changes
                    .iter_mut()
                    .map(|e| {
                        let range = e.range.expect("edit should be after the last full change");
                        SourceEdit::new(
                            SourcePosition::new(range.start.line, range.start.character)
                                ..SourcePosition::new(range.end.line, range.end.character),
                            SourcePositionEncoding::UTF16,
                            mem::take(&mut e.text),
                        )
                    })
                    .collect(),
            },
        ) {
            error!("failed to notify incremental change: {e}");
        }
    }

    /// `workspace/didChangeWorkspaceFolders` notification handler.
    async fn did_change_workspace_folders(
        params: DidChangeWorkspaceFoldersParams,
        state: &ServerState<S>,
    ) {
        // Process the removed folders
        if !params.event.removed.is_empty()
            && let Err(e) = state
                .config
                .analyzer
                .remove_documents(
                    params
                        .event
                        .removed
                        .into_iter()
                        .map(|mut f| {
                            normalize_uri_path(&mut f.uri);
                            f.uri
                        })
                        .collect(),
                )
                .await
        {
            error!("failed to remove documents from analyzer: {e}");
        }

        // Progress the added folders
        if !params.event.added.is_empty() {
            for folder in &params.event.added {
                match folder.uri.to_file_path() {
                    Ok(path) => {
                        if let Err(e) = state.config.analyzer.add_directory(path).await {
                            error!("failed to add documents from directory to analyzer: {e}");
                        }
                    }
                    Err(_) => {
                        warn!(
                            "failed to convert URI `{uri}` to a file path",
                            uri = folder.uri
                        );
                    }
                }
            }
        }
    }

    /// `workspace/didChangeConfiguration` notification handler.
    async fn did_change_configuration(
        _params: DidChangeConfigurationParams,
        state: &mut ServerState<S>,
        client: ClientSocket,
        options: &ServerOptions,
    ) {
        let workspace_configs = client
            .request::<WorkspaceConfiguration>(ConfigurationParams {
                items: vec![ConfigurationItem {
                    scope_uri: None,
                    section: Some(String::from("sprocket.server")),
                }],
            })
            .await;

        match workspace_configs {
            Ok(mut configs) if !configs.is_empty() => {
                match serde_json::from_value::<UserOptionsPatch>(configs.remove(0)) {
                    Ok(patch) => state.apply_config_patch(client, options, patch),
                    Err(e) => error!("failed to deserialize `UserOptionsPatch`: {e:?}"),
                }
            }
            Ok(_) => error!("client returned no configuration"),
            Err(e) => error!("failed to fetch workspace configuration: {e}"),
        }
    }

    /// `workspace/didChangeWatchedFiles` notification handler.
    async fn did_change_watched_files(params: DidChangeWatchedFilesParams, state: &ServerState<S>) {
        /// Converts a URI into a WDL file path.
        fn to_wdl_file_path(uri: &Url) -> Option<PathBuf> {
            if let Ok(path) = uri.to_file_path()
                && path.is_file()
                && path.extension().and_then(OsStr::to_str) == Some("wdl")
            {
                return Some(path);
            }

            None
        }

        let mut added = Vec::new();
        let mut deleted = Vec::new();
        for mut event in params.changes {
            normalize_uri_path(&mut event.uri);

            match event.typ {
                FileChangeType::CREATED => {
                    let Some(path) = to_wdl_file_path(&event.uri) else {
                        continue;
                    };

                    debug!("document `{uri}` has been created", uri = event.uri);
                    added.push(path_to_uri(&path).expect("should convert to uri"));
                }
                FileChangeType::CHANGED => {
                    if to_wdl_file_path(&event.uri).is_some() {
                        debug!("document `{uri}` has been changed", uri = event.uri);
                        if let Err(e) = state.config.analyzer.notify_change(event.uri, false) {
                            error!("failed to notify change: {e}");
                        }
                    }
                }
                FileChangeType::DELETED => {
                    if to_wdl_file_path(&event.uri).is_none() {
                        continue;
                    }

                    debug!("document `{uri}` has been deleted", uri = event.uri);
                    deleted.push(event.uri);
                }
                _ => {}
            }
        }

        if !added.is_empty() {
            for uri in added {
                if let Err(e) = state.config.analyzer.add_document(uri).await {
                    error!("failed to add documents to analyzer: {e}");
                }
            }
        }

        if !deleted.is_empty()
            && let Err(e) = state.config.analyzer.remove_documents(deleted).await
        {
            error!("failed to remove documents from analyzer: {e}");
        }
    }
}

impl<S: 'static> LanguageServer for Server<S> {
    type Error = ResponseError;
    type NotifyResult = ControlFlow<async_lsp::Result<()>>;

    fn initialize(
        &mut self,
        params: InitializeParams,
    ) -> BoxFuture<'static, Result<InitializeResult, Self::Error>> {
        let client_support = ClientSupport::new(&params.capabilities);

        if !client_support.pull_diagnostics {
            return Box::pin(async move {
                Err(ResponseError::new(
                    ErrorCode::REQUEST_FAILED,
                    "LSP server currently requires support for pulling diagnostics",
                ))
            });
        }

        // This is guaranteed to be called once anyway
        let _ = self.client_support.set(client_support);

        let state = self.state.clone();
        let info = self.options.info();
        Box::pin(async move {
            let mut state = state.write().await;

            if let Some(folders) = params.workspace_folders {
                for mut folder in folders {
                    normalize_uri_path(&mut folder.uri);
                    state.folders.push(folder.clone());
                    match folder.uri.to_file_path() {
                        Ok(path) => {
                            if let Err(e) = state.config.analyzer.add_directory(path).await {
                                error!(
                                    "failed to add initial workspace directory {uri}: {e}",
                                    uri = folder.uri
                                );
                            }
                        }
                        Err(_) => {
                            warn!(
                                "failed to convert URI `{uri}` to a file path",
                                uri = folder.uri
                            );
                        }
                    }
                }
            }

            Ok(InitializeResult {
                capabilities: ServerCapabilities {
                    text_document_sync: Some(TextDocumentSyncCapability::Options(
                        TextDocumentSyncOptions {
                            open_close: Some(true),
                            change: Some(TextDocumentSyncKind::INCREMENTAL),
                            ..Default::default()
                        },
                    )),
                    workspace: Some(WorkspaceServerCapabilities {
                        workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                            supported: Some(true),
                            change_notifications: Some(OneOf::Left(true)),
                        }),
                        ..Default::default()
                    }),
                    workspace_symbol_provider: Some(OneOf::Left(true)),
                    diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                        DiagnosticOptions {
                            inter_file_dependencies: true,
                            workspace_diagnostics: true,
                            // Intentionally disabled as currently VS code doesn't send a work done
                            // token on the diagnostic requests, only one for partial results;
                            // instead, we'll use a token created by the
                            // server to report progress.
                            // work_done_progress_options: WorkDoneProgressOptions {
                            //     work_done_progress: Some(true),
                            // },
                            ..Default::default()
                        },
                    )),
                    document_symbol_provider: Some(OneOf::Left(true)),
                    document_formatting_provider: Some(OneOf::Left(true)),
                    definition_provider: Some(OneOf::Left(true)),
                    references_provider: Some(OneOf::Left(true)),
                    completion_provider: Some(CompletionOptions {
                        resolve_provider: Some(false),
                        trigger_characters: Some(vec![
                            ".".to_string(),
                            "[".to_string(),
                            "#".to_string(),
                        ]),
                        ..Default::default()
                    }),
                    hover_provider: Some(HoverProviderCapability::Simple(true)),
                    rename_provider: Some(OneOf::Left(true)),
                    semantic_tokens_provider: Some(
                        SemanticTokensServerCapabilities::SemanticTokensOptions(
                            SemanticTokensOptions {
                                work_done_progress_options: Default::default(),
                                legend: SemanticTokensLegend {
                                    token_types: WDL_SEMANTIC_TOKEN_TYPES.to_vec(),
                                    token_modifiers: WDL_SEMANTIC_TOKEN_MODIFIERS.to_vec(),
                                },
                                range: Some(false),
                                full: Some(SemanticTokensFullOptions::Bool(true)),
                            },
                        ),
                    ),
                    signature_help_provider: Some(SignatureHelpOptions {
                        trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                        retrigger_characters: None,
                        work_done_progress_options: WorkDoneProgressOptions {
                            work_done_progress: Some(false),
                        },
                    }),
                    inlay_hint_provider: Some(OneOf::Left(true)),
                    call_hierarchy_provider: Some(CallHierarchyServerCapability::Simple(true)),
                    folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                    ..Default::default()
                },
                server_info: Some(info),
            })
        })
    }

    fn shutdown(&mut self, _: ()) -> BoxFuture<'static, Result<(), Self::Error>> {
        drop(self.message_tx.take());

        let message_loop_handle = self._message_loop_handle.take();
        Box::pin(async move {
            if let Some(handle) = message_loop_handle {
                handle.await.map_err(|e| {
                    ResponseError::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("message loop failed during shutdown: {e}"),
                    )
                })?;
            }

            Ok(())
        })
    }

    fn semantic_tokens_full(
        &mut self,
        params: SemanticTokensParams,
    ) -> BoxFuture<'static, Result<Option<SemanticTokensResult>, Self::Error>> {
        self.request(move |tx| Message::Request(Request::SemanticTokensFull { params, tx }))
    }

    fn inlay_hint(
        &mut self,
        params: InlayHintParams,
    ) -> BoxFuture<'static, Result<Option<Vec<InlayHint>>, Self::Error>> {
        self.request(move |tx| Message::Request(Request::InlayHint { params, tx }))
    }

    fn document_diagnostic(
        &mut self,
        params: DocumentDiagnosticParams,
    ) -> BoxFuture<'static, Result<DocumentDiagnosticReportResult, Self::Error>> {
        self.request(move |tx| Message::Request(Request::DocumentDiagnostic { params, tx }))
    }

    fn workspace_diagnostic(
        &mut self,
        params: WorkspaceDiagnosticParams,
    ) -> BoxFuture<'static, Result<WorkspaceDiagnosticReportResult, Self::Error>> {
        self.request(move |tx| Message::Request(Request::WorkspaceDiagnostic { params, tx }))
    }

    fn completion(
        &mut self,
        params: CompletionParams,
    ) -> BoxFuture<'static, Result<Option<CompletionResponse>, Self::Error>> {
        self.request(move |tx| Message::Request(Request::Completion { params, tx }))
    }

    fn prepare_call_hierarchy(
        &mut self,
        params: CallHierarchyPrepareParams,
    ) -> BoxFuture<'static, Result<Option<Vec<CallHierarchyItem>>, Self::Error>> {
        self.request(move |tx| Message::Request(Request::PrepareCallHierarchy { params, tx }))
    }

    fn incoming_calls(
        &mut self,
        params: CallHierarchyIncomingCallsParams,
    ) -> BoxFuture<'static, Result<Option<Vec<CallHierarchyIncomingCall>>, Self::Error>> {
        self.request(move |tx| Message::Request(Request::IncomingCalls { params, tx }))
    }

    fn outgoing_calls(
        &mut self,
        params: CallHierarchyOutgoingCallsParams,
    ) -> BoxFuture<'static, Result<Option<Vec<CallHierarchyOutgoingCall>>, Self::Error>> {
        self.request(move |tx| Message::Request(Request::OutgoingCalls { params, tx }))
    }

    fn hover(
        &mut self,
        params: HoverParams,
    ) -> BoxFuture<'static, Result<Option<Hover>, Self::Error>> {
        self.request(move |tx| Message::Request(Request::Hover { params, tx }))
    }

    fn signature_help(
        &mut self,
        params: SignatureHelpParams,
    ) -> BoxFuture<'static, Result<Option<SignatureHelp>, Self::Error>> {
        self.request(move |tx| Message::Request(Request::SignatureHelp { params, tx }))
    }

    fn folding_range(
        &mut self,
        params: FoldingRangeParams,
    ) -> BoxFuture<'static, Result<Option<Vec<FoldingRange>>, Self::Error>> {
        self.request(move |tx| Message::Request(Request::FoldingRange { params, tx }))
    }

    fn definition(
        &mut self,
        params: GotoDefinitionParams,
    ) -> BoxFuture<'static, Result<Option<GotoDefinitionResponse>, Self::Error>> {
        self.request(move |tx| Message::Request(Request::Definition { params, tx }))
    }

    fn references(
        &mut self,
        params: ReferenceParams,
    ) -> BoxFuture<'static, Result<Option<Vec<Location>>, Self::Error>> {
        self.request(move |tx| Message::Request(Request::References { params, tx }))
    }

    fn document_symbol(
        &mut self,
        params: DocumentSymbolParams,
    ) -> BoxFuture<'static, Result<Option<DocumentSymbolResponse>, Self::Error>> {
        self.request(move |tx| Message::Request(Request::DocumentSymbol { params, tx }))
    }

    fn symbol(
        &mut self,
        params: WorkspaceSymbolParams,
    ) -> BoxFuture<'static, Result<Option<WorkspaceSymbolResponse>, Self::Error>> {
        self.request(move |tx| Message::Request(Request::Symbol { params, tx }))
    }

    fn formatting(
        &mut self,
        params: DocumentFormattingParams,
    ) -> BoxFuture<'static, Result<Option<Vec<TextEdit>>, Self::Error>> {
        self.request(move |tx| Message::Request(Request::Formatting { params, tx }))
    }

    fn rename(
        &mut self,
        params: RenameParams,
    ) -> BoxFuture<'static, Result<Option<WorkspaceEdit>, Self::Error>> {
        self.request(move |tx| Message::Request(Request::Rename { params, tx }))
    }

    fn initialized(&mut self, _: InitializedParams) -> Self::NotifyResult {
        let info = self.options.info();
        let mut client = self.client.clone();
        let client_support = self.client_support.get().cloned().expect("should exist");
        tokio::task::spawn(async move {
            let mut registrations = Vec::new();
            if client_support.watched_files {
                registrations.push(Registration {
                    id: Uuid::new_v4().to_string(),
                    method: "workspace/didChangeWatchedFiles".into(),
                    register_options: Some(
                        to_value(DidChangeWatchedFilesRegistrationOptions {
                            watchers: vec![FileSystemWatcher {
                                // We use a generic glob so we can be notified for when directories,
                                // which might contain WDL documents, are deleted
                                glob_pattern: GlobPattern::String("**/*".to_string()),
                                kind: None,
                            }],
                        })
                        .expect("should convert to value"),
                    ),
                });
            }

            if client_support.did_change_configuration {
                registrations.push(Registration {
                    id: Uuid::new_v4().to_string(),
                    method: "workspace/didChangeConfiguration".into(),
                    register_options: None,
                });
            }

            if registrations.is_empty() {
                return;
            }

            client
                .register_capability(RegistrationParams { registrations })
                .await
                .expect("failed to register capabilities with client");

            info!(
                "{name} (v{version}) server initialized",
                name = info.name,
                version = info.version.expect("should exist")
            );
        });

        ControlFlow::Continue(())
    }

    fn did_change_workspace_folders(
        &mut self,
        params: DidChangeWorkspaceFoldersParams,
    ) -> Self::NotifyResult {
        self.queue(Message::Notification(
            Notification::DidChangeWorkspaceFolders(params),
        ))
    }

    fn did_change_configuration(
        &mut self,
        params: DidChangeConfigurationParams,
    ) -> Self::NotifyResult {
        self.queue(Message::Notification(Notification::DidChangeConfiguration(
            params,
        )))
    }

    fn did_open(&mut self, params: DidOpenTextDocumentParams) -> Self::NotifyResult {
        self.queue(Message::Notification(Notification::DidOpen(params)))
    }

    fn did_change(&mut self, params: DidChangeTextDocumentParams) -> Self::NotifyResult {
        self.queue(Message::Notification(Notification::DidChange(params)))
    }

    fn did_close(&mut self, params: DidCloseTextDocumentParams) -> Self::NotifyResult {
        self.queue(Message::Notification(Notification::DidClose(params)))
    }

    fn did_change_watched_files(
        &mut self,
        params: DidChangeWatchedFilesParams,
    ) -> Self::NotifyResult {
        self.queue(Message::Notification(Notification::DidChangeWatchedFiles(
            params,
        )))
    }
}
