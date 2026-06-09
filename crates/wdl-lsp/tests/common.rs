//! Common test suite for WDL LSP integration tests.
//!
//! This module implements a lightweight communication protocol for testing the
//! WDL Language Server Protocol (LSP) implementation. It uses JSON-RPC
//! message format and Content-Length headers (similar to LSP over stdio).
//!
//! The protocol works by:
//! - Encoding JSON-RPC messages with a simple format
//!
//!  `Content-Length: <size>\r\n\r\n<payload>`
//!
//! - Using in-memory streams to simulate client-server communication

use std::fmt::Debug;
use std::fs;
use std::ops::ControlFlow;
use std::ops::Deref;
use std::path::Path;
use std::path::PathBuf;

use async_lsp::ClientSocket;
use async_lsp::LanguageClient;
use async_lsp::MainLoop;
use async_lsp::ResponseError;
use async_lsp::ServerSocket;
use async_lsp::lsp_types;
use async_lsp::lsp_types::ClientCapabilities;
use async_lsp::lsp_types::InitializeParams;
use async_lsp::lsp_types::InitializedParams;
use async_lsp::lsp_types::WorkspaceDiagnosticParams;
use async_lsp::lsp_types::WorkspaceDiagnosticReportResult;
use async_lsp::lsp_types::WorkspaceFolder;
use async_lsp::lsp_types::request::WorkspaceDiagnosticRequest;
use async_lsp::router::Router;
use fs_extra::copy_items;
use fs_extra::dir::CopyOptions;
use futures::AsyncReadExt;
use tempfile::TempDir;
use tokio::io::duplex;
use tokio_util::compat::TokioAsyncReadCompatExt;
use tower::ServiceBuilder;
use url::Url;
use wdl_lsp::LintOptions;
use wdl_lsp::Server;
use wdl_lsp::ServerOptions;
use wdl_lsp::UserOptions;

/// Gets a test workspace directory path
fn get_workspace_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("workspace")
        .join(name)
}

/// Fake client implementation.
///
/// This client doesn't do anything, it's only used to spawn a client loop
/// to get a handle to the server.
#[derive(Debug)]
struct DummyClient;

impl LanguageClient for DummyClient {
    type Error = ResponseError;
    type NotifyResult = ControlFlow<async_lsp::Result<()>>;
}

/// Represents the context for a single integration test.
///
/// This sets up a temporary workspace, starts a server instance, and provides
/// methods for simulating a client interacting with the server.
#[derive(Debug)]
pub struct TestContext {
    /// The join handle for the running server task.
    pub _server_handle: tokio::task::JoinHandle<()>,
    /// The join handle for the running client task.
    pub _client_handle: tokio::task::JoinHandle<()>,
    /// Handle to communicate with the client.
    pub client: ClientSocket,
    /// Handle to communicate with the server.
    pub server: ServerSocket,
    /// The temporary directory representing the workspace root.
    pub workspace: TempDir,
}

const MAX_BUF_SIZE: usize = 4096;

impl TestContext {
    /// Creates a new test context.
    ///
    /// The `base` parameter is the name of a subdirectory in `tests/workspace`
    /// which contains the WDL files for the test. These files are copied
    /// into a temporary workspace directory.
    pub fn new(base: &str) -> Self {
        Self::with_options(
            base,
            ServerOptions::default(),
            UserOptions {
                lint: LintOptions {
                    enabled: true,
                    ..Default::default()
                },
                ..Default::default()
            },
        )
    }

    /// Creates a new test context with custom server options.
    pub fn with_options(
        base: &str,
        server_options: ServerOptions,
        user_options: UserOptions,
    ) -> Self {
        Self::with_options_fn(base, |_| (server_options, user_options))
    }

    /// Creates a new test context with server options computed from the
    /// temporary workspace's filesystem path.
    ///
    /// This is useful for tests that need to configure the server with
    /// options whose value depends on the workspace location (e.g., a
    /// `Baseline` whose `base_dir` must point at the workspace root).
    pub fn with_options_fn(
        base: &str,
        make_options: impl FnOnce(&Path) -> (ServerOptions, UserOptions),
    ) -> Self {
        let workspace = TempDir::new().unwrap();
        let workspace_path = get_workspace_path(base);
        if workspace_path.exists() {
            let items: Vec<_> = fs::read_dir(&workspace_path)
                .unwrap()
                .map(|e| e.unwrap().path())
                .collect();
            let copy_options = CopyOptions::new().overwrite(true);
            copy_items(&items, workspace.path(), &copy_options).unwrap();
        }

        let (server_options, user_options) = make_options(workspace.path());
        let (server_loop, client_socket) = MainLoop::new_server(|client| {
            ServiceBuilder::new().service(Router::from_language_server(Server::<()>::new(
                client,
                server_options,
                user_options,
                None,
            )))
        });

        let (client_loop, server_socket) = MainLoop::new_client(|_server| {
            ServiceBuilder::new().service(Router::from_language_client(DummyClient))
        });

        // Wire up a loopback channel between the server and the client.
        let (server_stream, client_stream) = duplex(MAX_BUF_SIZE);
        let (server_rx, server_tx) = server_stream.compat().split();
        let server_handle = tokio::spawn(async move {
            server_loop
                .run_buffered(server_rx, server_tx)
                .await
                .unwrap();
        });

        let (client_rx, client_tx) = client_stream.compat().split();
        let client_handle = tokio::spawn(async move {
            let err = client_loop
                .run_buffered(client_rx, client_tx)
                .await
                .unwrap_err();
            assert!(
                matches!(err, async_lsp::Error::Eof),
                "should fail due to EOF: {err}"
            );
        });

        Self {
            _server_handle: server_handle,
            _client_handle: client_handle,
            client: client_socket,
            server: server_socket,
            workspace,
        }
    }

    /// Creates a file URI for a path within the temporary workspace.
    pub fn doc_uri(&self, path: &str) -> Url {
        Url::from_file_path(self.workspace.path().join(path)).unwrap()
    }

    /// Performs the LSP initialization handshake and returns the initial
    /// workspace diagnostic report alongside the initialization result.
    pub async fn initialize(
        &mut self,
    ) -> (lsp_types::InitializeResult, WorkspaceDiagnosticReportResult) {
        let workspace_url = Url::from_file_path(self.workspace.path()).unwrap();
        let capabilities = ClientCapabilities {
            text_document: Some(lsp_types::TextDocumentClientCapabilities {
                synchronization: Some(lsp_types::TextDocumentSyncClientCapabilities {
                    dynamic_registration: Some(true),
                    ..Default::default()
                }),
                diagnostic: Some(lsp_types::DiagnosticClientCapabilities {
                    dynamic_registration: Some(false),
                    ..Default::default()
                }),
                document_symbol: Some(lsp_types::DocumentSymbolClientCapabilities {
                    dynamic_registration: Some(false),
                    ..Default::default()
                }),
                definition: Some(Default::default()),
                references: Some(Default::default()),
                ..Default::default()
            }),
            workspace: Some(lsp_types::WorkspaceClientCapabilities {
                workspace_folders: Some(true),
                ..Default::default()
            }),
            window: Some(lsp_types::WindowClientCapabilities {
                work_done_progress: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };

        let params = InitializeParams {
            process_id: None,
            initialization_options: None,
            capabilities,
            trace: None,
            workspace_folders: Some(vec![WorkspaceFolder {
                name: "wdl-lsp-workspace".to_owned(),
                uri: workspace_url,
            }]),
            client_info: None,
            locale: None,
            ..Default::default()
        };

        let result = self
            .request::<lsp_types::request::Initialize>(params)
            .await
            .expect("request should succeed");
        self.notify::<lsp_types::notification::Initialized>(InitializedParams {})
            .expect("notification should succeed");

        let diagnostics = self.workspace_diagnostic().await;
        (result, diagnostics)
    }

    /// Issues a fresh `workspace/diagnostic` pull with no previous result IDs.
    pub async fn workspace_diagnostic(&mut self) -> WorkspaceDiagnosticReportResult {
        self.request::<WorkspaceDiagnosticRequest>(WorkspaceDiagnosticParams {
            identifier: None,
            previous_result_ids: Vec::new(),
            partial_result_params: Default::default(),
            work_done_progress_params: Default::default(),
        })
        .await
        .expect("request should succeed")
    }
}

// So we can use the `request`/`notify` methods directly on `TestContext`.
impl Deref for TestContext {
    type Target = ServerSocket;

    fn deref(&self) -> &Self::Target {
        &self.server
    }
}
