//! Handlers for "find all references" requests.
//!
//! This module implements the LSP "textDocument/references" functionality for
//! WDL files. It finds all references to a symbol by first resolving the
//! symbol's definition, then searches through all the appropriate documents.
//!
//! See: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_references

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use lsp_types::Location;
use url::Url;
use wdl_ast::AstNode;
use wdl_ast::SyntaxKind;
use wdl_ast::TreeToken;

use crate::SourcePosition;
use crate::SourcePositionEncoding;
use crate::graph::DocumentGraph;
use crate::handlers;
use crate::handlers::common::location_from_span;
use crate::handlers::common::position;
use crate::handlers::common::position_to_offset;

/// Represents a target definition for which references are being searched.
#[derive(Debug)]
struct TargetDefinition {
    /// The identifier text of the target symbol.
    name: String,
    /// The location where the target is defined.
    location: Location,
}

/// Finds all references to the identifier at the given position.
///
/// It first resolves the definition of the identifier at the specified
/// position, then searches through the appropriate scope of
/// documents to find all references to that definition.
pub fn find_all_references(
    graph: &DocumentGraph,
    document_uri: Url,
    position: SourcePosition,
    encoding: SourcePositionEncoding,
    include_declaration: bool,
) -> Result<Vec<Location>> {
    let definition_location = handlers::goto_definition(graph, document_uri, position, encoding)
        .context("failed to resolve symbol definition")?
        .ok_or_else(|| {
            anyhow!(
                "no definition location found for symbol at position: {}:{}",
                position.line,
                position.character
            )
        })?;

    let doc_index = graph
        .get_index(&definition_location.uri)
        .ok_or_else(|| anyhow!("definition document not in graph"))?;

    let node = graph.get(doc_index);
    let document = node
        .document()
        .ok_or_else(|| anyhow!("definition document not analyzed"))?;

    let lines = node
        .parse_state()
        .lines()
        .ok_or_else(|| anyhow!("missing line index for target"))?;

    let offset = position_to_offset(
        lines,
        SourcePosition::new(
            definition_location.range.start.line,
            definition_location.range.start.character,
        ),
        encoding,
    )
    .context("failed to convert position to offset")?;

    let token = document
        .root()
        .inner()
        .token_at_offset(offset)
        .find(|t| t.kind() == SyntaxKind::Ident)
        .ok_or_else(|| anyhow!("could not find target token at definition site"))?;

    let target = TargetDefinition {
        name: token.text().to_string(),
        location: definition_location.clone(),
    };

    // TODO: better search scope for performance.
    let search_scope: Vec<_> = graph.transitive_dependents(doc_index).collect();

    let mut locations = Vec::new();
    for doc_index in search_scope {
        collect_references_from_document(graph, doc_index, &target, encoding, &mut locations)
            .with_context(|| {
                format!("failed to collect references from document at index {doc_index:?}")
            })?;
    }

    if !include_declaration {
        locations.retain(|loc| *loc != target.location);
    }

    Ok(locations)
}

/// Collects references to the target symbol form a single document.
///
/// 1. Traverse all tokens in the document's CST
/// 2. Filter for identifier tokens matching the target name
/// 3. For each match, resolve its definition using goto definition
/// 4. If the resolved definition matches the target, add the reference location
fn collect_references_from_document(
    graph: &DocumentGraph,
    doc_index: petgraph::graph::NodeIndex,
    target: &TargetDefinition,
    encoding: SourcePositionEncoding,
    locations: &mut Vec<Location>,
) -> Result<()> {
    let node = graph.get(doc_index);
    let document = match node.document() {
        Some(doc) => doc,
        None => return Ok(()),
    };

    let lines = match node.parse_state().lines() {
        Some(lines) => lines,
        None => return Ok(()),
    };

    let root = document.root().inner().clone();

    for token in root
        .descendants_with_tokens()
        .filter_map(|el| el.into_token())
    {
        // In WDL, variable shadowing is not allowed.
        //
        // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#appendix-b-wdl-namespaces-and-scopes
        //
        // - All members of a namespace must be unique within that namespace.
        // - When the user makes a declaration within a nested scope, they are
        //   essentially reserving that name in all of the higher-level scopes so that
        //   it cannot be reused.
        //
        // This means name matching combined with definition resolution is safe and
        // won't produce false positives from shadowed variables.
        if token.kind() == SyntaxKind::Ident && token.text() == target.name {
            let token_pos = position(lines, token.text_range().start())
                .context("failed to convert token position")?;
            let source_pos = SourcePosition::new(token_pos.line, token_pos.character);

            let resolved_location = handlers::goto_definition(
                graph,
                document.uri().as_ref().clone(),
                source_pos,
                encoding,
            )
            .context("failed to resolve token definition")?;

            if let Some(location) = resolved_location
                && location == target.location
            {
                let reference_location = location_from_span(document.uri(), token.span(), lines)
                    .context("failed to create reference location")?;

                locations.push(reference_location);
            }
        }
    }
    Ok(())
}
