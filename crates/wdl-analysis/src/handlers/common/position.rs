//! Utilities for working with positions and ranges.

use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use line_index::LineIndex;
use line_index::WideEncoding;
use lsp_types::Location;
use lsp_types::Position;
use rowan::TextSize;
use url::Url;
use wdl_ast::Span;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_ast::SyntaxToken;

use crate::SourcePosition;
use crate::SourcePositionEncoding;

/// Converts a text size offset to LSP position.
pub fn position(index: &LineIndex, offset: TextSize) -> Result<Position> {
    let line_col = index.line_col(offset);
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

/// Converts a `Span` to an LSP location.
pub fn location_from_span(uri: &Url, span: Span, lines: &Arc<LineIndex>) -> Result<Location> {
    let start_offset = TextSize::from(span.start() as u32);
    let end_offset = TextSize::from(span.end() as u32);
    let range = lsp_types::Range {
        start: position(lines, start_offset)?,
        end: position(lines, end_offset)?,
    };

    Ok(Location::new(uri.clone(), range))
}

/// Converts a source position to a text offset based on the specified encoding.
pub fn position_to_offset(
    lines: &Arc<LineIndex>,
    position: SourcePosition,
    encoding: SourcePositionEncoding,
) -> Result<TextSize> {
    let line_col = match encoding {
        SourcePositionEncoding::UTF8 => line_index::LineCol {
            line: position.line,
            col: position.character,
        },
        SourcePositionEncoding::UTF16 => {
            let wide_col = line_index::WideLineCol {
                line: position.line,
                col: position.character,
            };
            lines
                .to_utf8(line_index::WideEncoding::Utf16, wide_col)
                .ok_or_else(|| anyhow!("invalid utf-16 position: {position:?}"))?
        }
    };

    lines
        .offset(line_col)
        .ok_or_else(|| anyhow!("line_col is invalid"))
}

/// Finds an identifier token at the specified `TextSize` offset in the concrete
/// syntax tree.
pub fn find_identifier_token_at_offset(node: &SyntaxNode, offset: TextSize) -> Option<SyntaxToken> {
    node.token_at_offset(offset)
        .find(|t| t.kind() == SyntaxKind::Ident)
}
