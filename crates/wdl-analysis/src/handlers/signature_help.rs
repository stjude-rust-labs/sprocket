//! Handlers for signature help requests.
//!
//! This module implements the LSP `textDocument/signatureHelp` functionality
//! for WDL files. It provides context-aware signature help for standard library
//! function calls and `call` statements.
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
use rowan::TextSize;
use url::Url;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_ast::TreeToken;
use wdl_ast::v1::CallExpr;
use wdl_ast::v1::CallStatement;
use wdl_ast::v1::CloseBrace;
use wdl_ast::v1::CloseParen;
use wdl_ast::v1::OpenBrace;
use wdl_ast::v1::OpenParen;

use crate::Document;
use crate::SourcePosition;
use crate::SourcePositionEncoding;
use crate::graph::DocumentGraph;
use crate::graph::ParseState;
use crate::handlers::common::position_to_offset;
use crate::handlers::common::provide_callable_documentation;
use crate::handlers::common::provide_callable_inputs_documentation;
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
    let (root, lines, parsed_version) = match node.parse_state() {
        ParseState::Parsed {
            root,
            lines,
            wdl_version,
            ..
        } => (
            SyntaxNode::new_root(root.clone()),
            lines.clone(),
            *wdl_version,
        ),
        _ => bail!("document `{uri} has not been parsed",),
    };
    let version = node
        .document()
        .and_then(|document| document.version())
        .or(parsed_version);
    let Some(version) = version else {
        return Ok(None);
    };
    let offset = position_to_offset(&lines, position, encoding)?;
    let Some(token) = root.token_at_offset(offset).left_biased() else {
        return Ok(None);
    };

    // Standard library function call, e.g. `read_string(...)`.
    if let Some(call_expr) = token.parent_ancestors().find_map(CallExpr::cast) {
        return stdlib_function_help(&call_expr, offset, version);
    }

    // `call` statement, e.g. `call my_task { ... }`.
    if let Some(call_stmt) = token.parent_ancestors().find_map(CallStatement::cast) {
        return call_statement_help(&call_stmt, offset, node.document());
    }

    Ok(None)
}

/// Gets signature help for a standard library function call.
fn stdlib_function_help(
    call_expr: &CallExpr,
    offset: TextSize,
    version: SupportedVersion,
) -> Result<Option<SignatureHelp>> {
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

    let active_parameter = match call_expr
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
        .count()
        .try_into()
    {
        Ok(c) => c,
        Err(_) => {
            bail!("exceeded max number of parameters for signature help.")
        }
    };

    let (signatures, definition) = match func {
        Function::Monomorphic(m) if m.minimum_version() <= version => {
            (vec![m.signature()], m.signature().definition())
        }
        Function::Monomorphic(_) => return Ok(None),
        Function::Polymorphic(p) => (
            p.signatures()
                .iter()
                .filter(|s| s.minimum_version() <= version)
                .collect(),
            p.definition(version),
        ),
    };

    let sig_info: Vec<_> = signatures
        .into_iter()
        .map(|s| {
            let params = TypeParameters::new(s.type_parameters());
            let label = format!("{}{}", call_expr.target().text(), s.display(&params));

            let mut curr_offset = call_expr.target().text().len() + 1; // NOTE: `func` + `(`
            let required = s.required();
            let parameters = s
                .parameters()
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    if i > 0 {
                        curr_offset += 2; // for `, `
                    }

                    if i >= required {
                        curr_offset += 1; // for `<`
                    }

                    let param_label = format!("{}: {}", p.name(), p.ty().display(&params));
                    let start = curr_offset as u32;
                    let end = start + param_label.len() as u32;

                    curr_offset += param_label.len();

                    if i >= required {
                        curr_offset += 1; // for `>`
                    }
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
                documentation: definition.map(|def| {
                    Documentation::MarkupContent(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: def.to_string(),
                    })
                }),
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

/// Gets signature help for a `call` statement.
///
/// The signature describes the called task or workflow's inputs, and the
/// active parameter is determined by the number of inputs that precede the
/// cursor within the statement's input region.
fn call_statement_help(
    call_stmt: &CallStatement,
    offset: TextSize,
    document: Option<&Document>,
) -> Result<Option<SignatureHelp>> {
    let Some(document) = document else {
        return Ok(None);
    };

    // Signature help is only provided within the statement's input region.
    let Some(open_brace) = call_stmt.token::<OpenBrace>() else {
        return Ok(None);
    };

    let offset_usize = u32::from(offset) as usize;
    if offset_usize < open_brace.span().end() {
        return Ok(None);
    }

    if let Some(close_brace) = call_stmt.token::<CloseBrace>()
        && offset_usize > close_brace.span().start()
    {
        return Ok(None);
    }

    // Resolve the called task or workflow.
    let target = call_stmt.target();
    let mut target_names = target.names();
    let (namespace_name, callee_name) = match (target_names.next(), target_names.next()) {
        // Namespaced call, e.g. `call ns.task { ... }`.
        (Some(ns), Some(name)) => (Some(ns), name),
        // Local call.
        (Some(name), None) => (None, name),
        _ => return Ok(None),
    };

    let target_doc = if let Some(ns_name) = namespace_name {
        let Some(ns) = document.namespace(ns_name.text()) else {
            return Ok(None);
        };
        ns.document()
    } else {
        document
    };

    let Some(callable) = target_doc.callable_by_name(callee_name.text()) else {
        return Ok(None);
    };

    // The active parameter is the number of inputs that precede the cursor.
    let active_parameter = match call_stmt
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
        .count()
        .try_into()
    {
        Ok(c) => c,
        Err(_) => {
            bail!("exceeded max number of parameters for signature help.")
        }
    };

    // Build the signature label and parameter information from the callable's
    // inputs. Inputs that are not required are wrapped in angle brackets.
    let kind = if callable.is_workflow() {
        "workflow"
    } else {
        "task"
    };
    let inputs = callable.inputs();
    let input_docs = provide_callable_inputs_documentation(callable, &target_doc.root());

    let mut label = format!("{kind} {}(", callable.name());
    let mut curr_offset = label.len(); // NOTE: everything up to and including `(`
    let mut parameters = Vec::with_capacity(inputs.len());
    for (i, (input_name, input)) in inputs.iter().enumerate() {
        if i > 0 {
            label.push_str(", ");
            curr_offset += 2; // for `, `
        }

        if !input.required() {
            label.push('<');
            curr_offset += 1; // for `<`
        }

        let param_label = format!("{}: {}", input_name, input.ty());
        let start = curr_offset as u32;
        let end = start + param_label.len() as u32;

        label.push_str(&param_label);
        curr_offset += param_label.len();

        if !input.required() {
            label.push('>');
            curr_offset += 1; // for `>`
        }

        parameters.push(ParameterInformation {
            label: ParameterLabel::LabelOffsets([start, end]),
            documentation: input_docs
                .as_ref()
                .and_then(|docs| docs.get(input_name))
                .map(|description| {
                    Documentation::MarkupContent(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: description.clone(),
                    })
                }),
        });
    }
    label.push(')');

    Ok(Some(SignatureHelp {
        signatures: vec![SignatureInformation {
            label,
            documentation: provide_callable_documentation(callable, &target_doc.root()).map(
                |doc| {
                    Documentation::MarkupContent(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: doc,
                    })
                },
            ),
            parameters: Some(parameters),
            active_parameter: Some(active_parameter),
        }],
        active_signature: Some(0),
        active_parameter: Some(active_parameter),
    }))
}
