//! Namespace resolution utilities for LSP handlers.

use std::sync::Arc;

use line_index::LineIndex;
use url::Url;

use crate::Document;
use crate::graph::DocumentGraph;

/// Context for an imported document accessed through a namespace.
///
/// This struct provides convenient access to all the information needed when
/// resolving symbols in imported documents.
pub struct ImportedDocContext<'a> {
    /// The imported document
    pub doc: &'a Document,
    /// Line index for the imported document
    pub lines: &'a Arc<LineIndex>,
    /// URI of the imported document
    pub uri: &'a Url,
}

/// Gets the context for an imported document by its URI.
///
/// This is a convenience function that performs the common pattern of looking
/// up the source document in the graph and extracting the document and line
/// information.
///
/// Returns an [`ImportedDocContext`] if the document exists in the graph,
/// otherwise [`None`].
pub fn get_imported_doc_context<'a>(
    uri: &'a Url,
    graph: &'a DocumentGraph,
) -> Option<ImportedDocContext<'a>> {
    let node = graph.get(graph.get_index(uri)?);
    let doc = node.document()?;
    let lines = node.parse_state().lines()?;

    Some(ImportedDocContext { doc, lines, uri })
}
