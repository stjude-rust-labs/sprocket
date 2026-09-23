//! Tests for diagnostics in the LSP.

use async_lsp::lsp_types::Diagnostic;
use async_lsp::lsp_types::DiagnosticSeverity;
use async_lsp::lsp_types::DidChangeTextDocumentParams;
use async_lsp::lsp_types::DidOpenTextDocumentParams;
use async_lsp::lsp_types::DocumentDiagnosticParams;
use async_lsp::lsp_types::DocumentDiagnosticReport;
use async_lsp::lsp_types::DocumentDiagnosticReportResult;
use async_lsp::lsp_types::Position;
use async_lsp::lsp_types::Range;
use async_lsp::lsp_types::TextDocumentContentChangeEvent;
use async_lsp::lsp_types::TextDocumentIdentifier;
use async_lsp::lsp_types::VersionedTextDocumentIdentifier;
use async_lsp::lsp_types::WorkspaceDiagnosticReportResult;
use async_lsp::lsp_types::WorkspaceDocumentDiagnosticReport;
use async_lsp::lsp_types::notification::DidChangeTextDocument;
use async_lsp::lsp_types::notification::DidOpenTextDocument;
use async_lsp::lsp_types::request::DocumentDiagnosticRequest;
use wdl_lint::Baseline;
use wdl_lint::BaselineEntry;
use wdl_lsp::LintOptions;
use wdl_lsp::UserOptions;

use crate::common::TestContextBuilder;

/// Extracts all diagnostic rule codes from a workspace diagnostic report.
fn diagnostic_codes(report: &WorkspaceDiagnosticReportResult) -> Vec<String> {
    let WorkspaceDiagnosticReportResult::Report(report) = report else {
        return Vec::new();
    };

    report
        .items
        .iter()
        .flat_map(|item| match item {
            WorkspaceDocumentDiagnosticReport::Full(full) => {
                full.full_document_diagnostic_report.items.clone()
            }
            WorkspaceDocumentDiagnosticReport::Unchanged(_) => Vec::new(),
        })
        .filter_map(|d| match d.code {
            Some(async_lsp::lsp_types::NumberOrString::String(s)) => Some(s),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn baseline_suppresses_matching_diagnostics() {
    let mut ctx = TestContextBuilder::new("baseline")
        .user_options(UserOptions {
            lint: LintOptions {
                enabled: true,
                ..Default::default()
            },
            ..Default::default()
        })
        .build_with_options_fn(|workspace, server_options, _| {
            let hash = blake3::hash(b"x").to_hex();
            let baseline = Baseline::new(vec![
                BaselineEntry::new("InputName", "source.wdl", hash),
                BaselineEntry::new("UnusedInput", "source.wdl", hash),
            ])
            .with_base_dir(workspace.to_path_buf());

            server_options.baseline = Some(baseline);
        });
    ctx.initialize().await;
    let report = ctx.workspace_diagnostic().await;
    let codes = diagnostic_codes(&report);

    assert!(
        codes.contains(&"MetaSections".to_string()),
        "`MetaSections` should not be suppressed; got: {codes:?}"
    );
    assert!(
        !codes.contains(&"InputName".to_string()),
        "`InputName` should be suppressed; got: {codes:?}"
    );
    assert!(
        !codes.contains(&"UnusedInput".to_string()),
        "`UnusedInput` should be suppressed; got: {codes:?}"
    );
}

#[tokio::test]
async fn no_baseline_reports_all_diagnostics() {
    let mut ctx = TestContextBuilder::new("baseline")
        .user_options(UserOptions {
            lint: LintOptions {
                enabled: true,
                ..Default::default()
            },
            ..Default::default()
        })
        .build();
    ctx.initialize().await;
    let report = ctx.workspace_diagnostic().await;
    let codes = diagnostic_codes(&report);

    assert!(
        codes.contains(&"MetaSections".to_string()),
        "`MetaSections` should be reported; got: {codes:?}"
    );
    assert!(
        codes.contains(&"InputName".to_string()),
        "`InputName` should be reported; got: {codes:?}"
    );
    assert!(
        codes.contains(&"UnusedInput".to_string()),
        "`UnusedInput` should be reported; got: {codes:?}"
    );
}

#[tokio::test]
async fn baseline_still_suppresses_after_repeated_pulls() {
    let mut ctx = TestContextBuilder::new("baseline")
        .user_options(UserOptions {
            lint: LintOptions {
                enabled: true,
                ..Default::default()
            },
            ..Default::default()
        })
        .build_with_options_fn(|workspace, server_options, _| {
            let hash = blake3::hash(b"x").to_hex();
            let baseline = Baseline::new(vec![
                BaselineEntry::new("InputName", "source.wdl", hash),
                BaselineEntry::new("UnusedInput", "source.wdl", hash),
            ])
            .with_base_dir(workspace.to_path_buf());

            server_options.baseline = Some(baseline);
        });

    ctx.initialize().await;
    let first = ctx.workspace_diagnostic().await;
    let codes = diagnostic_codes(&first);
    assert!(
        !codes.contains(&"InputName".to_string()),
        "`InputName` should be suppressed on first pull; got: {codes:?}"
    );
    assert!(
        !codes.contains(&"UnusedInput".to_string()),
        "`UnusedInput` should be suppressed on first pull; got: {codes:?}"
    );

    for pull in 2..=3 {
        let report = ctx.workspace_diagnostic().await;
        let codes = diagnostic_codes(&report);
        assert!(
            !codes.contains(&"InputName".to_string()),
            "`InputName` should still be suppressed on pull {pull}; got: {codes:?}"
        );
        assert!(
            !codes.contains(&"UnusedInput".to_string()),
            "`UnusedInput` should still be suppressed on pull {pull}; got: {codes:?}"
        );
    }
}

fn assert_diagnostics(response: Vec<Diagnostic>, mut expected: Vec<Diagnostic>) {
    for actual in response {
        let matched = expected.iter().position(|expected| {
            expected.range == actual.range
                && expected.severity == actual.severity
                && actual.message.starts_with(expected.message.as_str())
        });

        if let Some(index) = matched {
            expected.remove(index);
        } else {
            panic!("unexpected diagnostic returned: {actual:?}");
        }
    }

    assert!(
        expected.is_empty(),
        "some expected items were not returned: {expected:?}"
    );
}

#[tokio::test]
async fn should_report_test_yaml_diagnostics() {
    let mut ctx = TestContextBuilder::new("diagnostics").build();
    ctx.initialize().await;

    let doc = ctx.text_document("source.yaml", "yaml");
    ctx.server
        .notify::<DidOpenTextDocument>(DidOpenTextDocumentParams { text_document: doc })
        .unwrap();

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
        .unwrap();

    let DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(report)) = report
    else {
        panic!("Expected diagnostic report");
    };

    let missing_target_diagnostic = Diagnostic {
        range: Range {
            start: Position {
                line: 9,
                character: 0,
            },
            end: Position {
                line: 9,
                character: 10,
            },
        },
        severity: Some(DiagnosticSeverity::ERROR),
        message: "no target named `FakeTarget`".to_string(),
        ..Diagnostic::default()
    };
    let expected = vec![
        Diagnostic {
            range: Range {
                start: Position {
                    line: 3,
                    character: 6,
                },
                end: Position {
                    line: 3,
                    character: 18,
                },
            },
            severity: Some(DiagnosticSeverity::ERROR),
            message: "no input named `not_an_input`".to_string(),
            ..Diagnostic::default()
        },
        Diagnostic {
            range: Range {
                start: Position {
                    line: 7,
                    character: 8,
                },
                end: Position {
                    line: 7,
                    character: 21,
                },
            },
            severity: Some(DiagnosticSeverity::ERROR),
            message: "no output named `not_an_output`".to_string(),
            ..Diagnostic::default()
        },
        missing_target_diagnostic.clone(),
    ];

    assert_diagnostics(report.full_document_diagnostic_report.items, expected);

    // Pulling diagnostics again with previous_result_id set should return
    // Unchanged
    let unchanged_report = ctx
        .request::<DocumentDiagnosticRequest>(DocumentDiagnosticParams {
            text_document: TextDocumentIdentifier {
                uri: ctx.doc_uri("source.yaml"),
            },
            identifier: None,
            previous_result_id: report.full_document_diagnostic_report.result_id.clone(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
        .unwrap();

    let DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Unchanged(unchanged)) =
        unchanged_report
    else {
        panic!("expected unchanged diagnostic report");
    };
    assert_eq!(
        Some(unchanged.unchanged_document_diagnostic_report.result_id),
        report.full_document_diagnostic_report.result_id
    );

    // Move the fake target to the top of the document
    ctx.server
        .notify::<DidChangeTextDocument>(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: ctx.doc_uri("source.yaml"),
                version: 1,
            },
            content_changes: vec![
                TextDocumentContentChangeEvent {
                    range: Some(Range {
                        start: Position {
                            line: 9,
                            character: 0,
                        },
                        end: Position {
                            line: 10,
                            character: 23,
                        },
                    }),
                    range_length: None,
                    text: "".to_string(),
                },
                TextDocumentContentChangeEvent {
                    range: Some(Range {
                        start: Position {
                            line: 0,
                            character: 0,
                        },
                        end: Position {
                            line: 0,
                            character: 0,
                        },
                    }),
                    range_length: None,
                    text: r#"FakeTarget:
  - name: broken_target
"#
                    .to_string(),
                },
            ],
        })
        .unwrap();

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
        .unwrap();

    let DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(report)) = report
    else {
        panic!("Expected diagnostic report");
    };

    let missing_target = report
        .full_document_diagnostic_report
        .items
        .iter()
        .find(|d| d.message.starts_with("no target named `FakeTarget`"))
        .expect("should produce a missing target diagnostic");
    assert_eq!(
        missing_target.range.start.line,
        missing_target_diagnostic.range.start.line - 9
    );
    assert_eq!(
        missing_target.range.end.line,
        missing_target_diagnostic.range.start.line - 9
    );
}

#[tokio::test]
async fn should_ignored_orphaned_test() {
    let mut ctx = TestContextBuilder::new("orphaned_test").build();
    ctx.initialize().await;

    let doc = ctx.text_document("source.yaml", "yaml");
    ctx.server
        .notify::<DidOpenTextDocument>(DidOpenTextDocumentParams { text_document: doc })
        .unwrap();

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
        .unwrap();

    let DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(report)) = report
    else {
        panic!("Expected diagnostic report");
    };

    assert!(report.full_document_diagnostic_report.items.is_empty());
}
