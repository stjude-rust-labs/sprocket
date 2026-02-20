//! Handlers for inlay hint requests.
//!
//! This module implements the LSP `textDocument/inlayHint` functionality for
//! WDL files.
//!
//! See: [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_inlayHint)

use anyhow::Result;
use anyhow::bail;
use ls_types::InlayHint;
use ls_types::InlayHintKind;
use ls_types::InlayHintLabel;
use ls_types::Position;
use ls_types::Range;
use ls_types::TextEdit;
use rowan::TextSize;
use url::Url;
use wdl_ast::AstToken;

use crate::graph::DocumentGraph;
use crate::graph::ParseState;
use crate::handlers::common::position;
use crate::types::CustomType;
use crate::types::Type;

/// Checks if a position is within a given range.
fn position_in_range(pos: &Position, range: &Range) -> bool {
    (pos.line > range.start.line
        || (pos.line == range.start.line && pos.character >= range.start.character))
        && (pos.line < range.end.line
            || (pos.line == range.end.line && pos.character <= range.end.character))
}

/// Handles an inlay hint request for a document.
///
/// Returns inlay hints for the following:
///
/// - Enum definitions where the inner type was inferred rather than explicitly
///   specified.
/// - Enum variants without explicit values, showing the inferred string value.
///
/// Only returns hints that fall within the specified range.
pub fn inlay_hints(
    graph: &DocumentGraph,
    uri: &Url,
    range: Range,
) -> Result<Option<Vec<InlayHint>>> {
    let Some(index) = graph.get_index(uri) else {
        bail!("document `{uri}` not found in graph.");
    };

    let node = graph.get(index);
    let lines = match node.parse_state() {
        ParseState::Parsed { lines, .. } => lines.clone(),
        _ => bail!("document `{uri}` has not been parsed", uri = uri),
    };

    let Some(document) = node.document() else {
        bail!("document analysis data not available for {}", uri);
    };

    let mut hints = Vec::new();

    // Find all enum definitions in the document
    for (_, enum_entry) in document.enums() {
        // Skip imported enums
        if enum_entry.namespace().is_some() {
            continue;
        }

        let definition = enum_entry.definition();

        // Calculate the enum name end position (where the type hint would appear)
        let name_span = definition.name().span();
        let absolute_end = enum_entry.offset() + name_span.end();
        let enum_name_end_pos = position(&lines, TextSize::try_from(absolute_end)?)?;

        // Skip if the enum name end is not within the requested range
        if !position_in_range(&enum_name_end_pos, &range) {
            continue;
        }

        // Check if the enum has an explicit type parameter
        if definition.type_parameter().is_none() {
            // Get the inferred type from the enum
            let Some(enum_type) = enum_entry.ty() else {
                continue;
            };

            let CustomType::Enum(enum_type) = enum_type.as_custom().unwrap() else {
                continue;
            };

            let inner_type = enum_type.inner_value_type();

            // Create an inlay hint showing the inferred type
            if !matches!(inner_type, Type::Union) {
                hints.push(InlayHint {
                    position: Position {
                        line: enum_name_end_pos.line,
                        character: enum_name_end_pos.character,
                    },
                    label: InlayHintLabel::String(format!("[{}]", inner_type)),
                    kind: Some(InlayHintKind::TYPE),
                    text_edits: Some(vec![TextEdit {
                        range: Range {
                            start: enum_name_end_pos,
                            end: enum_name_end_pos,
                        },
                        new_text: format!("[{}]", inner_type),
                    }]),
                    tooltip: Some(ls_types::InlayHintTooltip::String(
                        "Click to insert type parameter".to_string(),
                    )),
                    padding_left: None,
                    padding_right: None,
                    data: None,
                });
            }
        }

        // Add hints for variants without explicit values
        for variant in definition.variants() {
            // Skip variants that have an explicit value
            if variant.value().is_some() {
                continue;
            }

            let variant_name = variant.name().text().to_string();
            let variant_span = variant.name().span();
            let absolute_end = enum_entry.offset() + variant_span.end();
            let variant_end_pos = position(&lines, TextSize::try_from(absolute_end)?)?;

            // Skip if the variant position is not within the requested range
            if !position_in_range(&variant_end_pos, &range) {
                continue;
            }

            hints.push(InlayHint {
                position: Position {
                    line: variant_end_pos.line,
                    character: variant_end_pos.character,
                },
                label: InlayHintLabel::String(format!(" = \"{}\"", variant_name)),
                kind: Some(InlayHintKind::PARAMETER),
                text_edits: Some(vec![TextEdit {
                    range: Range {
                        start: variant_end_pos,
                        end: variant_end_pos,
                    },
                    new_text: format!(" = \"{}\"", variant_name),
                }]),
                tooltip: Some(ls_types::InlayHintTooltip::String(
                    "Click to insert variant value".to_string(),
                )),
                padding_left: None,
                padding_right: None,
                data: None,
            });
        }
    }

    Ok(Some(hints))
}
