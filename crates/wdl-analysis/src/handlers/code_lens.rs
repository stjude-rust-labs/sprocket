//! Handlers for code lens requests.
//!
//! This module implements the LSP `textDocument/codeLens` functionality for
//! WDL files.
//!
//! See: [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_codeLens)

use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use line_index::TextSize;
use lsp_types::CodeLens;
use lsp_types::Command;
use lsp_types::Range;
use url::Url;
use wdl_grammar::SyntaxNode;

use crate::graph::DocumentGraph;
use crate::graph::ParseState;
use crate::handlers::common::position;

/// A `sprocket run` command, to be run on an LSP client.
#[derive(Debug)]
pub struct RunCommand {
    /// The source document URI.
    pub source: String,
    /// The target to run.
    pub target: String,
}

impl From<RunCommand> for Command {
    fn from(command: RunCommand) -> Self {
        Command {
            title: format!("Run '{}'", command.target),
            command: "sprocket.run".to_string(),
            arguments: Some(vec![command.source.into(), command.target.into()]),
        }
    }
}

/// Computes the [`CodeLens`]es for the given document, if applicable.
///
/// Implementation of [`textDocument/codeLens`]
///
/// [`textDocument/codeLens`]: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_codeLens
pub fn code_lens(graph: &DocumentGraph, document_uri: &Url) -> Result<Option<Vec<CodeLens>>> {
    let index = graph
        .get_index(document_uri)
        .ok_or_else(|| anyhow!("document `{uri}` not found in graph", uri = document_uri))?;

    let node = graph.get(index);
    let (_, lines) = match node.parse_state() {
        ParseState::Parsed { lines, root, .. } => {
            (SyntaxNode::new_root(root.clone()), lines.clone())
        }
        _ => bail!("document `{uri}` has not been parsed", uri = document_uri),
    };

    let Some(analysis_doc) = node.document() else {
        bail!("document analysis data not available for `{document_uri}`");
    };

    let mut lenses = Vec::new();
    for target in analysis_doc.callables() {
        if target.inputs().values().any(|i| i.required()) {
            continue;
        }

        let start_offset = TextSize::from(target.name_span().start() as u32);
        let end_offset = TextSize::from(target.name_span().end() as u32);

        lenses.push(CodeLens {
            range: Range {
                start: position(&lines, start_offset)?,
                end: position(&lines, end_offset)?,
            },
            command: Some(
                RunCommand {
                    source: document_uri.to_string(),
                    target: target.name().to_string(),
                }
                .into(),
            ),
            data: None,
        });
    }

    if lenses.is_empty() {
        Ok(None)
    } else {
        Ok(Some(lenses))
    }
}
