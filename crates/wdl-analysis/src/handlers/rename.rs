//! Handlers for "rename" requests.
//!
//! This module implements the LSP `textDocument/rename` functionality for WDL
//! files.
//!
//! See: [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_rename)

use std::collections::HashMap;

use anyhow::Result;
use anyhow::bail;
use lsp_types::TextEdit;
use lsp_types::Url;
use lsp_types::WorkspaceEdit;
use serde_json::Map;
use serde_json::Value;
use serde_json::json;
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

    Ok(Some(workspace_edit_from_changes(&changes)?))
}

fn workspace_edit_from_changes(changes: &HashMap<Url, Vec<TextEdit>>) -> Result<WorkspaceEdit> {
    let mut changes_json = Map::new();
    let mut document_changes = Vec::with_capacity(changes.len());

    for (uri, edits) in changes {
        let edits = serde_json::to_value(edits)?;
        changes_json.insert(uri.to_string(), edits.clone());
        document_changes.push(json!({
            "textDocument": {
                "uri": uri,
                "version": null,
            },
            "edits": edits,
        }));
    }

    let edit = serde_json::from_value::<WorkspaceEdit>(json!({
        "changes": Value::Object(changes_json),
        "documentChanges": document_changes,
    }))?;

    Ok(edit)
}
