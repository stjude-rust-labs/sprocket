//! Integration tests for the `textDocument/didOpen` and `textDocument/didClose`
//! notifications.

use async_lsp::lsp_types::DidCloseTextDocumentParams;
use async_lsp::lsp_types::DidOpenTextDocumentParams;
use async_lsp::lsp_types::DocumentDiagnosticParams;
use async_lsp::lsp_types::DocumentDiagnosticReport;
use async_lsp::lsp_types::DocumentDiagnosticReportResult;
use async_lsp::lsp_types::DocumentSymbolParams;
use async_lsp::lsp_types::TextDocumentIdentifier;
use async_lsp::lsp_types::TextDocumentItem;
use async_lsp::lsp_types::notification::DidCloseTextDocument;
use async_lsp::lsp_types::notification::DidOpenTextDocument;
use async_lsp::lsp_types::request::DocumentDiagnosticRequest;
use async_lsp::lsp_types::request::DocumentSymbolRequest;
use tempfile::TempDir;
use url::Url;

use crate::common::TestContext;
use crate::common::TestContextBuilder;

async fn document_in_graph(ctx: &mut TestContext, document: Url) -> bool {
    ctx.request::<DocumentSymbolRequest>(DocumentSymbolParams {
        text_document: TextDocumentIdentifier { uri: document },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    })
    .await
    .expect("request should succeed")
    .is_some()
}

/// Sets up a test workspace with external WDL and YAML files
async fn setup_workspace() -> (TestContext, TempDir, TextDocumentItem, TextDocumentItem) {
    let mut ctx = TestContextBuilder::new("diagnostics").build();
    ctx.initialize().await;

    // The external dir is necessary since the server roots the entire workspace
    // directory. Test YAMLs can only manage WDL file lifecycles if they
    // don't live in the workspace, otherwise the server keeps them alive
    // until the workspace is fully changed.
    let external_dir = tempfile::tempdir().unwrap();

    let external_wdl = external_dir.path().join("external.wdl");
    let external_yaml = external_dir.path().join("external.yaml");

    std::fs::copy(ctx.doc_path("source.wdl"), &external_wdl).unwrap();
    std::fs::copy(ctx.doc_path("source.yaml"), &external_yaml).unwrap();

    let wdl_doc = TextDocumentItem {
        uri: Url::from_file_path(&external_wdl).unwrap(),
        language_id: String::from("wdl"),
        version: 0,
        text: std::fs::read_to_string(&external_wdl).unwrap(),
    };
    let yaml_doc = TextDocumentItem {
        uri: Url::from_file_path(&external_yaml).unwrap(),
        language_id: String::from("yaml"),
        version: 0,
        text: std::fs::read_to_string(&external_yaml).unwrap(),
    };
    (ctx, external_dir, wdl_doc, yaml_doc)
}

#[tokio::test]
async fn test_yaml_manages_wdl_lifecycle() {
    let (mut ctx, _external_dir, wdl_doc, yaml_doc) = setup_workspace().await;

    assert!(!document_in_graph(&mut ctx, wdl_doc.uri.clone()).await);

    // This should implicitly open `external.wdl`
    ctx.server
        .notify::<DidOpenTextDocument>(DidOpenTextDocumentParams {
            text_document: yaml_doc.clone(),
        })
        .unwrap();
    assert!(document_in_graph(&mut ctx, wdl_doc.uri.clone()).await);

    // Closing `external.yaml` should also close `external.wdl`
    ctx.server
        .notify::<DidCloseTextDocument>(DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier {
                uri: yaml_doc.uri.clone(),
            },
        })
        .unwrap();
    assert!(!document_in_graph(&mut ctx, wdl_doc.uri.clone()).await);

    // If a test depends on a WDL file being open, it should remain open even if
    // the client closes it
    ctx.server
        .notify::<DidOpenTextDocument>(DidOpenTextDocumentParams {
            text_document: wdl_doc.clone(),
        })
        .unwrap();
    assert!(document_in_graph(&mut ctx, wdl_doc.uri.clone()).await);

    ctx.server
        .notify::<DidOpenTextDocument>(DidOpenTextDocumentParams {
            text_document: yaml_doc.clone(),
        })
        .unwrap();

    ctx.server
        .notify::<DidCloseTextDocument>(DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier {
                uri: wdl_doc.uri.clone(),
            },
        })
        .unwrap();
    assert!(document_in_graph(&mut ctx, wdl_doc.uri.clone()).await);

    // Diagnostics on the test YAML should still work, even with the WDL closed
    let report = ctx
        .request::<DocumentDiagnosticRequest>(DocumentDiagnosticParams {
            text_document: TextDocumentIdentifier {
                uri: yaml_doc.uri.clone(),
            },
            identifier: None,
            previous_result_id: None,
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
        .expect("diagnostic request should succeed");

    let DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(report)) = report
    else {
        panic!("expected full diagnostic report");
    };
    assert!(
        !report.full_document_diagnostic_report.items.is_empty(),
        "test YAML diagnostics should still work"
    );

    // Closing the YAML leaves the WDL with no dependents, so it can finally be
    // closed
    ctx.server
        .notify::<DidCloseTextDocument>(DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier {
                uri: yaml_doc.uri.clone(),
            },
        })
        .unwrap();
    assert!(!document_in_graph(&mut ctx, wdl_doc.uri.clone()).await);
}

#[tokio::test]
async fn retain_opened_wdl_on_test_close() {
    // When a test YAML is opened before its associated WDL file, the WDL is
    // opened and managed by the test cache until the client explicitly
    // opens it.

    let (mut ctx, _external_dir, wdl_doc, yaml_doc) = setup_workspace().await;

    // This implicitly opens `external.wdl` and binds it to the test YAML
    ctx.server
        .notify::<DidOpenTextDocument>(DidOpenTextDocumentParams {
            text_document: yaml_doc.clone(),
        })
        .unwrap();

    // The client explicitly opening `external.wdl` means the YAML is no longer
    // responsible for its lifecycle
    ctx.server
        .notify::<DidOpenTextDocument>(DidOpenTextDocumentParams {
            text_document: wdl_doc.clone(),
        })
        .unwrap();

    ctx.server
        .notify::<DidCloseTextDocument>(DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier {
                uri: yaml_doc.uri.clone(),
            },
        })
        .unwrap();

    assert!(document_in_graph(&mut ctx, wdl_doc.uri.clone()).await);

    ctx.server
        .notify::<DidCloseTextDocument>(DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier {
                uri: wdl_doc.uri.clone(),
            },
        })
        .unwrap();
    assert!(!document_in_graph(&mut ctx, wdl_doc.uri.clone()).await);
}

#[tokio::test]
async fn workspace_wdl_not_unrooted_on_close() {
    let (mut ctx, ..) = setup_workspace().await;

    let workspace_wdl = ctx.doc_uri("source.wdl");
    assert!(document_in_graph(&mut ctx, workspace_wdl.clone()).await);

    // Closing a workspace WDL must not unroot it from the analyzer graph
    ctx.server
        .notify::<DidCloseTextDocument>(DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier {
                uri: workspace_wdl.clone(),
            },
        })
        .unwrap();

    assert!(document_in_graph(&mut ctx, workspace_wdl.clone()).await);
}
