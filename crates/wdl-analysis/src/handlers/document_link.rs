//! Handlers for document links.
//!
//! This module implements the LSP `textDocument/documentLink` functionality for
//! WDL files.
//!
//! See: [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_documentLink)

use anyhow::Result;
use lsp_types::DocumentLink;
use url::Url;
use wdl_ast::Ast;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Document;
use wdl_ast::v1::ImportSource;
use wdl_ast::v1::LiteralStringText;

use crate::graph::DocumentGraph;
use crate::handlers::common::ensure_analyzed;
use crate::handlers::common::range_from_span;

/// Computes the [`DocumentLink`]s for the given document, if applicable.
///
/// Implementation of [`textDocument/documentLink`]
///
/// [`textDocument/documentLink`]: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_documentLink
pub fn document_link(
    graph: &DocumentGraph,
    document_uri: &Url,
) -> Result<Option<Vec<DocumentLink>>> {
    let (index, root, lines, document) = ensure_analyzed(graph, document_uri)?;

    let Some(Ast::V1(ast)) =
        Document::cast(root).map(|doc| doc.ast_with_version_fallback(document.version()))
    else {
        return Ok(None);
    };

    let mut links = Vec::new();
    for import in ast.imports() {
        let (uri, span) = match import.source() {
            ImportSource::Uri(uri) => {
                let Some(LiteralStringText::Token(text)) = uri.text() else {
                    continue;
                };

                let Ok(uri) = document.uri().join(text.text()) else {
                    continue;
                };

                (uri, text.span())
            }
            ImportSource::ModulePath(path) => {
                let Some(uri) = graph.get_resolved_symbolic_import(index, &path.text()) else {
                    continue;
                };

                ((**uri).clone(), path.span())
            }
        };

        links.push(DocumentLink {
            range: range_from_span(span, &lines)?,
            target: Some(uri),
            tooltip: None,
            data: None,
        })
    }

    if links.is_empty() {
        Ok(None)
    } else {
        Ok(Some(links))
    }
}
