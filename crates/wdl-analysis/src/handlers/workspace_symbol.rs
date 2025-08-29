//! Handlers for workspace symbols.
//!
//! This module implements the LSP "workspace/symbol" functionality
//! for WDL files. It searches for symbols across all documents in the
//! workspace.
//!
//! See: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#workspace_symbol

use anyhow::Result;
use lsp_types::DocumentSymbol;
use lsp_types::DocumentSymbolResponse;
use lsp_types::Location;
use lsp_types::SymbolInformation;
use url::Url;

use crate::graph::DocumentGraph;
use crate::handlers;

/// Handles a workspace symbol request.
pub fn workspace_symbol(
    graph: &DocumentGraph,
    query: &str,
) -> Result<Option<Vec<SymbolInformation>>> {
    let mut symbols = Vec::new();

    for index in graph.inner().node_indices() {
        let node = graph.get(index);
        if let Some(doc) = node.document()
            && let Ok(Some(doc_symbols)) = handlers::document_symbol(graph, doc.uri())
            && let DocumentSymbolResponse::Nested(nested) = doc_symbols
        {
            flatten_document_symbols(doc.uri(), &nested, None, query, &mut symbols)?;
        }
    }

    Ok(Some(symbols))
}

/// Recursively flattens [DocumentSymbol]'s into [SymbolInformation].
fn flatten_document_symbols(
    uri: &Url,
    document_symbols: &[DocumentSymbol],
    parent_name: Option<&str>,
    query: &str,
    symbols: &mut Vec<SymbolInformation>,
) -> Result<()> {
    for symbol in document_symbols {
        if query.is_empty() || symbol.name.contains(query) {
            #[allow(deprecated)]
            symbols.push(SymbolInformation {
                name: symbol.name.clone(),
                kind: symbol.kind,
                tags: symbol.tags.clone(),
                deprecated: symbol.deprecated,
                location: Location {
                    uri: uri.clone(),
                    range: symbol.range,
                },
                container_name: parent_name.map(|s| s.to_string()),
            });
        }

        if let Some(children) = &symbol.children {
            flatten_document_symbols(uri, children, Some(&symbol.name), query, symbols)?;
        }
    }

    Ok(())
}
