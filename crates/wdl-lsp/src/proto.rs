//! Helper functions from converting to and from LSP structures

use std::collections::HashMap;

use anyhow::Context;
use anyhow::Result;
use line_index::LineIndex;
use line_index::WideEncoding;
use tower_lsp::lsp_types::Diagnostic;
use tower_lsp::lsp_types::DiagnosticRelatedInformation;
use tower_lsp::lsp_types::DiagnosticSeverity;
use tower_lsp::lsp_types::DocumentDiagnosticParams;
use tower_lsp::lsp_types::DocumentDiagnosticReport;
use tower_lsp::lsp_types::DocumentDiagnosticReportResult;
use tower_lsp::lsp_types::FullDocumentDiagnosticReport;
use tower_lsp::lsp_types::Location;
use tower_lsp::lsp_types::NumberOrString;
use tower_lsp::lsp_types::Position;
use tower_lsp::lsp_types::Range;
use tower_lsp::lsp_types::RelatedFullDocumentDiagnosticReport;
use tower_lsp::lsp_types::RelatedUnchangedDocumentDiagnosticReport;
use tower_lsp::lsp_types::UnchangedDocumentDiagnosticReport;
use tower_lsp::lsp_types::WorkspaceDiagnosticParams;
use tower_lsp::lsp_types::WorkspaceDiagnosticReport;
use tower_lsp::lsp_types::WorkspaceDiagnosticReportResult;
use tower_lsp::lsp_types::WorkspaceDocumentDiagnosticReport;
use tower_lsp::lsp_types::WorkspaceFullDocumentDiagnosticReport;
use tower_lsp::lsp_types::WorkspaceUnchangedDocumentDiagnosticReport;
use tracing::debug;
use url::Url;
use wdl_analysis::AnalysisResult;
use wdl_ast::Severity;
use wdl_ast::Span;

/// Converts a file byte offset to an LSP position.
pub fn position(index: &LineIndex, offset: usize) -> Result<Position> {
    let line_col = index.line_col(offset.try_into()?);
    let line_col = index
        .to_wide(WideEncoding::Utf16, line_col)
        .with_context(|| {
            format!(
                "invalid line column: {line}:{column}",
                line = line_col.line,
                column = line_col.col
            )
        })?;

    Ok(Position::new(line_col.line, line_col.col))
}

/// Converts a diagnostic span into an LSP range.
pub fn range_from_span(index: &LineIndex, span: Span) -> Result<Range> {
    Ok(Range::new(
        position(index, span.start())?,
        position(index, span.end())?,
    ))
}

/// Converts a WDL diagnostic into an LSP diagnostic.
pub fn diagnostic(
    uri: &Url,
    index: &LineIndex,
    source: &str,
    diagnostic: &wdl_ast::Diagnostic,
) -> Result<Diagnostic> {
    let mut labels = diagnostic.labels();

    let range = labels
        .next()
        .map(|label| range_from_span(index, label.span()))
        .transpose()?;

    let severity = match diagnostic.severity() {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::Note => DiagnosticSeverity::INFORMATION,
    };

    let code = diagnostic
        .rule()
        .map(|r| NumberOrString::String(r.to_string()));

    let message = diagnostic.message().to_string();

    let mut related: Vec<_> = labels
        .map(|label| {
            Ok(DiagnosticRelatedInformation {
                location: Location::new(uri.clone(), range_from_span(index, label.span())?),
                message: label.message().to_string(),
            })
        })
        .collect::<Result<_>>()?;

    if let Some(fix) = diagnostic.fix()
        && let Some(span) = diagnostic.labels().next().map(|l| l.span())
    {
        related.push(DiagnosticRelatedInformation {
            location: Location::new(uri.clone(), range_from_span(index, span)?),
            message: format!("fix: {fix}"),
        });
    }

    Ok(Diagnostic::new(
        range.unwrap_or_default(),
        Some(severity),
        code,
        Some(source.to_owned()),
        message,
        Some(related),
        None,
    ))
}

/// Converts analysis results into an LSP document diagnostic report.
pub fn document_diagnostic_report(
    params: DocumentDiagnosticParams,
    results: Vec<AnalysisResult>,
    source: &str,
) -> Option<DocumentDiagnosticReportResult> {
    let result = results
        .iter()
        .find(|r| r.document().uri().as_ref() == &params.text_document.uri)?;

    if let Some(previous) = params.previous_result_id {
        if &previous == result.document().id().as_ref() {
            debug!(
                "diagnostics for document `{uri}` have not changed (client has latest)",
                uri = params.text_document.uri,
            );
            return Some(DocumentDiagnosticReportResult::Report(
                DocumentDiagnosticReport::Unchanged(RelatedUnchangedDocumentDiagnosticReport {
                    related_documents: None,
                    unchanged_document_diagnostic_report: UnchangedDocumentDiagnosticReport {
                        result_id: previous,
                    },
                }),
            ));
        }

        debug!(
            "diagnostics for document `{uri}` have changed since last client request",
            uri = params.text_document.uri
        );
    }

    let items = result
        .document()
        .diagnostics()
        .map(|d| {
            diagnostic(
                result.document().uri(),
                result.lines().expect("should have line index"),
                source,
                d,
            )
        })
        .collect::<Result<Vec<_>>>()
        .ok()?;

    Some(DocumentDiagnosticReportResult::Report(
        DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
            related_documents: None,
            full_document_diagnostic_report: FullDocumentDiagnosticReport {
                result_id: Some(result.document().id().as_ref().clone()),
                items,
            },
        }),
    ))
}

/// Converts analysis results into an LSP workspace diagnostic report.
pub fn workspace_diagnostic_report(
    params: WorkspaceDiagnosticParams,
    results: Vec<AnalysisResult>,
    source: &str,
) -> WorkspaceDiagnosticReportResult {
    let ids = params
        .previous_result_ids
        .into_iter()
        .map(|id| (id.uri, id.value))
        .collect::<HashMap<_, _>>();

    let mut items = Vec::new();
    for result in results {
        // Only store local file results
        if result.document().uri().scheme() != "file" {
            continue;
        }

        if let Some(previous) = ids.get(result.document().uri())
            && previous == result.document().id().as_ref()
        {
            debug!(
                "diagnostics for document `{uri}` have not changed (client has latest)",
                uri = result.document().uri(),
            );

            items.push(WorkspaceDocumentDiagnosticReport::Unchanged(
                WorkspaceUnchangedDocumentDiagnosticReport {
                    uri: result.document().uri().as_ref().clone(),
                    version: result.version().map(|v| v as i64),
                    unchanged_document_diagnostic_report: UnchangedDocumentDiagnosticReport {
                        result_id: result.document().id().as_ref().clone(),
                    },
                },
            ));
            continue;
        }

        debug!(
            "diagnostics for document `{uri}` have changed since last client request",
            uri = result.document().uri()
        );

        let diagnostics = result
            .document()
            .diagnostics()
            .filter_map(|d| {
                diagnostic(
                    result.document().uri(),
                    result.lines().expect("should have line index"),
                    source,
                    d,
                )
                .ok()
            })
            .collect();

        items.push(WorkspaceDocumentDiagnosticReport::Full(
            WorkspaceFullDocumentDiagnosticReport {
                uri: result.document().uri().as_ref().clone(),
                version: result.version().map(|v| v as i64),
                full_document_diagnostic_report: FullDocumentDiagnosticReport {
                    result_id: Some(result.document().id().as_ref().clone()),
                    items: diagnostics,
                },
            },
        ))
    }

    WorkspaceDiagnosticReportResult::Report(WorkspaceDiagnosticReport { items })
}
