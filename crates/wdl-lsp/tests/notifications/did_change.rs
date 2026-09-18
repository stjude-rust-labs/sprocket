//! Integration tests for the `textDocument/didChange` notification.

use async_lsp::lsp_types::DidChangeTextDocumentParams;
use async_lsp::lsp_types::DidOpenTextDocumentParams;
use async_lsp::lsp_types::DocumentDiagnosticParams;
use async_lsp::lsp_types::DocumentDiagnosticReport;
use async_lsp::lsp_types::DocumentDiagnosticReportResult;
use async_lsp::lsp_types::TextDocumentContentChangeEvent;
use async_lsp::lsp_types::TextDocumentIdentifier;
use async_lsp::lsp_types::VersionedTextDocumentIdentifier;
use async_lsp::lsp_types::notification::DidChangeTextDocument;
use async_lsp::lsp_types::notification::DidOpenTextDocument;
use async_lsp::lsp_types::request::DocumentDiagnosticRequest;

use crate::common::TestContextBuilder;

#[tokio::test]
async fn wdl_change_invalidates_dependent_test_yaml() {
    let mut ctx = TestContextBuilder::new("test").build();
    ctx.initialize().await;

    ctx.server
        .notify::<DidOpenTextDocument>(DidOpenTextDocumentParams {
            text_document: ctx.text_document("source.yaml", "yaml"),
        })
        .unwrap();
    ctx.server
        .notify::<DidOpenTextDocument>(DidOpenTextDocumentParams {
            text_document: ctx.text_document("source.wdl", "wdl"),
        })
        .unwrap();

    // Initial diagnostics pull
    let report = ctx
        .request::<DocumentDiagnosticRequest>(DocumentDiagnosticParams {
            text_document: TextDocumentIdentifier {
                uri: ctx.doc_uri("source.yaml"),
            },
            identifier: None,
            previous_result_id: None,
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
        .expect("diagnostic request should succeed");
    let DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(report1)) = report
    else {
        panic!("expected full diagnostic report, got: {report:?}");
    };
    let result_id_1 = report1.full_document_diagnostic_report.result_id;
    assert!(result_id_1.is_some());
    assert!(
        report1.full_document_diagnostic_report.items.is_empty(),
        "there should be no diagnostics"
    );

    // The server should return unchanged for repeated requests
    let report = ctx
        .request::<DocumentDiagnosticRequest>(DocumentDiagnosticParams {
            text_document: TextDocumentIdentifier {
                uri: ctx.doc_uri("source.yaml"),
            },
            identifier: None,
            previous_result_id: result_id_1.clone(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
        .expect("diagnostic request should succeed");
    let DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Unchanged(unchanged)) =
        report
    else {
        panic!("expected unchanged diagnostic report, got: {report:?}");
    };
    assert_eq!(
        Some(unchanged.unchanged_document_diagnostic_report.result_id),
        result_id_1
    );

    // Renaming the target in the WDL should also invalidate the YAML
    let wdl_content_v2 = ctx
        .doc_content("source.wdl")
        .replace("say_hello", "say_goodbye");
    ctx.server
        .notify::<DidChangeTextDocument>(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: ctx.doc_uri("source.wdl"),
                version: 1,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: wdl_content_v2.to_string(),
            }],
        })
        .unwrap();

    let report = ctx
        .request::<DocumentDiagnosticRequest>(DocumentDiagnosticParams {
            text_document: TextDocumentIdentifier {
                uri: ctx.doc_uri("source.yaml"),
            },
            identifier: None,
            previous_result_id: result_id_1.clone(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
        .expect("diagnostic request should succeed");
    let DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(report2)) = report
    else {
        panic!("expected full diagnostic report after WDL change, got: {report:?}");
    };
    let result_id_2 = report2.full_document_diagnostic_report.result_id;
    assert_ne!(
        result_id_2, result_id_1,
        "result_id must differ after WDL change"
    );
    let has_unknown_target_diag = report2
        .full_document_diagnostic_report
        .items
        .iter()
        .any(|item| item.message.starts_with("no target named `say_hello`"));
    assert!(has_unknown_target_diag);

    // Pulling again should return Unchanged again
    let report = ctx
        .request::<DocumentDiagnosticRequest>(DocumentDiagnosticParams {
            text_document: TextDocumentIdentifier {
                uri: ctx.doc_uri("source.yaml"),
            },
            identifier: None,
            previous_result_id: result_id_2.clone(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
        .expect("diagnostic request should succeed");
    let DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Unchanged(unchanged2)) =
        report
    else {
        panic!("expected unchanged diagnostic report for second result ID, got: {report:?}");
    };
    assert_eq!(
        Some(unchanged2.unchanged_document_diagnostic_report.result_id),
        result_id_2
    );
}
