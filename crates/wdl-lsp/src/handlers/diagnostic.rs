//! Handler for `textDocument/diagnostic` requests in Sprocket test YAML files.

use anyhow::Result;
use anyhow::anyhow;
use async_lsp::lsp_types::DocumentDiagnosticParams;
use async_lsp::lsp_types::DocumentDiagnosticReport;
use async_lsp::lsp_types::DocumentDiagnosticReportResult;
use async_lsp::lsp_types::RelatedFullDocumentDiagnosticReport;

use crate::ServerOptions;
use crate::proto::document_diagnostic_report;
use crate::server::ProgressToken;
use crate::server::ServerState;

/// Create an empty diagnostic report.
fn empty_diagnostics() -> DocumentDiagnosticReportResult {
    DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(
        RelatedFullDocumentDiagnosticReport::default(),
    ))
}

/// Computes the diagnostic report for the given Sprocket test YAML, if
/// applicable.
///
/// Implementation of [`textDocument/diagnostic`]
///
/// [`textDocument/diagnostic`]: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_diagnostic
pub async fn document_diagnostic<S>(
    params: DocumentDiagnosticParams,
    state: &ServerState<S>,
    options: &ServerOptions,
) -> Result<DocumentDiagnosticReportResult> {
    let Some(test_yaml) = state.test_yamls.get(&params.text_document.uri).await else {
        return Ok(empty_diagnostics());
    };

    let Some(associated_wdl) = test_yaml.associated_wdl() else {
        return Ok(empty_diagnostics());
    };

    let Some(result) = state
        .config
        .analyzer
        .analyze_document(ProgressToken::default(), associated_wdl.clone())
        .await?
        .into_iter()
        .find(|r| *r.document().uri() == associated_wdl)
    else {
        return Ok(empty_diagnostics());
    };

    let Some(result) = state
        .test_yamls
        .analyze_document(params.text_document.uri.clone(), result.document())
        .await?
    else {
        return Ok(empty_diagnostics());
    };
    document_diagnostic_report(
        params,
        &result.id,
        result.diagnostics.iter(),
        &options.name,
        &result.document.lines,
        |_| true,
    )
    .ok_or_else(|| anyhow!("no diagnostic report produced"))
}
