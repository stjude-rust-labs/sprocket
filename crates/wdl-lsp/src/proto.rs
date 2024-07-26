//! Helper functions from converting to and from LSP structures

use anyhow::Context;
use anyhow::Result;
use line_index::LineIndex;
use line_index::WideEncoding;
use tower_lsp::lsp_types::Diagnostic;
use tower_lsp::lsp_types::DiagnosticRelatedInformation;
use tower_lsp::lsp_types::DiagnosticSeverity;
use tower_lsp::lsp_types::Location;
use tower_lsp::lsp_types::NumberOrString;
use tower_lsp::lsp_types::Position;
use tower_lsp::lsp_types::Range;
use url::Url;
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

    let related = labels
        .map(|label| {
            Ok(DiagnosticRelatedInformation {
                location: Location::new(uri.clone(), range_from_span(index, label.span())?),
                message: label.message().to_string(),
            })
        })
        .collect::<Result<_>>()?;

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
