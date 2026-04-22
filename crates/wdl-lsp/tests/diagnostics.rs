//! Tests for diagnostic baseline filtering in the LSP.

mod common;

use tower_lsp::lsp_types::WorkspaceDiagnosticReportResult;
use tower_lsp::lsp_types::WorkspaceDocumentDiagnosticReport;
use wdl_lint::Baseline;
use wdl_lint::BaselineEntry;

use crate::common::TestContext;

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
            Some(tower_lsp::lsp_types::NumberOrString::String(s)) => Some(s),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn baseline_suppresses_matching_diagnostics() {
    let mut ctx = TestContext::with_options_fn("baseline", |workspace| {
        let hash = blake3::hash(b"x").to_hex();
        let baseline = Baseline::new(vec![
            BaselineEntry::new("InputName", "source.wdl", hash),
            BaselineEntry::new("UnusedInput", "source.wdl", hash),
        ])
        .with_base_dir(workspace.to_path_buf());
        wdl_lsp::ServerOptions {
            lint: wdl_lsp::LintOptions {
                enabled: true,
                ..Default::default()
            },
            baseline: Some(baseline),
            ..Default::default()
        }
    });
    let (_, report) = ctx.initialize().await;
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
    let mut ctx = TestContext::new("baseline");
    let (_, report) = ctx.initialize().await;
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
    let mut ctx = TestContext::with_options_fn("baseline", |workspace| {
        let hash = blake3::hash(b"x").to_hex();
        let baseline = Baseline::new(vec![
            BaselineEntry::new("InputName", "source.wdl", hash),
            BaselineEntry::new("UnusedInput", "source.wdl", hash),
        ])
        .with_base_dir(workspace.to_path_buf());
        wdl_lsp::ServerOptions {
            lint: wdl_lsp::LintOptions {
                enabled: true,
                ..Default::default()
            },
            baseline: Some(baseline),
            ..Default::default()
        }
    });

    let (_, first) = ctx.initialize().await;
    for report in [&first] {
        let codes = diagnostic_codes(report);
        assert!(
            !codes.contains(&"InputName".to_string()),
            "`InputName` should be suppressed on first pull; got: {codes:?}"
        );
        assert!(
            !codes.contains(&"UnusedInput".to_string()),
            "`UnusedInput` should be suppressed on first pull; got: {codes:?}"
        );
    }

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
