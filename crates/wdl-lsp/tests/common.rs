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
use tracing_subscriber::FmtSubscriber;
use url::Url;
use wdl_lsp::FilterReloadHandle;
use wdl_lsp::Server;
use wdl_lsp::ServerOptions;
use wdl_lsp::UserOptions;

/// Gets a test workspace directory path
pub fn get_workspace_path(name: &str) -> PathBuf {
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
pub struct DummyClient;

impl LanguageClient for DummyClient {
    type Error = ResponseError;
    type NotifyResult = ControlFlow<async_lsp::Result<()>>;
}

/// Represents the context for a single integration test.
///
/// This sets up a temporary workspace, starts a server instance, and provides
/// methods for simulating a client interacting with the server.
#[derive(Debug)]
#[allow(unused)]
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
/// Name of the test workspace folder.
pub const WORKSPACE_FOLDER_NAME: &str = "wdl-lsp-workspace";

/// Builder for [`TestContext`]s
#[derive(Debug)]
pub struct TestContextBuilder<C> {
    workspace: PathBuf,
    client: C,
    log_handle: Option<FilterReloadHandle<FmtSubscriber>>,
    server_options: ServerOptions,
    user_options: UserOptions,
}

impl TestContextBuilder<DummyClient> {
    /// Create a new `TestContextBuilder` for the given workspace.
    pub fn new(base: &str) -> Self {
        Self {
            workspace: get_workspace_path(base),
            client: DummyClient,
            log_handle: None,
            server_options: Default::default(),
            user_options: Default::default(),
        }
    }
}

#[allow(unused)]
impl<C> TestContextBuilder<C>
where
    C: LanguageClient<NotifyResult = ControlFlow<async_lsp::Result<()>>, Error = ResponseError>
        + Send
        + 'static,
{
    /// Set the LSP client implementation.
    ///
    /// By default, the context uses a [`DummyClient`].
    pub fn client<C2>(self, client: C2) -> TestContextBuilder<C2>
    where
        C2: LanguageClient<NotifyResult = ControlFlow<async_lsp::Result<()>>, Error = ResponseError>
            + Send
            + 'static,
    {
        TestContextBuilder {
            workspace: self.workspace,
            client,
            log_handle: self.log_handle,
            server_options: self.server_options,
            user_options: self.user_options,
        }
    }

    /// Set the log handle.
    pub fn log_handle(mut self, handle: FilterReloadHandle<FmtSubscriber>) -> Self {
        self.log_handle = Some(handle);
        self
    }

    /// Set the [`ServerOptions`].
    pub fn server_options(mut self, options: ServerOptions) -> Self {
        self.server_options = options;
        self
    }

    /// Set the [`UserOptions`].
    pub fn user_options(mut self, options: UserOptions) -> Self {
        self.user_options = options;
        self
    }

    /// Spawn the client and server tasks and create a new [`TestContext`].
    pub fn build(self) -> TestContext {
        self.build_with_options_fn(|_, _, _| {})
    }

    /// Same as [`Self::build()`], but allows the options to be modified with
    /// the newly created workspace directory.
    pub fn build_with_options_fn(
        mut self,
        make_options: impl FnOnce(&Path, &mut ServerOptions, &mut UserOptions),
    ) -> TestContext {
        let workspace = TempDir::new().unwrap();
        if self.workspace.exists() {
            let items: Vec<_> = fs::read_dir(&self.workspace)
                .unwrap()
                .map(|e| e.unwrap().path())
                .collect();
            let copy_options = CopyOptions::new().overwrite(true);
            copy_items(&items, workspace.path(), &copy_options).unwrap();
        }

        make_options(
            workspace.path(),
            &mut self.server_options,
            &mut self.user_options,
        );
        let (server_loop, client_socket) = MainLoop::new_server(|client| {
            ServiceBuilder::new().service(Router::from_language_server(
                Server::<FmtSubscriber>::new(
                    client,
                    self.server_options,
                    self.user_options,
                    self.log_handle,
                ),
            ))
        });

        let (client_loop, server_socket) = MainLoop::new_client(|_server| {
            ServiceBuilder::new().service(Router::from_language_client(self.client))
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

        TestContext {
            _server_handle: server_handle,
            _client_handle: client_handle,
            client: client_socket,
            server: server_socket,
            workspace,
        }
    }
}

impl TestContext {
    /// Creates a file URI for a path within the temporary workspace.
    pub fn doc_uri(&self, path: &str) -> Url {
        Url::from_file_path(self.doc_path(path)).unwrap()
    }

    /// Gets the path to a file within the temporary workspace.
    pub fn doc_path(&self, path: &str) -> PathBuf {
        self.workspace.path().join(path)
    }

    /// Creates a file URI for the temporary workspace.
    pub fn workspace_uri(&self) -> Url {
        Url::from_file_path(self.workspace.path()).unwrap()
    }

    /// Performs the LSP initialization handshake and returns the initial
    /// workspace diagnostic report alongside the initialization result.
    pub async fn initialize(
        &mut self,
    ) -> (lsp_types::InitializeResult, WorkspaceDiagnosticReportResult) {
        let workspace_url = self.workspace_uri();
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
                name: WORKSPACE_FOLDER_NAME.to_owned(),
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
