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
use std::path::Path;
use std::path::PathBuf;

use fs_extra::copy_items;
use fs_extra::dir::CopyOptions;
use tempfile::TempDir;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::io::DuplexStream;
use tokio::io::duplex;
use tower_lsp::LspService;
use tower_lsp::jsonrpc;
use tower_lsp::lsp_types;
use tower_lsp::lsp_types::ClientCapabilities;
use tower_lsp::lsp_types::InitializeParams;
use tower_lsp::lsp_types::InitializedParams;
use tower_lsp::lsp_types::WorkspaceDiagnosticParams;
use tower_lsp::lsp_types::WorkspaceFolder;
use tower_lsp::lsp_types::notification::Notification;
use tower_lsp::lsp_types::request::Request;
use tower_lsp::lsp_types::request::WorkspaceDiagnosticRequest;
use url::Url;
use wdl_lsp::Server;
use wdl_lsp::ServerOptions;

/// Encodes a JSON-RPC message with the required `Content-Length` header.
fn encode_message(message: &str) -> String {
    format!("Content-Length: {}\r\n\r\n{}", message.len(), message)
}

/// Gets a test workspace directory path
fn get_workspace_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("workspace")
        .join(name)
}

/// Represents the context for a single integration test.
///
/// This sets up a temporary workspace, starts a server instance, and provides
/// methods for simulating a client interacting with the server.
#[derive(Debug)]
pub struct TestContext {
    /// The stream for sending requests to the server.
    pub request_tx: DuplexStream,
    /// The stream for receiving responses from the server.
    pub response_rx: BufReader<DuplexStream>,
    /// The join handle for the running server task.
    pub _server: tokio::task::JoinHandle<()>,
    /// The counter for generating unique request IDs.
    pub request_id: i64,
    /// The temporary directory representing the workspace root.
    pub workspace: TempDir,
}

const MAX_BUF_SIZE: usize = 4096;

impl TestContext {
    /// Crates a new test context.
    ///
    /// The `base` parameter is the name of a subdirectory in `tests/workspace`
    /// which contains the WDL files for the test. These files are copied
    /// into a temporary workspace directory.
    pub fn new(base: &str) -> Self {
        let (request_tx, req_server) = duplex(MAX_BUF_SIZE);
        let (resp_server, response_rx) = duplex(MAX_BUF_SIZE);
        let response_rx = BufReader::new(response_rx);

        let (service, socket) = LspService::new(|client| {
            Server::new(
                client,
                ServerOptions {
                    lint: true,
                    ..Default::default()
                },
            )
        });
        let server =
            tokio::spawn(tower_lsp::Server::new(req_server, resp_server, socket).serve(service));

        let workspace = TempDir::new().unwrap();
        let workspace_path = get_workspace_path(base);
        if workspace_path.exists() {
            let items: Vec<_> = fs::read_dir(&workspace_path)
                .unwrap()
                .map(|e| e.unwrap().path())
                .collect();
            let options = CopyOptions::new().overwrite(true);
            copy_items(&items, workspace.path(), &options).unwrap();
        }

        Self {
            request_tx,
            response_rx,
            _server: server,
            request_id: 0,
            workspace,
        }
    }

    /// Creates a file URI for a path within the temporary workspace.
    pub fn doc_uri(&self, path: &str) -> Url {
        Url::from_file_path(self.workspace.path().join(path)).unwrap()
    }

    /// Sends a raw JSON-RPC request to the server.
    pub async fn send_raw(&mut self, message: &str) {
        self.request_tx
            .write_all(encode_message(message).as_bytes())
            .await
            .unwrap();
    }

    /// Sends a typed JSON-RPC request to the server.
    pub async fn send(&mut self, request: &jsonrpc::Request) {
        let content = serde_json::to_string(request).unwrap();
        self.send_raw(&content).await;
    }

    /// Reads a raw JSON-RPC message string from the server response stream.
    pub async fn read_message_str(&mut self) -> Option<String> {
        let mut content_length = 0;

        loop {
            let mut header = String::new();
            if self.response_rx.read_line(&mut header).await.unwrap() == 0 {
                return None; // Connection closed
            }
            if header.trim().is_empty() {
                break; // End of headers
            }
            let parts: Vec<&str> = header.trim().splitn(2, ": ").collect();
            if parts.len() == 2 && parts[0].eq_ignore_ascii_case("Content-Length") {
                content_length = parts[1].parse().unwrap();
            }
        }

        if content_length > 0 {
            let mut content = vec![0; content_length];
            self.response_rx.read_exact(&mut content).await.unwrap();
            Some(String::from_utf8(content).unwrap())
        } else {
            Some(String::new())
        }
    }

    /// Receives and deserializes the next JSON-RPC response from the server.
    ///
    /// This method waits for a response with the `expected_id`, filtering out:
    ///
    /// - Server-initiated notifications (which don't need responses)
    /// - Server-initiated requests that require client responses
    ///
    /// The server sends `window/workDoneProgress/create` requests during
    /// long-running operations. Per the LSP spec, clients must respond to these
    /// requests to acknowledge progress token creation. We automatically
    /// respond with `null` to keep the server's progress reporting
    /// functional without blocking tests.
    pub async fn response<R>(&mut self, expected_id: jsonrpc::Id) -> R
    where
        R: Debug + serde::de::DeserializeOwned,
    {
        loop {
            let content_str = self
                .read_message_str()
                .await
                .expect("server closed connection");

            if let Ok(response) = serde_json::from_str::<jsonrpc::Response>(&content_str) {
                let (id, result) = response.into_parts();
                if id == expected_id {
                    return serde_json::from_value(result.unwrap()).unwrap();
                } else {
                    continue;
                }
            }

            if let Ok(request) = serde_json::from_str::<jsonrpc::Request>(&content_str) {
                let (method, id_opt, _params) = request.into_parts();
                if method == "window/workDoneProgress/create"
                    && let Some(id) = id_opt
                {
                    let response = jsonrpc::Response::from_ok(id, serde_json::Value::Null);
                    let response_str = serde_json::to_string(&response).unwrap();
                    self.send_raw(&response_str).await;
                }
                continue;
            }
            // skip notifications from server.
        }
    }

    /// Sends a typed LSP request and awaits a typed response.
    pub async fn request<R: Request>(&mut self, params: R::Params) -> R::Result
    where
        R::Result: Debug,
    {
        let request_id = jsonrpc::Id::Number(self.request_id);
        let request = jsonrpc::Request::build(R::METHOD)
            .id(self.request_id)
            .params(serde_json::to_value(params).unwrap())
            .finish();
        self.request_id += 1;
        self.send(&request).await;
        self.response(request_id).await
    }

    /// Sends a typed LSP notification to the server.
    pub async fn notify<N: Notification>(&mut self, params: N::Params) {
        let notification = jsonrpc::Request::build(N::METHOD)
            .params(serde_json::to_value(params).unwrap())
            .finish();
        self.send(&notification).await;
    }

    /// Performs the LSP initialization handshake.
    pub async fn initialize(&mut self) -> lsp_types::InitializeResult {
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
            process_id: Some(1234),
            root_uri: Some(workspace_url.clone()),
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

        let result = self.request::<lsp_types::request::Initialize>(params).await;
        self.notify::<lsp_types::notification::Initialized>(InitializedParams {})
            .await;

        // After initialization, we immediately ask for a full workspace diagnostic.
        // This forces the server to do its initial analysis of all files found
        // in the workspace folders provided during `initialize`.
        self.request::<WorkspaceDiagnosticRequest>(WorkspaceDiagnosticParams {
            identifier: None,
            previous_result_ids: Vec::new(),
            partial_result_params: Default::default(),
            work_done_progress_params: Default::default(),
        })
        .await;

        result
    }
}
