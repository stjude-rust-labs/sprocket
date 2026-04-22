//! Implementation of the LSP server.

use std::ffi::OsStr;
use std::fmt::Formatter;
use std::mem;
use std::path::Component;
use std::path::PathBuf;
use std::path::Prefix;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::OnceLock;

use anyhow::Result;
use notification::Progress;
use parking_lot::RwLock;
use request::WorkDoneProgressCreate;
use serde::Deserialize;
use serde::Deserializer;
use serde_json::to_value;
use struct_patch::Patch;
use tower_lsp::Client;
use tower_lsp::LanguageServer;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Error as RpcError;
use tower_lsp::jsonrpc::ErrorCode;
use tower_lsp::jsonrpc::Result as RpcResult;
use tower_lsp::lsp_types::request::WorkspaceConfiguration;
use tower_lsp::lsp_types::*;
use tracing::debug;
use tracing::error;
use tracing::info;
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
    pub async fn new(client: &Client, client_supported: bool) -> Self {
        if !client_supported {
            return Self(None);
        }

        let token = Uuid::new_v4().to_string();
        if client
            .send_request::<WorkDoneProgressCreate>(WorkDoneProgressCreateParams {
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
    pub async fn start(
        &self,
        client: &Client,
        title: impl Into<String>,
        message: impl Into<String>,
    ) {
        if let Some(token) = &self.0 {
            client
                .send_notification::<Progress>(ProgressParams {
                    token: NumberOrString::String(token.clone()),
                    value: ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(
                        WorkDoneProgressBegin {
                            title: title.into(),
                            cancellable: None,
                            message: Some(message.into()),
                            percentage: Some(0),
                        },
                    )),
                })
                .await;
        }
    }

    /// Updates the work progress.
    pub async fn update(&self, client: &Client, message: impl Into<String>, percentage: u32) {
        if let Some(token) = &self.0 {
            client
                .send_notification::<Progress>(ProgressParams {
                    token: NumberOrString::String(token.clone()),
                    value: ProgressParamsValue::WorkDone(WorkDoneProgress::Report(
                        WorkDoneProgressReport {
                            cancellable: None,
                            message: Some(message.into()),
                            percentage: Some(percentage),
                        },
                    )),
                })
                .await;
        }
    }

    /// Completes the work progress.
    pub async fn complete(self, client: &Client, message: impl Into<String>) {
        if let Some(token) = self.0 {
            client
                .send_notification::<Progress>(ProgressParams {
                    token: NumberOrString::String(token),
                    value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(
                        WorkDoneProgressEnd {
                            message: Some(message.into()),
                        },
                    )),
                })
                .await;
        }
    }
}

// NOTE: Renamed camelCase to make it play nicely with the vscode extension.
/// Represents options for running the LSP server.
#[derive(Debug, Clone, Patch)]
#[patch(attribute(derive(Debug, Default, Deserialize)))]
#[patch(attribute(serde(rename_all = "camelCase")))]
#[patch(attribute(allow(missing_docs)))]
pub struct ServerOptions {
    /// The name of the server.
    ///
    /// Defaults to `wdl-lsp` crate name.
    #[patch(skip)]
    pub name: String,

    /// The version of the server.
    ///
    /// Defaults to the version of the `wdl-lsp` crate.
    #[patch(skip)]
    pub version: String,

    /// The verbosity level of the server.
    pub log_level: LevelFilter,

    /// The options for linting.
    #[patch(nesting)]
    pub lint: LintOptions,

    /// Analysis or lint rule IDs to except (ignore).
    pub exceptions: Vec<String>,

    /// Basename for any ignorefiles which should be respected.
    pub ignore_filename: Option<String>,

    /// Feature flags for enabling experimental features.
    #[patch(skip)]
    pub feature_flags: FeatureFlags,

    /// The diagnostic baseline for suppressing known diagnostics.
    #[patch(skip)]
    pub baseline: Option<wdl_lint::Baseline>,
}

impl Default for ServerOptions {
    fn default() -> Self {
        Self {
            name: String::from(env!("CARGO_CRATE_NAME")),
            version: String::from(env!("CARGO_PKG_VERSION")),
            log_level: LevelFilter(tracing::metadata::LevelFilter::ERROR),
            lint: Default::default(),
            exceptions: Vec::new(),
            ignore_filename: None,
            feature_flags: Default::default(),
            baseline: None,
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
pub type FilterReloadHandle<S> =
    tracing_subscriber::reload::Handle<tracing::metadata::LevelFilter, S>;

/// Represents an LSP server for analyzing WDL documents.
#[derive(Debug)]
pub struct Server<S> {
    /// The LSP client connected to the server.
    client: Client,
    /// The features supported by the LSP client.
    client_support: OnceLock<ClientSupport>,
    /// The current set of workspace folders.
    folders: Arc<RwLock<Vec<WorkspaceFolder>>>,
    /// Mutable configuration fields.
    config: Arc<tokio::sync::RwLock<ServerConfig>>,
    /// Level filter reload handle.
    log_handle: Option<FilterReloadHandle<S>>,
}

/// The server config and dependent fields.
#[derive(Debug)]
struct ServerConfig {
    /// The options for the server.
    options: ServerOptions,
    /// The analyzer used to analyze documents.
    analyzer: Analyzer<ProgressToken>,
}

impl ServerOptions {
    /// Create an [`Analyzer`] based on this config.
    fn analyzer(&self, client: Client) -> Analyzer<ProgressToken> {
        let linting_enabled = self.lint.enabled;
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

        let wdl_lint_config = self.lint.config.clone();
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
                    token.update(&client, message, percentage).await
                }
            },
            move || {
                let mut validator = Validator::default();
                if linting_enabled {
                    validator.add_visitor(Linter::new(
                        wdl_lint::rules(&wdl_lint_config)
                            .into_iter()
                            .filter(|r| !exceptions.contains(&r.id().into())),
                    ));
                }
                validator
            },
        )
    }
}

impl<S: 'static> Server<S> {
    /// Creates a new WDL language server.
    ///
    /// `log_handle` can be provided to enable dynamic log level setting.
    pub fn new(
        client: Client,
        options: ServerOptions,
        log_handle: Option<FilterReloadHandle<S>>,
    ) -> Self {
        let analyzer = options.analyzer(client.clone());
        Self {
            client,
            client_support: Default::default(),
            folders: Default::default(),
            config: Arc::new(tokio::sync::RwLock::new(ServerConfig { options, analyzer })),
            log_handle,
        }
    }

    /// Patch the config with the new values from the client.
    async fn apply_config_patch(&self, patch: ServerOptionsPatch) {
        let mut config = self.config.write().await;
        if let Some(log_level) = patch.log_level
            && let Some(reload_handle) = self.log_handle.as_ref()
            && let Err(e) = reload_handle.modify(|filter| *filter = log_level.0)
        {
            error!("failed to set log level: {e:?}");
        }

        config.options.apply(patch);
        config.analyzer = config.options.analyzer(self.client.clone());
    }

    /// Runs the server until a request is received to shut down.
    ///
    /// See also: [`Self::new()`]
    pub async fn run(
        options: ServerOptions,
        log_handle: Option<FilterReloadHandle<S>>,
    ) -> Result<()> {
        debug!("running LSP server: {options:#?}");

        let (service, socket) = LspService::new(|client| Self::new(client, options, log_handle));

        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        tower_lsp::Server::new(stdin, stdout, socket)
            .serve(service)
            .await;

        Ok(())
    }

    /// Get info about the server.
    async fn info(&self) -> ServerInfo {
        let config = self.config.read().await;

        ServerInfo {
            name: config.options.name.clone(),
            version: Some(config.options.version.clone()),
        }
    }

    /// Registers a generic watcher for all files/directories in the workspace.
    async fn register_capabilities(&self, client_support: &ClientSupport) {
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

        self.client
            .register_capability(registrations)
            .await
            .expect("failed to register capabilities with client");
    }
}

#[tower_lsp::async_trait]
impl<S: 'static> LanguageServer for Server<S> {
    async fn initialize(&self, params: InitializeParams) -> RpcResult<InitializeResult> {
        debug!("received `initialize` request: {params:#?}");

        if let Some(folders) = params.workspace_folders {
            let config = self.config.read().await;
            for mut folder in folders {
                normalize_uri_path(&mut folder.uri);
                self.folders.write().push(folder.clone());
                if let Ok(path) = folder.uri.to_file_path()
                    && let Err(e) = config.analyzer.add_directory(path).await
                {
                    error!(
                        "failed to add initial workspace directory {uri}: {e}",
                        uri = folder.uri
                    );
                }
            }
        }

        {
            let client_support = ClientSupport::new(&params.capabilities);

            if !client_support.pull_diagnostics {
                return Err(RpcError {
                    code: ErrorCode::ServerError(0),
                    message: "LSP server currently requires support for pulling diagnostics".into(),
                    data: None,
                });
            }

            // This is guaranteed to be called once anyway
            let _ = self.client_support.set(client_support);
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
                        // token on the diagnostic requests, only one for partial results; instead,
                        // we'll use a token created by the server to report progress.
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
                ..Default::default()
            },
            server_info: Some(self.info().await),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        let client_support = self.client_support.get().expect("should exist");
        self.register_capabilities(client_support).await;

        let info = self.info().await;
        info!(
            "{name} (v{version}) server initialized",
            name = info.name,
            version = info.version.expect("should exist")
        );
    }

    async fn shutdown(&self) -> RpcResult<()> {
        Ok(())
    }

    async fn did_open(&self, mut params: DidOpenTextDocumentParams) {
        normalize_uri_path(&mut params.text_document.uri);

        debug!("received `textDocument/didOpen` request: {params:#?}");

        let config = self.config.read().await;
        if let Err(e) = config
            .analyzer
            .add_document(params.text_document.uri.clone())
            .await
        {
            error!(
                "failed to add document {uri}: {e}",
                uri = params.text_document.uri
            );
            return;
        }

        if let Err(e) = config.analyzer.notify_incremental_change(
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

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        let config = self.config.read().await;

        normalize_uri_path(&mut params.text_document.uri);

        debug!("received `textDocument/didChange` request: {params:#?}");

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

        // Notify the analyzer that the document has changed
        if let Err(e) = config.analyzer.notify_incremental_change(
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

    async fn did_close(&self, mut params: DidCloseTextDocumentParams) {
        let config = self.config.read().await;

        normalize_uri_path(&mut params.text_document.uri);

        debug!("received `textDocument/didClose` request: {params:#?}");
        if let Err(e) = config
            .analyzer
            .notify_change(params.text_document.uri, true)
        {
            error!("failed to notify change: {e}");
        }
    }

    async fn diagnostic(
        &self,
        mut params: DocumentDiagnosticParams,
    ) -> RpcResult<DocumentDiagnosticReportResult> {
        let config = self.config.read().await;

        normalize_uri_path(&mut params.text_document.uri);

        debug!("received `textDocument/diagnostic` request: {params:#?}");

        let results: Vec<wdl_analysis::AnalysisResult> = config
            .analyzer
            .analyze_document(ProgressToken::default(), params.text_document.uri.clone())
            .await
            .map_err(|e| RpcError {
                code: ErrorCode::InternalError,
                message: e.to_string().into(),
                data: None,
            })?;

        drop(config);
        let name = self.info().await.name;
        let config = self.config.read().await;
        let mut matcher = config.options.baseline.as_ref().map(|b| b.matcher());
        proto::document_diagnostic_report(params, results, &name, matcher.as_mut())
            .ok_or_else(RpcError::request_cancelled)
    }

    async fn workspace_diagnostic(
        &self,
        params: WorkspaceDiagnosticParams,
    ) -> RpcResult<WorkspaceDiagnosticReportResult> {
        let config = self.config.read().await;

        debug!("received `workspace/diagnostic` request: {params:#?}");

        let name = self.info().await.name;

        let client_support = self.client_support.get().expect("should exist");
        let progress = ProgressToken::new(&self.client, client_support.work_done_progress).await;
        progress
            .start(&self.client, name.clone(), "analyzing...")
            .await;
        let results = config
            .analyzer
            .analyze(progress.clone())
            .await
            .map_err(|e| RpcError {
                code: ErrorCode::InternalError,
                message: e.to_string().into(),
                data: None,
            })?;
        progress.complete(&self.client, "analysis complete").await;

        let mut matcher = config.options.baseline.as_ref().map(|b| b.matcher());
        Ok(proto::workspace_diagnostic_report(
            params,
            results,
            &name,
            matcher.as_mut(),
        ))
    }

    async fn did_change_workspace_folders(&self, params: DidChangeWorkspaceFoldersParams) {
        let config = self.config.read().await;

        debug!("received `workspace/didChangeWorkspaceFolders` request: {params:#?}");

        // Process the removed folders
        if !params.event.removed.is_empty()
            && let Err(e) = config
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
                if let Err(e) = config
                    .analyzer
                    .add_directory(folder.uri.to_file_path().expect("should be a file path"))
                    .await
                {
                    error!("failed to add documents from directory to analyzer: {e}");
                }
            }
        }
    }

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        debug!("received `workspace/didChangeConfiguration` notification: {params:#?}");

        let workspace_configs = self
            .client
            .send_request::<WorkspaceConfiguration>(ConfigurationParams {
                items: vec![ConfigurationItem {
                    scope_uri: None,
                    section: Some(String::from("sprocket.server")),
                }],
            })
            .await;

        match workspace_configs {
            Ok(mut configs) if !configs.is_empty() => {
                match serde_json::from_value::<ServerOptionsPatch>(configs.remove(0)) {
                    Ok(patch) => self.apply_config_patch(patch).await,
                    Err(e) => error!("failed to deserialize `ServerOptionsPatch`: {e:?}"),
                }
            }
            Ok(_) => error!("client returned no configuration"),
            Err(e) => error!("failed to fetch workspace configuration: {e}"),
        }
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        let config = self.config.read().await;

        debug!("received `workspace/didChangeWatchedFiles` request: {params:#?}");

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
                    if let Some(path) = to_wdl_file_path(&event.uri) {
                        debug!("document `{uri}` has been created", uri = event.uri);
                        added.push(path);
                    }
                }
                FileChangeType::CHANGED => {
                    if to_wdl_file_path(&event.uri).is_some() {
                        debug!("document `{uri}` has been changed", uri = event.uri);
                        if let Err(e) = config.analyzer.notify_change(event.uri, false) {
                            error!("failed to notify change: {e}");
                        }
                    }
                }
                FileChangeType::DELETED => {
                    if to_wdl_file_path(&event.uri).is_some() {
                        debug!("document `{uri}` has been deleted", uri = event.uri);
                        deleted.push(event.uri);
                    }
                }
                _ => continue,
            }
        }

        // Add any documents to the analyzer
        if !added.is_empty() {
            for file in added {
                if let Err(e) = config
                    .analyzer
                    .add_document(path_to_uri(&file).expect("should convert to uri"))
                    .await
                {
                    error!("failed to add documents to analyzer: {e}");
                }
            }
        }

        // Remove any documents from the analyzer
        if !deleted.is_empty()
            && let Err(e) = config.analyzer.remove_documents(deleted).await
        {
            error!("failed to remove documents from analyzer: {e}");
        }
    }

    async fn formatting(
        &self,
        mut params: DocumentFormattingParams,
    ) -> RpcResult<Option<Vec<TextEdit>>> {
        let config = self.config.read().await;

        normalize_uri_path(&mut params.text_document.uri);

        debug!("received `textDocument/formatting` request: {params:#?}");

        let result = config
            .analyzer
            .format_document(params.text_document.uri)
            .await
            .map_err(|e| RpcError {
                code: ErrorCode::InternalError,
                message: e.to_string().into(),
                data: None,
            })?
            .map(|(end_line, end_col, formatted)| {
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
            });

        Ok(result)
    }

    async fn goto_definition(
        &self,
        mut params: GotoDefinitionParams,
    ) -> RpcResult<Option<GotoDefinitionResponse>> {
        let config = self.config.read().await;

        normalize_uri_path(&mut params.text_document_position_params.text_document.uri);

        debug!("received `textDocument/gotoDefinition` request: {params:#?}");

        let position = SourcePosition::new(
            params.text_document_position_params.position.line,
            params.text_document_position_params.position.character,
        );

        let result = config
            .analyzer
            .goto_definition(
                params.text_document_position_params.text_document.uri,
                position,
                SourcePositionEncoding::UTF16,
            )
            .await
            .map_err(|e| RpcError {
                code: ErrorCode::InternalError,
                message: e.to_string().into(),
                data: None,
            })?;

        Ok(result)
    }

    async fn references(&self, mut params: ReferenceParams) -> RpcResult<Option<Vec<Location>>> {
        let config = self.config.read().await;

        normalize_uri_path(&mut params.text_document_position.text_document.uri);

        debug!("received `textDocument/references` request: {params:#?}");

        let position = SourcePosition::new(
            params.text_document_position.position.line,
            params.text_document_position.position.character,
        );

        let result = config
            .analyzer
            .find_all_references(
                params.text_document_position.text_document.uri,
                position,
                SourcePositionEncoding::UTF16,
                params.context.include_declaration,
            )
            .await
            .map_err(|e| RpcError {
                code: ErrorCode::InternalError,
                message: e.to_string().into(),
                data: None,
            })?;

        Ok(Some(result))
    }

    async fn completion(
        &self,
        mut params: CompletionParams,
    ) -> RpcResult<Option<CompletionResponse>> {
        let config = self.config.read().await;

        normalize_uri_path(&mut params.text_document_position.text_document.uri);

        debug!("received `textDocument/completion` request: {params:#?}");

        let position = SourcePosition::new(
            params.text_document_position.position.line,
            params.text_document_position.position.character,
        );

        let result = config
            .analyzer
            .completion(
                ProgressToken::default(),
                params.text_document_position.text_document.uri,
                position,
                SourcePositionEncoding::UTF16,
            )
            .await
            .map_err(|e| RpcError {
                code: ErrorCode::InternalError,
                message: e.to_string().into(),
                data: None,
            })?;

        Ok(result)
    }

    async fn hover(&self, mut params: HoverParams) -> RpcResult<Option<Hover>> {
        let config = self.config.read().await;

        normalize_uri_path(&mut params.text_document_position_params.text_document.uri);

        debug!("received `textDocument/hover` request: {params:#?}");

        let position = SourcePosition::new(
            params.text_document_position_params.position.line,
            params.text_document_position_params.position.character,
        );

        let result = config
            .analyzer
            .hover(
                params.text_document_position_params.text_document.uri,
                position,
                SourcePositionEncoding::UTF16,
            )
            .await
            .map_err(|e| RpcError {
                code: ErrorCode::InternalError,
                message: e.to_string().into(),
                data: None,
            })?;
        Ok(result)
    }

    async fn rename(&self, mut params: RenameParams) -> RpcResult<Option<WorkspaceEdit>> {
        let config = self.config.read().await;

        normalize_uri_path(&mut params.text_document_position.text_document.uri);

        debug!("received `textDocument/rename` request: {params:#?}");

        let position = SourcePosition::new(
            params.text_document_position.position.line,
            params.text_document_position.position.character,
        );

        let result = config
            .analyzer
            .rename(
                params.text_document_position.text_document.uri,
                position,
                SourcePositionEncoding::UTF16,
                params.new_name,
            )
            .await
            .map_err(|e| RpcError {
                code: ErrorCode::InternalError,
                message: e.to_string().into(),
                data: None,
            })?;

        Ok(result)
    }

    async fn semantic_tokens_full(
        &self,
        mut params: SemanticTokensParams,
    ) -> RpcResult<Option<SemanticTokensResult>> {
        let config = self.config.read().await;

        normalize_uri_path(&mut params.text_document.uri);

        debug!("received `textDocument/semanticTokens/full` request: {params:#?}");

        let result = config
            .analyzer
            .semantic_tokens(params.text_document.uri)
            .await
            .map_err(|e| RpcError {
                code: ErrorCode::InternalError,
                message: e.to_string().into(),
                data: None,
            })?;

        Ok(result)
    }

    async fn document_symbol(
        &self,
        mut params: DocumentSymbolParams,
    ) -> RpcResult<Option<DocumentSymbolResponse>> {
        let config = self.config.read().await;

        normalize_uri_path(&mut params.text_document.uri);

        debug!("received `textDocument/documentSymbol` request: {params:#?}");

        let result = config
            .analyzer
            .document_symbol(params.text_document.uri)
            .await
            .map_err(|e| RpcError {
                code: ErrorCode::InternalError,
                message: e.to_string().into(),
                data: None,
            })?;

        Ok(result)
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> RpcResult<Option<Vec<SymbolInformation>>> {
        let config = self.config.read().await;

        debug!("received `workspace/symbol` request: {params:#?}");

        let result = config
            .analyzer
            .workspace_symbol(params.query)
            .await
            .map_err(|e| RpcError {
                code: ErrorCode::InternalError,
                message: e.to_string().into(),
                data: None,
            })?;

        Ok(result)
    }

    async fn signature_help(
        &self,
        mut params: SignatureHelpParams,
    ) -> RpcResult<Option<SignatureHelp>> {
        let config = self.config.read().await;

        normalize_uri_path(&mut params.text_document_position_params.text_document.uri);

        debug!("received `textDocument/signatureHelp` request: {params:#?}");

        let position = SourcePosition::new(
            params.text_document_position_params.position.line,
            params.text_document_position_params.position.character,
        );

        let result = config
            .analyzer
            .signature_help(
                params.text_document_position_params.text_document.uri,
                position,
                SourcePositionEncoding::UTF16,
            )
            .await
            .map_err(|e| RpcError {
                code: ErrorCode::InternalError,
                message: e.to_string().into(),
                data: None,
            })?;

        Ok(result)
    }

    async fn inlay_hint(&self, mut params: InlayHintParams) -> RpcResult<Option<Vec<InlayHint>>> {
        let config = self.config.read().await;

        normalize_uri_path(&mut params.text_document.uri);

        debug!("received `textDocument/inlayHint` request: {params:#?}");

        // Analyze the document first to ensure we have up-to-date information
        config
            .analyzer
            .analyze(ProgressToken(None))
            .await
            .map_err(|e| RpcError {
                code: ErrorCode::InternalError,
                message: e.to_string().into(),
                data: None,
            })?;

        let result = config
            .analyzer
            .inlay_hints(params.text_document.uri, params.range)
            .await
            .map_err(|e| RpcError {
                code: ErrorCode::InternalError,
                message: e.to_string().into(),
                data: None,
            })?;

        Ok(result)
    }
}
