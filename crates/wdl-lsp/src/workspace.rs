//! Represents state relating to a workspace.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::mem;
use std::sync::Arc;

use anyhow::Result;
use tower_lsp::jsonrpc::Error as RpcError;
use tower_lsp::jsonrpc::Result as RpcResult;
use tower_lsp::lsp_types::ClientCapabilities;
use tower_lsp::lsp_types::DidChangeTextDocumentParams;
use tower_lsp::lsp_types::DidCloseTextDocumentParams;
use tower_lsp::lsp_types::DidOpenTextDocumentParams;
use tower_lsp::lsp_types::DocumentDiagnosticParams;
use tower_lsp::lsp_types::DocumentDiagnosticReport;
use tower_lsp::lsp_types::DocumentDiagnosticReportKind;
use tower_lsp::lsp_types::DocumentDiagnosticReportResult;
use tower_lsp::lsp_types::FileChangeType;
use tower_lsp::lsp_types::FileEvent;
use tower_lsp::lsp_types::FullDocumentDiagnosticReport;
use tower_lsp::lsp_types::RelatedFullDocumentDiagnosticReport;
use tower_lsp::lsp_types::RelatedUnchangedDocumentDiagnosticReport;
use tower_lsp::lsp_types::TextDocumentContentChangeEvent;
use tower_lsp::lsp_types::UnchangedDocumentDiagnosticReport;
use tower_lsp::lsp_types::WorkspaceDocumentDiagnosticReport;
use tower_lsp::lsp_types::WorkspaceFolder;
use tower_lsp::lsp_types::WorkspaceFoldersChangeEvent;
use tower_lsp::lsp_types::WorkspaceFullDocumentDiagnosticReport;
use tower_lsp::lsp_types::WorkspaceUnchangedDocumentDiagnosticReport;
use url::Url;
use uuid::Uuid;
use wdl_analysis::AnalysisResult;
use wdl_analysis::DocumentChange;
use wdl_analysis::SourceEdit;
use wdl_analysis::SourcePosition;
use wdl_analysis::SourcePositionEncoding;

use crate::proto;

/// Represents a document version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentVersion {
    /// The version is provided by the client.
    ///
    /// This indicates that the file is being managed by the client.
    Client(i32),
    /// The version is provided by the server.
    ///
    /// This indicates that the file contents have been read from disk.
    Server(i32),
}

impl DocumentVersion {
    /// Gets the client version.
    ///
    /// Returns `None` if the version was not provided by the client.
    pub fn client(&self) -> Option<i32> {
        match self {
            Self::Client(v) => Some(*v),
            Self::Server(_) => None,
        }
    }
}

impl Default for DocumentVersion {
    fn default() -> Self {
        Self::Server(0)
    }
}

/// Represents cached analysis result.
#[derive(Debug, Clone)]
pub struct CachedResult {
    /// The id of the cache result.
    id: String,
    /// The document version of the result.
    version: DocumentVersion,
    /// The cached analysis result.
    result: AnalysisResult,
}

impl CachedResult {
    /// Creates a new cache result.
    pub fn new(version: DocumentVersion, result: AnalysisResult) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            version,
            result,
        }
    }

    /// Creates a document diagnostic result from the cached result.
    pub fn document_diagnostic_report(
        &self,
        source: &str,
        previous: Option<String>,
    ) -> RpcResult<DocumentDiagnosticReportResult> {
        if let Some(previous) = previous {
            if previous == self.id {
                log::debug!(
                    "diagnostics for document `{uri}` have not changed (client has latest report)",
                    uri = self.result.uri()
                );
                return Ok(DocumentDiagnosticReportResult::Report(
                    DocumentDiagnosticReport::Unchanged(RelatedUnchangedDocumentDiagnosticReport {
                        related_documents: None,
                        unchanged_document_diagnostic_report: UnchangedDocumentDiagnosticReport {
                            result_id: previous,
                        },
                    }),
                ));
            }
        }

        log::debug!(
            "diagnostics for document `{uri}` have not changed (client needs report)",
            uri = self.result.uri()
        );

        let items = self
            .result
            .diagnostics()
            .iter()
            .map(|d| {
                proto::diagnostic(
                    self.result.uri(),
                    self.result
                        .parse_result()
                        .lines()
                        .expect("should have line index"),
                    source,
                    d,
                )
            })
            .collect::<Result<Vec<_>>>()
            .map_err(|_| RpcError::internal_error())?;

        Ok(DocumentDiagnosticReportResult::Report(
            DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                related_documents: None,
                full_document_diagnostic_report: FullDocumentDiagnosticReport {
                    result_id: Some(self.id.clone()),
                    items,
                },
            }),
        ))
    }

    /// Creates a workspace diagnostic report from the cached result.
    pub fn workspace_diagnostic_report(
        &self,
        source: &str,
        uri: &Url,
        previous: Option<String>,
    ) -> RpcResult<WorkspaceDocumentDiagnosticReport> {
        let version = self.version.client().map(Into::into);
        if let Some(previous) = previous {
            if previous == self.id {
                log::debug!(
                    "workspace diagnostics for document `{uri}` have not changed (client has \
                     latest report)",
                    uri = self.result.uri()
                );
                return Ok(WorkspaceDocumentDiagnosticReport::Unchanged(
                    WorkspaceUnchangedDocumentDiagnosticReport {
                        uri: uri.clone(),
                        version,
                        unchanged_document_diagnostic_report: UnchangedDocumentDiagnosticReport {
                            result_id: previous,
                        },
                    },
                ));
            }
        }

        log::debug!(
            "workspace diagnostics for document `{uri}` have not changed (client needs report)",
            uri = self.result.uri()
        );

        let items = self
            .result
            .diagnostics()
            .iter()
            .map(|d| {
                proto::diagnostic(
                    self.result.uri(),
                    self.result
                        .parse_result()
                        .lines()
                        .expect("should have line index"),
                    source,
                    d,
                )
            })
            .collect::<Result<Vec<_>>>()
            .map_err(|_| RpcError::internal_error())?;

        Ok(WorkspaceDocumentDiagnosticReport::Full(
            WorkspaceFullDocumentDiagnosticReport {
                uri: uri.clone(),
                version,
                full_document_diagnostic_report: FullDocumentDiagnosticReport {
                    result_id: Some(self.id.clone()),
                    items,
                },
            },
        ))
    }
}

/// Represents a document in the workspace.
#[derive(Debug, Default)]
pub struct Document {
    /// The current version of the document.
    version: DocumentVersion,
    /// The cached analysis result for the document.
    ///
    /// A value of `None` indicates the document has never been analyzed.
    cached: Option<CachedResult>,
    /// The current change to the document.
    change: Option<DocumentChange>,
}

impl Document {
    /// Applies the given set of change events to the document.
    pub fn apply_changes(
        &mut self,
        version: i32,
        mut changes: Vec<TextDocumentContentChangeEvent>,
    ) {
        // The specified version is for the entire set of changes
        self.version = DocumentVersion::Client(version);

        let (mut start, mut edits) = match self.change.take() {
            Some(DocumentChange::Incremental { start, edits }) => (start, edits),
            _ => (Default::default(), Default::default()),
        };

        // Look for the last full change (one without a range) and start there
        let changes = match changes.iter().rposition(|change| change.range.is_none()) {
            Some(idx) => {
                start = Some(Arc::new(mem::take(&mut changes[idx].text)));
                &mut changes[idx + 1..]
            }
            None => &mut changes[..],
        };

        edits.extend(changes.iter_mut().map(|e| {
            let range = e.range.expect("edit should be after the last full change");
            SourceEdit::new(
                SourcePosition::new(range.start.line, range.start.character)
                    ..SourcePosition::new(range.end.line, range.end.character),
                SourcePositionEncoding::UTF16,
                mem::take(&mut e.text),
            )
        }));

        self.change = Some(DocumentChange::Incremental { start, edits });
    }

    /// Gets a cached analysis result from the document.
    ///
    /// Returns `None` if the document hasn't been analyzed yet or if the cached
    /// result is out of date.
    pub fn cached(&self) -> Option<CachedResult> {
        if let Some(cached) = &self.cached {
            if cached.version == self.version {
                return Some(cached.clone());
            }
        }

        None
    }

    /// Takes the change from the document.
    pub fn take_change(&mut self) -> Option<DocumentChange> {
        self.change.take()
    }

    /// Handles when the document is created.
    pub fn created(&mut self) {
        self.cached = None;

        match self.version {
            DocumentVersion::Client(_) => {
                // Ignore changes to files managed by the client
            }
            DocumentVersion::Server(_) => {
                self.change = Some(DocumentChange::Refetch);
            }
        }
    }

    /// Handles when the document changes.
    pub fn changed(&mut self) {
        self.cached = None;

        match self.version {
            DocumentVersion::Client(_) => {
                // Ignore changes to files managed by the client
            }
            DocumentVersion::Server(version) => {
                self.version = DocumentVersion::Server(version + 1);
                self.change = Some(DocumentChange::Refetch);
            }
        }
    }
}

/// LSP features supported by the client.
#[derive(Clone, Copy, Debug, Default)]
pub struct ClientSupport {
    /// Whether or not the client supports dynamic registration of watched
    /// files.
    pub watched_files: bool,
    /// Whether or not the client supports pull diagnostics (workspace and text
    /// document).
    pub pull_diagnostics: bool,
    /// Whether or not the client supports registering work done progress
    /// tokens.
    pub work_done_progress: bool,
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
        }
    }
}

/// Represents a workspace being managed by the LSP server.
#[derive(Default, Debug)]
pub struct Workspace {
    /// The features supported by the LSP client.
    pub client_support: ClientSupport,
    /// The current set of workspace folders.
    pub folders: Vec<WorkspaceFolder>,
    /// The documents in the workspace.
    pub documents: HashMap<Arc<Url>, Document>,
}

impl Workspace {
    /// Handles a document open event.
    pub fn on_document_open(&mut self, params: DidOpenTextDocumentParams) {
        self.documents.insert(
            Arc::new(params.text_document.uri),
            Document {
                version: DocumentVersion::Client(params.text_document.version),
                cached: None,
                change: Some(DocumentChange::Incremental {
                    start: Some(Arc::new(params.text_document.text)),
                    edits: Vec::new(),
                }),
            },
        );
    }

    /// Handles a document change event.
    pub fn on_document_change(&mut self, params: DidChangeTextDocumentParams) {
        if let Some(document) = self.documents.get_mut(&params.text_document.uri) {
            log::debug!(
                "document `{uri}` is now client version {version}",
                uri = params.text_document.uri,
                version = params.text_document.version
            );
            document.apply_changes(params.text_document.version, params.content_changes);
        }
    }

    /// Handles a document close event.
    pub fn on_document_close(&mut self, params: DidCloseTextDocumentParams) {
        if let Some(document) = self.documents.get_mut(&params.text_document.uri) {
            document.version = DocumentVersion::Server(1);
            document.cached = None;
            document.change = Some(DocumentChange::Refetch);
        }
    }

    /// Handles the results of a document diagnostic request.
    pub fn on_document_diagnostics_results(
        &mut self,
        source: &str,
        params: DocumentDiagnosticParams,
        results: Vec<AnalysisResult>,
    ) -> RpcResult<DocumentDiagnosticReportResult> {
        let mut report = None;
        let mut related_documents = HashMap::new();
        for result in results {
            // Only store local file results
            if result.uri().scheme() != "file" {
                continue;
            }

            let uri = result.uri().as_ref().clone();
            let requested = uri == params.text_document.uri;

            let items = result
                .diagnostics()
                .iter()
                .map(|d| {
                    proto::diagnostic(
                        result.uri(),
                        result
                            .parse_result()
                            .lines()
                            .expect("should have line index"),
                        source,
                        d,
                    )
                })
                .collect::<Result<Vec<_>>>()
                .map_err(|_| RpcError::internal_error())?;

            // Update the cached result in the document
            let document = self.documents.entry(result.uri().clone()).or_default();
            let cached = CachedResult::new(document.version, result);
            let result_id = cached.id.clone();
            document.cached = Some(cached);

            // Check if this is the requested result
            if requested {
                assert!(report.is_none(), "multiple results for the request file");
                report = Some(FullDocumentDiagnosticReport {
                    result_id: Some(result_id),
                    items,
                });
            } else {
                related_documents.insert(
                    uri,
                    DocumentDiagnosticReportKind::Full(FullDocumentDiagnosticReport {
                        result_id: Some(result_id),
                        items,
                    }),
                );
            }
        }

        match report {
            Some(report) => Ok(DocumentDiagnosticReportResult::Report(
                DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                    related_documents: if related_documents.is_empty() {
                        None
                    } else {
                        Some(related_documents)
                    },
                    full_document_diagnostic_report: report,
                }),
            )),
            None => Err(RpcError::request_cancelled()),
        }
    }

    /// Handles the results of a workspace diagnostic request.
    pub fn on_workspace_diagnostics_results(
        &mut self,
        source: &str,
        results: Vec<AnalysisResult>,
        items: &mut Vec<WorkspaceDocumentDiagnosticReport>,
    ) {
        for result in results {
            // Only store local file results
            if result.uri().scheme() != "file" {
                continue;
            }

            // Update the document in the workspace
            let document = self.documents.entry(result.uri().clone()).or_default();
            let cached = CachedResult::new(document.version, result.clone());
            let result_id = cached.id.clone();
            document.cached = Some(cached);

            let diagnostics = result
                .diagnostics()
                .iter()
                .filter_map(|d| {
                    proto::diagnostic(
                        result.uri(),
                        result
                            .parse_result()
                            .lines()
                            .expect("should have line index"),
                        source,
                        d,
                    )
                    .ok()
                })
                .collect();

            items.push(WorkspaceDocumentDiagnosticReport::Full(
                WorkspaceFullDocumentDiagnosticReport {
                    uri: result.uri().as_ref().clone(),
                    version: document.version.client().map(|v| v as i64),
                    full_document_diagnostic_report: FullDocumentDiagnosticReport {
                        result_id: Some(result_id),
                        items: diagnostics,
                    },
                },
            ))
        }
    }

    /// Handles changes to workspace folders.
    pub fn on_workspace_folders_changed(
        &mut self,
        event: WorkspaceFoldersChangeEvent,
        documents: Vec<Arc<Url>>,
    ) {
        // Process the removed folders
        if !event.removed.is_empty() {
            self.documents.retain(|uri, _| {
                !event
                    .removed
                    .iter()
                    .any(|f| uri.as_str().starts_with(f.uri.as_str()))
            });

            self.folders
                .retain(|f: &WorkspaceFolder| !event.removed.contains(f));
        }

        self.folders.extend(event.added);

        // Process the newly discovered documents
        for uri in documents {
            if let Some(document) = self.documents.get_mut(&uri) {
                document.changed();
            } else {
                self.documents.insert(uri, Document::default());
            }
        }
    }

    /// Handles changes to watched files.
    pub fn on_watched_files_changed(&mut self, events: &Vec<FileEvent>) {
        /// Checks if the given URI is to a WDL file
        fn is_wdl_file(uri: &Url) -> bool {
            if let Ok(path) = uri.to_file_path() {
                if path.is_file() && path.extension().and_then(OsStr::to_str) == Some("wdl") {
                    return true;
                }
            }

            false
        }

        let mut clear_cache = false;
        for event in events {
            match event.typ {
                FileChangeType::CREATED => {
                    if is_wdl_file(&event.uri) {
                        log::debug!("document `{uri}` has been created", uri = event.uri);
                        self.documents
                            .entry(Arc::new(event.uri.clone()))
                            .or_default()
                            .created();
                    }
                }
                FileChangeType::CHANGED => {
                    if is_wdl_file(&event.uri) {
                        log::debug!("document `{uri}` has been changed", uri = event.uri);
                        self.documents
                            .entry(Arc::new(event.uri.clone()))
                            .or_default()
                            .changed();
                    }
                }
                FileChangeType::DELETED => {
                    let base = match event.uri.to_file_path() {
                        Ok(base) => base,
                        Err(_) => continue,
                    };

                    // What was deleted might be a directory, so do a prefix match
                    self.documents.retain(|uri, _| {
                        let path = match uri.to_file_path() {
                            Ok(path) => path,
                            Err(_) => return true,
                        };

                        if path.starts_with(&base) {
                            log::debug!("document `{uri}` has been removed from the workspace");
                            clear_cache = true;
                            false
                        } else {
                            true
                        }
                    });
                }
                _ => {}
            }
        }

        if clear_cache {
            // As we don't know which documents may be impacted, clear the entire cache
            for document in self.documents.values_mut() {
                document.cached = None;
            }
        }
    }
}
