//! A Language Server Protocol implementation for analyzing WDL documents.

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::missing_docs_in_private_items)]
#![warn(rustdoc::broken_intra_doc_links)]

use std::collections::HashMap;
use std::mem;
use std::sync::Arc;

use anyhow::Result;
use notification::Progress;
use parking_lot::RwLock;
use request::WorkDoneProgressCreate;
use serde_json::to_value;
use tower_lsp::jsonrpc::Error as RpcError;
use tower_lsp::jsonrpc::ErrorCode;
use tower_lsp::jsonrpc::Result as RpcResult;
use tower_lsp::lsp_types::*;
use tower_lsp::Client;
use tower_lsp::LanguageServer;
use tower_lsp::LspService;
use uuid::Uuid;
use wdl_analysis::Analyzer;
use wdl_analysis::DocumentChange;
use wdl_analysis::ProgressKind;
use wdl_ast::Validator;
use wdl_lint::LintVisitor;
use workspace::ClientSupport;
use workspace::Workspace;

mod proto;
mod workspace;

/// Constructs a new validator.
///
/// This is used as a callback to the analyzer.
fn validator(lint: bool) -> Validator {
    let mut validator = Validator::default();
    if lint {
        validator.add_visitor(LintVisitor::default());
    }

    validator
}

/// Takes the requested changes for the given document.
///
/// This is used as a callback to the analyzer.
fn changes(workspace: &Arc<RwLock<Workspace>>, uri: &Url) -> Option<DocumentChange> {
    let mut workspace = workspace.write();
    if let Some(document) = workspace.documents.get_mut(uri) {
        return document.take_change();
    }

    None
}

/// Represents options for running the LSP server.
#[derive(Debug, Default)]
pub struct ServerOptions {
    /// The name of the server.
    ///
    /// Defaults to `wdl-lsp` crate name.
    pub name: Option<String>,

    /// The version of the server.
    ///
    /// Defaults to the version of the `wdl-lsp` crate.
    pub version: Option<String>,

    /// Whether or not linting is enabled.
    pub lint: bool,
}

/// Represents an LSP server for analyzing WDL documents.
#[derive(Debug)]
pub struct Server {
    /// The LSP client connected to the server.
    client: Client,
    /// The options for the server.
    options: ServerOptions,
    /// The analyzer used to analyze documents.
    analyzer: Analyzer,
    /// The workspace managed by the server.
    workspace: Arc<RwLock<Workspace>>,
}

impl Server {
    /// Runs the server until a request is received to shut down.
    pub async fn run(options: ServerOptions) -> Result<()> {
        log::debug!("running LSP server: {options:#?}");

        let (service, socket) = LspService::new(|client| {
            let lint = options.lint;
            let workspace: Arc<RwLock<Workspace>> = Default::default();
            let analyzer_client = client.clone();
            let analyzer_workspace = workspace.clone();

            Self {
                client,
                options,
                analyzer: Analyzer::new_with_changes(
                    move |kind, current, total, context| {
                        Self::report_analysis_progress(
                            analyzer_client.clone(),
                            context.clone(),
                            kind,
                            current,
                            total,
                        )
                    },
                    move || validator(lint),
                    move |uri| changes(&analyzer_workspace, uri),
                ),

                workspace,
            }
        });

        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        tower_lsp::Server::new(stdin, stdout, socket)
            .serve(service)
            .await;

        Ok(())
    }

    /// Gets the name of the server.
    fn name(&self) -> &str {
        self.options
            .name
            .as_deref()
            .unwrap_or(env!("CARGO_CRATE_NAME"))
    }

    /// Gets the version of the server.
    fn version(&self) -> &str {
        self.options
            .version
            .as_deref()
            .unwrap_or(env!("CARGO_PKG_VERSION"))
    }

    /// Registers a generic watcher for all files/directories in the workspace.
    async fn register_watcher(&self) {
        self.client
            .register_capability(vec![Registration {
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
            }])
            .await
            .expect("failed to register capabilities with client");
    }

    /// Starts an analysis task by creating a work done token.
    ///
    /// If the client returned an error during token creation, `None` is
    /// returned.
    async fn start_analysis_task(&self) -> Option<String> {
        let token = Uuid::new_v4().to_string();
        if self
            .client
            .send_request::<WorkDoneProgressCreate>(WorkDoneProgressCreateParams {
                token: NumberOrString::String(token.clone()),
            })
            .await
            .is_err()
        {
            return None;
        }

        self.client
            .send_notification::<Progress>(ProgressParams {
                token: NumberOrString::String(token.clone()),
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(
                    WorkDoneProgressBegin {
                        title: self.name().to_string(),
                        cancellable: None,
                        message: Some("analyzing...".to_string()),
                        percentage: Some(0),
                    },
                )),
            })
            .await;

        Some(token)
    }

    /// Reports analysis progress to the client.
    async fn report_analysis_progress(
        client: Client,
        token: Option<String>,
        kind: ProgressKind,
        current: usize,
        total: usize,
    ) {
        if let Some(token) = token {
            client
                .send_notification::<Progress>(ProgressParams {
                    token: NumberOrString::String(token),
                    value: ProgressParamsValue::WorkDone(WorkDoneProgress::Report(
                        WorkDoneProgressReport {
                            cancellable: None,
                            message: Some(format!(
                                "{kind} {current}/{total} file{s}",
                                s = if total > 1 { "s" } else { "" }
                            )),
                            percentage: Some(((current * 100) as f64 / total as f64) as u32),
                        },
                    )),
                })
                .await;
        }
    }

    /// Completes an analysis task with the given token.
    async fn complete_analysis_task(&self, token: String) {
        self.client
            .send_notification::<Progress>(ProgressParams {
                token: NumberOrString::String(token),
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(WorkDoneProgressEnd {
                    message: Some("analysis complete".to_string()),
                })),
            })
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Server {
    async fn initialize(&self, params: InitializeParams) -> RpcResult<InitializeResult> {
        log::debug!("received `initialize` request: {params:#?}");

        let mut workspace = self.workspace.write();
        workspace.folders = params
            .workspace_folders
            .unwrap_or_default()
            .into_iter()
            .collect();

        workspace.client_support = ClientSupport::new(&params.capabilities);

        if !workspace.client_support.pull_diagnostics {
            return Err(RpcError {
                code: ErrorCode::ServerError(0),
                message: "LSP server currently requires support for pulling diagnostics".into(),
                data: None,
            });
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
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: self.name().to_string(),
                version: Some(self.version().to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        if self.workspace.read().client_support.watched_files {
            self.register_watcher().await;
        }

        // Process the initial workspace folders
        let folders = mem::take(&mut self.workspace.write().folders);
        if !folders.is_empty() {
            self.did_change_workspace_folders(DidChangeWorkspaceFoldersParams {
                event: WorkspaceFoldersChangeEvent {
                    added: folders,
                    removed: Vec::new(),
                },
            })
            .await;
        }

        log::info!(
            "{name} (v{version}) server initialized",
            name = self
                .options
                .name
                .as_deref()
                .unwrap_or(env!("CARGO_CRATE_NAME")),
            version = self
                .options
                .version
                .as_deref()
                .unwrap_or(env!("CARGO_PKG_VERSION"))
        );
    }

    async fn shutdown(&self) -> RpcResult<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        log::debug!("received `textDocument/didOpen` request: {params:#?}");
        self.workspace.write().on_document_open(params);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        log::debug!("received `textDocument/didChange` request: {params:#?}");
        self.workspace.write().on_document_change(params);
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        log::debug!("received `textDocument/didClose` request: {params:#?}");
        self.workspace.write().on_document_close(params);
    }

    async fn diagnostic(
        &self,
        params: DocumentDiagnosticParams,
    ) -> RpcResult<DocumentDiagnosticReportResult> {
        log::debug!("received `textDocument/diagnostic` request: {params:#?}");

        let (cached, with_progress) = {
            let workspace = self.workspace.read();
            (
                workspace
                    .documents
                    .get(&params.text_document.uri)
                    .and_then(|d| d.cached()),
                workspace.client_support.work_done_progress,
            )
        };

        if let Some(cached) = cached {
            return cached.document_diagnostic_report(self.name(), params.previous_result_id);
        }

        log::debug!(
            "document `{uri}` will be analyzed for diagnostics",
            uri = params.text_document.uri
        );

        let token = if with_progress {
            self.start_analysis_task().await
        } else {
            None
        };

        let documents = vec![Arc::new(params.text_document.uri.clone())];
        let results = self.analyzer.analyze(documents, token.clone()).await;
        if let Some(token) = token {
            self.complete_analysis_task(token).await;
        }

        self.workspace
            .write()
            .on_document_diagnostics_results(self.name(), params, results)
    }

    async fn workspace_diagnostic(
        &self,
        params: WorkspaceDiagnosticParams,
    ) -> RpcResult<WorkspaceDiagnosticReportResult> {
        log::debug!("received `workspace/diagnostic` request: {params:#?}");

        let mut ids = params
            .previous_result_ids
            .into_iter()
            .map(|id| (id.uri, id.value))
            .collect::<HashMap<_, _>>();

        let mut items = Vec::new();
        let mut documents = Vec::new();

        let with_progress = {
            let workspace = self.workspace.read();
            for (uri, document) in &workspace.documents {
                if let Some(cached) = document.cached() {
                    items.push(cached.workspace_diagnostic_report(
                        self.name(),
                        uri,
                        ids.remove(uri),
                    )?);
                } else {
                    log::debug!("document `{uri}` will be analyzed for diagnostics");
                    documents.push(uri.clone());
                }
            }

            workspace.client_support.work_done_progress
        };

        if !documents.is_empty() {
            let token = if with_progress {
                self.start_analysis_task().await
            } else {
                None
            };

            let results = self.analyzer.analyze(documents, token.clone()).await;
            if let Some(token) = token {
                self.complete_analysis_task(token).await;
            }

            self.workspace.write().on_workspace_diagnostics_results(
                self.name(),
                results,
                &mut items,
            );
        }

        Ok(WorkspaceDiagnosticReportResult::Report(
            WorkspaceDiagnosticReport { items },
        ))
    }

    async fn did_change_workspace_folders(&self, params: DidChangeWorkspaceFoldersParams) {
        log::debug!("received `workspace/didChangeWorkspaceFolders` request: {params:#?}");

        // Find the documents for the newly added workspace folders
        let documents = Analyzer::find_documents(
            params
                .event
                .added
                .iter()
                .filter_map(|f| f.uri.to_file_path().ok())
                .collect(),
        )
        .await;

        self.workspace
            .write()
            .on_workspace_folders_changed(params.event, documents);
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        log::debug!("received `workspace/didChangeWatchedFiles` request: {params:#?}");
        self.workspace
            .write()
            .on_watched_files_changed(&params.changes);

        let documents = params
            .changes
            .into_iter()
            .filter_map(|e| {
                if e.typ == FileChangeType::DELETED {
                    Some(e.uri)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        // Remove any documents from the analyzer
        if !documents.is_empty() {
            self.analyzer.remove_documents(documents).await;
        }
    }
}
