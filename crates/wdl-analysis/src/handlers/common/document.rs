//! Utilities for interacting with the [`DocumentGraph`].

use std::sync::Arc;

use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use line_index::LineIndex;
use petgraph::graph::NodeIndex;
use url::Url;
use wdl_grammar::SupportedVersion;
use wdl_grammar::SyntaxNode;

use crate::Document;
use crate::graph::DocumentGraph;
use crate::graph::ParseState;

/// Ensure that the given `uri` exists in the `graph` and has been parsed.
///
/// Returns the CST root, line index, and the WDL version used to parse the
/// document
pub fn ensure_parsed(
    graph: &DocumentGraph,
    uri: &Url,
) -> Result<(SyntaxNode, Arc<LineIndex>, Option<SupportedVersion>)> {
    let index = graph
        .get_index(uri)
        .ok_or_else(|| anyhow!("document `{uri}` not found in graph"))?;

    let node = graph.get(index);
    let (root, lines, wdl_version) = match node.parse_state() {
        ParseState::Parsed {
            lines,
            root,
            wdl_version,
            ..
        } => (
            SyntaxNode::new_root(root.clone()),
            lines.clone(),
            *wdl_version,
        ),
        _ => bail!("document `{uri}` has not been parsed"),
    };

    Ok((root, lines, wdl_version))
}

/// Ensure that the given `uri` exists in the `graph` and has been analyzed.
///
/// Returns the document index, CST root, line index, and analyzed document
pub fn ensure_analyzed<'a>(
    graph: &'a DocumentGraph,
    uri: &Url,
) -> Result<(NodeIndex, SyntaxNode, Arc<LineIndex>, &'a Document)> {
    let index = graph
        .get_index(uri)
        .ok_or_else(|| anyhow!("document `{uri}` not found in graph"))?;

    let node = graph.get(index);
    let (root, lines) = match node.parse_state() {
        ParseState::Parsed { lines, root, .. } => {
            (SyntaxNode::new_root(root.clone()), lines.clone())
        }
        _ => bail!("document `{uri}` has not been parsed"),
    };

    let Some(analysis_doc) = node.document() else {
        bail!("document analysis data not available for `{uri}`");
    };

    Ok((index, root, lines, analysis_doc))
}
