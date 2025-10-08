//! Handlers for signature help requests.
//!
//! This module implements the LSP `textDocument/signatureHelp` functionality
//! for WDL files. It provides context-aware signature help for standard library
//! functions.
//!
//! See: [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_signatureHelp)

use anyhow::Result;
use anyhow::bail;
use lsp_types::Documentation;
use lsp_types::MarkupContent;
use lsp_types::MarkupKind;
use lsp_types::ParameterInformation;
use lsp_types::ParameterLabel;
use lsp_types::SignatureHelp;
use lsp_types::SignatureInformation;
use url::Url;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_ast::TreeToken;
use wdl_ast::v1::CallExpr;
use wdl_ast::v1::CloseParen;
use wdl_ast::v1::OpenParen;

use crate::SourcePosition;
use crate::SourcePositionEncoding;
use crate::graph::DocumentGraph;
use crate::graph::ParseState;
use crate::handlers::common::position_to_offset;
use crate::stdlib::Function;
use crate::stdlib::STDLIB;
use crate::stdlib::TypeParameters;

/// Handles a signature help request.
pub fn signature_help(
    graph: &DocumentGraph,
    uri: &Url,
    position: SourcePosition,
    encoding: SourcePositionEncoding,
) -> Result<Option<SignatureHelp>> {
    let Some(index) = graph.get_index(uri) else {
        bail!("document `{uri}` not found in graph")
    };
    let node = graph.get(index);
    let (root, lines) = match node.parse_state() {
        ParseState::Parsed { root, lines, .. } => {
            (SyntaxNode::new_root(root.clone()), lines.clone())
        }
        _ => bail!("document `{uri} has not been parsed",),
    };
    let offset = position_to_offset(&lines, position, encoding)?;
    let Some(token) = root.token_at_offset(offset).left_biased() else {
        return Ok(None);
    };

    let Some(call_expr) = token.parent_ancestors().find_map(CallExpr::cast) else {
        return Ok(None);
    };

    let Some(open_paren) = call_expr.token::<OpenParen>() else {
        return Ok(None);
    };

    let offset_usize = u32::from(offset) as usize;
    if offset_usize < open_paren.span().end() {
        return Ok(None);
    }

    if let Some(close_paren) = call_expr.token::<CloseParen>()
        && offset_usize > close_paren.span().start()
    {
        return Ok(None);
    }

    let Some(func) = STDLIB.function(call_expr.target().text()) else {
        return Ok(None);
    };

    let active_parameter = call_expr
        .inner()
        .children_with_tokens()
        .filter(|t| t.kind() == SyntaxKind::Comma)
        .take_while(|t| {
            let span = match t.as_token() {
                Some(t) => t.span(),
                None => return false,
            };
            span.start() < offset.into()
        })
        .count() as u32;

    let signatures = match func {
        Function::Monomorphic(m) => vec![m.signature()],
        Function::Polymorphic(p) => p.signatures().iter().collect(),
    };

    let sig_info: Vec<_> = signatures
        .into_iter()
        .map(|s| {
            let params = TypeParameters::new(s.type_parameters());
            let label = format!("{}{}", call_expr.target().text(), s.display(&params));

            let mut curr_offset = call_expr.target().text().len() + 1; // NOTE: `func` + `(`
            let parameters = s
                .parameters()
                .iter()
                .map(|p| {
                    let param_label = format!("{}: {}", p.name(), p.ty().display(&params));
                    let start = curr_offset as u32;
                    let end = start + param_label.len() as u32;

                    curr_offset += param_label.len() + 2; // NOTE: COMMA + SPACE

                    ParameterInformation {
                        label: ParameterLabel::LabelOffsets([start, end]),
                        documentation: Some(Documentation::MarkupContent(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: p.description().to_string(),
                        })),
                    }
                })
                .collect();

            SignatureInformation {
                label,
                // documentation: s.definition().map(|d| {
                //     Documentation::MarkupContent(MarkupContent {
                //         kind: MarkupKind::Markdown,
                //         value: d.to_string(),
                //     })
                // }),
                documentation: None,
                parameters: Some(parameters),
                active_parameter: Some(active_parameter),
            }
        })
        .collect();

    if sig_info.is_empty() {
        return Ok(None);
    };

    Ok(Some(SignatureHelp {
        signatures: sig_info,
        active_signature: Some(0),
        active_parameter: Some(active_parameter),
    }))
}
