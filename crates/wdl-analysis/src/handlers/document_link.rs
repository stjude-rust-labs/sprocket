//! Handlers for document links.
//!
//! This module implements the LSP `textDocument/documentLink` functionality for
//! WDL files.
//!
//! See: [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_documentLink)

use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use anyhow::ensure;
use lsp_types::DocumentLink;
use url::Url;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Document;
use wdl_ast::v1::ImportSource;
use wdl_ast::v1::LiteralStringText;
use wdl_grammar::SyntaxNode;

use crate::graph::DocumentGraph;
use crate::graph::ParseState;
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
    let index = graph
        .get_index(document_uri)
        .ok_or_else(|| anyhow!("document `{uri}` not found in graph", uri = document_uri))?;

    let node = graph.get(index);
    let (root, lines) = match node.parse_state() {
        ParseState::Parsed { lines, root, .. } => {
            (SyntaxNode::new_root(root.clone()), lines.clone())
        }
        _ => bail!("document `{uri}` has not been parsed", uri = document_uri),
    };

    let Some(ast) = Document::cast(root).and_then(|doc| doc.ast().into_v1()) else {
        return Ok(None);
    };

    ensure!(
        node.document().is_some(),
        "document analysis data not available for `{document_uri}`"
    );

    let mut links = Vec::new();
    for import in ast.imports() {
        let (uri, span) = match import.source() {
            ImportSource::Uri(uri) => {
                let Some(LiteralStringText::Token(text)) = uri.text() else {
                    continue;
                };

                let Ok(uri) = node.uri().join(text.text()) else {
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
