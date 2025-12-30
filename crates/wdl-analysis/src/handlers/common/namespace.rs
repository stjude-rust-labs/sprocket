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

/// Gets the context for a document imported through a namespace.
///
/// This is a convenience function that performs the common pattern of looking
/// up a namespace, getting the graph node, and extracting the document and line
/// information.
///
/// Returns an [`ImportedDocContext`] if the namespace and document exist,
/// otherwise [`None`].
pub fn get_imported_doc_context<'a>(
    namespace_name: &str,
    analysis_doc: &'a Document,
    graph: &'a DocumentGraph,
) -> Option<ImportedDocContext<'a>> {
    let ns = analysis_doc.namespace(namespace_name)?;
    let node = graph.get(graph.get_index(ns.source())?);
    let doc = node.document()?;
    let lines = node.parse_state().lines()?;

    Some(ImportedDocContext {
        doc,
        lines,
        uri: ns.source(),
    })
}
