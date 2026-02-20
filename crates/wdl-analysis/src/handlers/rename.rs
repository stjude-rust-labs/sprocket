//! Handlers for "rename" requests.
//!
//! This module implements the LSP `textDocument/rename` functionality for WDL
//! files.
//!
//! See: [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_rename)

use std::collections::HashMap;

use anyhow::Result;
use anyhow::bail;
use ls_types::TextEdit;
use ls_types::Uri;
use ls_types::WorkspaceEdit;
use url::Url;
use wdl_ast::lexer::v1::is_ident;

use crate::SourcePosition;
use crate::SourcePositionEncoding;
use crate::graph::DocumentGraph;
use crate::handlers;

/// Renames a symbol at a given position in a document.
///
/// It first finds all references to the symbol at the given position,
/// including the definition itself. Then, it creates a `WorkspaceEdit`
/// to rename all occurrences.
///
/// The rename is rejected if the new name is not a valid WDL identifier.
pub fn rename(
    graph: &DocumentGraph,
    document_uri: Url,
    position: SourcePosition,
    encoding: SourcePositionEncoding,
    new_name: String,
) -> Result<Option<WorkspaceEdit>> {
    if !is_ident(&new_name) {
        bail!("name '{}' is not a valid WDL identifier.", new_name);
    }

    let locations = handlers::find_all_references(graph, document_uri, position, encoding, true)?;
    if locations.is_empty() {
        return Ok(None);
    }

    let mut changes: HashMap<Uri, Vec<TextEdit>> = HashMap::new();
    for location in locations {
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
