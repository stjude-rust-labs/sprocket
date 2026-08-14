//! Integration tests for the `workspace/didChangeConfiguration` notification.

use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;

use async_lsp::LanguageClient;
use async_lsp::ResponseError;
use async_lsp::lsp_types::ConfigurationParams;
use async_lsp::lsp_types::DidChangeConfigurationParams;
use async_lsp::lsp_types::DidOpenTextDocumentParams;
use async_lsp::lsp_types::DocumentDiagnosticParams;
use async_lsp::lsp_types::DocumentDiagnosticReport;
use async_lsp::lsp_types::DocumentDiagnosticReportResult;
use async_lsp::lsp_types::NumberOrString;
use async_lsp::lsp_types::TextDocumentIdentifier;
use async_lsp::lsp_types::TextDocumentItem;
use async_lsp::lsp_types::WorkspaceDiagnosticReportResult;
use async_lsp::lsp_types::WorkspaceDocumentDiagnosticReport;
use async_lsp::lsp_types::notification::DidChangeConfiguration;
use async_lsp::lsp_types::notification::DidOpenTextDocument;
use async_lsp::lsp_types::request::DocumentDiagnosticRequest;
use futures::future::BoxFuture;
use serde_json::Value;
use tokio::sync::Mutex;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::FmtSubscriber;
use wdl_lsp::FilterReloadHandle;
use wdl_lsp::LintOptions;
use wdl_lsp::UserOptions;

use crate::common::TestContextBuilder;

/// Fake client implementation.
///
/// This client doesn't do anything, it's only used to spawn a client loop
/// to get a handle to the server.
#[derive(Debug, Default)]
struct ConfigurableClient {
    config: Arc<Mutex<UserOptions>>,
}

impl LanguageClient for ConfigurableClient {
    type Error = ResponseError;
    type NotifyResult = ControlFlow<async_lsp::Result<()>>;

    fn configuration(
        &mut self,
        params: ConfigurationParams,
    ) -> BoxFuture<'static, Result<Vec<Value>, ResponseError>> {
        assert_eq!(params.items.len(), 1);
        assert_eq!(params.items[0].section.as_deref(), Some("sprocket.server"));
        let config = self.config.clone();
        Box::pin(async move {
            let config = config.lock().await;
            let log_level = match config.log_level.0 {
                LevelFilter::OFF => "off",
                LevelFilter::ERROR => "error",
                LevelFilter::WARN => "warn",
                LevelFilter::INFO => "info",
                LevelFilter::DEBUG => "debug",
                LevelFilter::TRACE => "trace",
            };
            Ok(vec![serde_json::json!({
                "logLevel": log_level,
                "lint": {
                    "enabled": config.lint.enabled,
                }
            })])
        })
    }
}

#[tokio::test]
async fn should_respect_lint_settings() {
    let client = ConfigurableClient::default();
    let config = client.config.clone();

    let mut ctx = TestContextBuilder::new("diagnostics")
        .client(client)
        .build();
    let (_, diagnostics) = ctx.initialize().await;

    // Linting is disabled by default, should return nothing
    match diagnostics {
        WorkspaceDiagnosticReportResult::Report(report) => {
            assert_eq!(report.items.len(), 1);
            let WorkspaceDocumentDiagnosticReport::Full(report) = &report.items[0] else {
                panic!("expected full report, got: {report:?}")
            };
            assert!(
                report.full_document_diagnostic_report.items.is_empty(),
                "no diagnostics should be produced"
            );
        }
        WorkspaceDiagnosticReportResult::Partial(_) => {
            unreachable!("should not return partial report")
        }
    }

    let text = std::fs::read_to_string(ctx.doc_path("source.wdl")).unwrap();
    ctx.notify::<DidOpenTextDocument>(DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: ctx.doc_uri("source.wdl"),
            language_id: "wdl".to_string(),
            version: 0,
            text,
        },
    })
    .expect("failed to send didOpenTextDocument notification");

    // Enable linting
    config.lock().await.lint = LintOptions {
        enabled: true,
        config: Arc::default(),
    };

    ctx.notify::<DidChangeConfiguration>(DidChangeConfigurationParams {
        settings: Value::Null,
    })
    .expect("failed to send didChangeConfiguration notification");

    let diagnostic_report = ctx
        .request::<DocumentDiagnosticRequest>(DocumentDiagnosticParams {
            text_document: TextDocumentIdentifier {
                uri: ctx.doc_uri("source.wdl"),
            },
            identifier: None,
            previous_result_id: None,
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
        .expect("failed to get diagnostics");

    match diagnostic_report {
        DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(report)) => {
            let items = report.full_document_diagnostic_report.items;
            assert_eq!(items.len(), 1);
            assert!(
                items.iter().any(|item| {
                    matches!(
                        item.code.as_ref(),
                        Some(NumberOrString::String(code)) if code == "SnakeCase"
                    )
                }),
                "expected a `SnakeCase` diagnostic, got: {items:?}"
            );
        }
        DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Unchanged(_)) => {
            unreachable!("there are no prior diagnostic reports")
        }
        DocumentDiagnosticReportResult::Partial(_) => {
            unreachable!("should not return partial report")
        }
    }
}

#[tokio::test]
async fn should_respect_log_level() {
    async fn wait_for_log_level(handle: FilterReloadHandle<FmtSubscriber>, expected: LevelFilter) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if handle.clone_current().unwrap().to_string() == expected.to_string() {
                    break;
                }

                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("timed out waiting for log level to update");
    }

    let client = ConfigurableClient::default();
    let config = client.config.clone();

    let mut current_level = LevelFilter::ERROR;
    let (_layer, log_handle) =
        tracing_subscriber::reload::Layer::new(EnvFilter::new(current_level.to_string()));
    let mut ctx = TestContextBuilder::new("diagnostics")
        .client(client)
        .log_handle(log_handle.clone())
        .build();
    ctx.initialize().await;
    assert_eq!(
        log_handle.clone_current().unwrap().to_string(),
        current_level.to_string()
    );

    current_level = LevelFilter::TRACE;
    config.lock().await.log_level = current_level.into();
    ctx.notify::<DidChangeConfiguration>(DidChangeConfigurationParams {
        settings: Value::Null,
    })
    .expect("failed to send didChangeConfiguration notification");

    wait_for_log_level(log_handle.clone(), current_level).await;

    current_level = LevelFilter::ERROR;
    config.lock().await.log_level = current_level.into();
    ctx.notify::<DidChangeConfiguration>(DidChangeConfigurationParams {
        settings: Value::Null,
    })
    .expect("failed to send didChangeConfiguration notification");

    wait_for_log_level(log_handle.clone(), current_level).await;
}
