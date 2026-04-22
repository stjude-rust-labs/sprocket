//! Helper functions from converting to and from LSP structures

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Component;
use std::path::PathBuf;
use std::path::Prefix;
use std::str::FromStr;

use anyhow::Context;
use anyhow::Result;
use line_index::LineIndex;
use line_index::WideEncoding;
use tower_lsp_server::jsonrpc::Error as RpcError;
use tower_lsp_server::jsonrpc::ErrorCode;
use tower_lsp_server::jsonrpc::Result as RpcResult;
use tower_lsp_server::ls_types::Diagnostic;
use tower_lsp_server::ls_types::DiagnosticRelatedInformation;
use tower_lsp_server::ls_types::DiagnosticSeverity;
use tower_lsp_server::ls_types::DocumentDiagnosticReport;
use tower_lsp_server::ls_types::DocumentDiagnosticReportResult;
use tower_lsp_server::ls_types::FullDocumentDiagnosticReport;
use tower_lsp_server::ls_types::Location;
use tower_lsp_server::ls_types::NumberOrString;
use tower_lsp_server::ls_types::Position;
use tower_lsp_server::ls_types::Range;
use tower_lsp_server::ls_types::RelatedFullDocumentDiagnosticReport;
use tower_lsp_server::ls_types::RelatedUnchangedDocumentDiagnosticReport;
use tower_lsp_server::ls_types::UnchangedDocumentDiagnosticReport;
use tower_lsp_server::ls_types::Uri;
use tower_lsp_server::ls_types::WorkspaceDiagnosticParams;
use tower_lsp_server::ls_types::WorkspaceDiagnosticReport;
use tower_lsp_server::ls_types::WorkspaceDiagnosticReportResult;
use tower_lsp_server::ls_types::WorkspaceDocumentDiagnosticReport;
use tower_lsp_server::ls_types::WorkspaceFullDocumentDiagnosticReport;
use tower_lsp_server::ls_types::WorkspaceUnchangedDocumentDiagnosticReport;
use tracing::debug;
use url::Url;
use wdl_analysis::AnalysisResult;
use wdl_analysis::handlers::UriToUrl;
use wdl_analysis::handlers::UrlToUri;
use wdl_ast::Severity;
use wdl_ast::Span;

/// A normalized URI.
///
/// If the path contains percent encoded sequences, the sequences are decoded.
///
/// Additionally, on Windows, this will normalize the drive letter to uppercase.
pub struct NormalizedUri {
    /// The normalized URI.
    uri: Uri,
    /// The normalized URL derived from the URI.
    url: Url,
}

impl NormalizedUri {
    /// Get the URI as a [`Uri`].
    pub fn uri(&self) -> &Uri {
        &self.uri
    }

    /// Consume the `NormalizedUri` and return its URI variant.
    pub fn into_uri(self) -> Uri {
        self.uri
    }

    /// Get the URI as a [`Url`].
    pub fn url(&self) -> &Url {
        &self.url
    }

    /// Consume the `NormalizedUri` and return its URL variant.
    pub fn into_url(self) -> Url {
        self.url
    }

    /// Create a `NormalizedUri` from a previously normalized URL.
    fn from_normalized_url(url: &Url) -> Self {
        Self {
            uri: url.try_into_uri().expect("should be a valid URI"),
            url: url.clone(),
        }
    }
}

/// Wrapper for [`UriToUrl`] to return consistent error messages.
fn uri_to_url(uri: &Uri) -> RpcResult<Url> {
    uri.try_into_url().map_err(|e| RpcError {
        code: ErrorCode::InvalidParams,
        message: format!(
            "failed to convert document URI {uri}: {e}",
            uri = uri.as_str()
        )
        .into(),
        data: None,
    })
}

impl TryFrom<Uri> for NormalizedUri {
    type Error = RpcError;

    fn try_from(mut uri: Uri) -> RpcResult<Self> {
        if uri.scheme().as_str() != "file" {
            let url = uri_to_url(&uri)?;
            return Ok(Self { uri, url });
        }

        uri = Uri::from(uri.normalize());
        let Some(path) = uri.to_file_path() else {
            let url = uri_to_url(&uri)?;
            return Ok(Self { uri, url });
        };

        // On windows we need to normalize any drive letter prefixes to uppercase
        let path = if cfg!(windows) {
            let mut comps = path.components();
            match comps.next() {
                Some(Component::Prefix(prefix)) => match prefix.kind() {
                    Prefix::Disk(d) => {
                        let mut path = PathBuf::new();
                        path.push(format!("{}:", d.to_ascii_uppercase() as char));
                        path.extend(comps);
                        Cow::Owned(path)
                    }
                    Prefix::VerbatimDisk(d) => {
                        let mut path = PathBuf::new();
                        path.push(format!(r"\\?\{}:", d.to_ascii_uppercase() as char));
                        path.extend(comps);
                        Cow::Owned(path)
                    }
                    _ => path,
                },
                _ => path,
            }
        } else {
            path
        };

        if let Ok(url) = Url::from_file_path(&*path)
            && let Ok(uri) = Uri::from_str(url.as_str())
        {
            let url = uri_to_url(&uri)?;
            return Ok(Self { uri, url });
        }

        let mut path_str = path.to_string_lossy();
        if cfg!(windows) {
            path_str = Cow::Owned(path_str.replace('\\', "/"));

            // The leading slash on Windows is a shorthand for `localhost` (e.g. `file://localhost/C:/Windows` with the `localhost` omitted).
            if !path_str.starts_with('/') {
                path_str = Cow::Owned(format!("/{path_str}"));
            }
        }

        if let Ok(uri) = Uri::from_str(&format!("file://{path_str}")) {
            let url = uri_to_url(&uri)?;
            return Ok(Self { uri, url });
        }

        let url = uri_to_url(&uri)?;
        Ok(Self { uri, url })
    }
}

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
    uri: &NormalizedUri,
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
                location: Location::new(uri.uri().clone(), range_from_span(index, label.span())?),
                message: label.message().to_string(),
            })
        })
        .collect::<Result<_>>()?;

    if let Some(fix) = diagnostic.fix()
        && let Some(span) = diagnostic.labels().next().map(|l| l.span())
    {
        related.push(DiagnosticRelatedInformation {
            location: Location::new(uri.uri().clone(), range_from_span(index, span)?),
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
    uri: &NormalizedUri,
    previous_result_id: Option<String>,
    results: Vec<AnalysisResult>,
    source: &str,
) -> Option<DocumentDiagnosticReportResult> {
    let result = results
        .iter()
        .find(|r| r.document().uri().as_ref() == uri.url())?;

    if let Some(previous) = previous_result_id {
        if &previous == result.document().id().as_ref() {
            debug!(
                "diagnostics for document `{uri}` have not changed (client has latest)",
                uri = uri.url,
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
            uri = uri.url
        );
    }

    let items = result
        .document()
        .diagnostics()
        .map(|d| {
            // Document URIs are already normalized
            let uri = NormalizedUri::from_normalized_url(result.document().uri());

            diagnostic(
                &uri,
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

        let Some(document_uri) = result.document().uri().try_into_uri().ok() else {
            continue;
        };

        if let Some(previous) = ids.get(&document_uri)
            && previous == result.document().id().as_ref()
        {
            debug!(
                "diagnostics for document `{uri}` have not changed (client has latest)",
                uri = result.document().uri(),
            );

            items.push(WorkspaceDocumentDiagnosticReport::Unchanged(
                WorkspaceUnchangedDocumentDiagnosticReport {
                    uri: document_uri.clone(),
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
                    // Document URIs are already normalized
                    &NormalizedUri::from_normalized_url(result.document().uri()),
                    result.lines().expect("should have line index"),
                    source,
                    d,
                )
                .ok()
            })
            .collect();

        items.push(WorkspaceDocumentDiagnosticReport::Full(
            WorkspaceFullDocumentDiagnosticReport {
                uri: document_uri,
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
