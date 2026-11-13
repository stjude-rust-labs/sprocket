//! Handlers for "rename" requests.
//!
//! This module implements the LSP `textDocument/rename` functionality for WDL
//! files.
//!
//! See: [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_rename)

use std::collections::HashMap;

use anyhow::Result;
use anyhow::bail;
use lsp_types::PrepareRenameResponse;
use lsp_types::TextEdit;
use lsp_types::Url;
use lsp_types::WorkspaceEdit;
use wdl_ast::TreeToken;
use wdl_ast::lexer::v1::is_ident;

use crate::SourcePosition;
use crate::SourcePositionEncoding;
use crate::graph::DocumentGraph;
use crate::handlers;
use crate::handlers::common::ensure_analyzed;
use crate::handlers::common::find_identifier_token_at_offset;
use crate::handlers::common::position_to_offset;
use crate::handlers::common::range_from_span;

/// Get the range of the symbol at `position` for a rename.
///
/// Returns `Ok(None)` for invalid renames (e.g., renaming a keyword), otherwise
/// it returns the span of the closest identifier.
pub fn prepare_rename(
    graph: &DocumentGraph,
    document_uri: &Url,
    position: SourcePosition,
    encoding: SourcePositionEncoding,
) -> Result<Option<PrepareRenameResponse>> {
    let (_, root, lines, _) = ensure_analyzed(graph, document_uri)?;

    let offset = position_to_offset(&lines, position, encoding)?;
    let Some(token) = find_identifier_token_at_offset(&root, offset) else {
        return Ok(None);
    };

    if handlers::goto_definition(graph, document_uri, position, encoding)?.is_none() {
        return Ok(None); // Not a valid definition, reject the rename
    }

    Ok(Some(PrepareRenameResponse::RangeWithPlaceholder {
        range: range_from_span(token.span(), &lines)?,
        placeholder: token.text().to_string(),
    }))
}

/// Renames a symbol at a given position in a document.
///
/// It first finds all references to the symbol at the given position,
/// including the definition itself. Then, it creates a `WorkspaceEdit`
/// to rename all occurrences.
///
/// The rename is rejected if the new name is not a valid WDL identifier.
pub fn rename(
    graph: &DocumentGraph,
    document_uri: &Url,
    position: SourcePosition,
    encoding: SourcePositionEncoding,
    new_name: String,
) -> Result<Option<WorkspaceEdit>> {
    if !is_ident(&new_name) {
        bail!("name `{new_name}` is not a valid WDL identifier");
    }

    let references = handlers::find_all_references(graph, document_uri, position, encoding, true)?;
    if references.is_empty() {
        return Ok(None);
    }

    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
    for location in references {
        let text_edit = TextEdit {
            range: location.range,
            new_text: new_name.clone(),
        };
        changes.entry(location.uri).or_default().push(text_edit);
    }

    Ok(Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    }))
}
