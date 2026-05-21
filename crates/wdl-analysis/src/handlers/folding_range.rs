//! Handlers for folding range requests.
//!
//! This module implements the LSP `textDocument/foldingRange` functionality for
//! WDL files.
//!
//! See: [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_foldingRange)

use std::collections::HashSet;

use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use lsp_types::FoldingRange;
use lsp_types::FoldingRangeKind;
use rowan::Direction;
use rowan::TextRange;
use url::Url;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::HasBlock;
use wdl_ast::Whitespace;
use wdl_ast::v1::CommandSection;
use wdl_ast::v1::InputSection;
use wdl_ast::v1::MetadataSection;
use wdl_ast::v1::OutputSection;
use wdl_ast::v1::ParameterMetadataSection;
use wdl_ast::v1::Placeholder;
use wdl_ast::v1::RequirementsSection;
use wdl_ast::v1::RuntimeSection;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::TaskHintsSection;
use wdl_ast::v1::WorkflowDefinition;
use wdl_ast::v1::WorkflowHintsSection;
use wdl_grammar::SyntaxElement;
use wdl_grammar::SyntaxKind;
use wdl_grammar::SyntaxNode;

use crate::graph::DocumentGraph;
use crate::graph::ParseState;
use crate::handlers::common::position;

/// Context for folding range operations.
#[derive(Default)]
struct FoldingContext {
    /// Elements we've already handled. Used for multiline ranges (e.g.,
    /// comments and imports).
    visited_elements: HashSet<SyntaxElement>,
}

/// Determines all folding ranges in the document.
pub fn folding_range(graph: &DocumentGraph, document_uri: Url) -> Result<Vec<FoldingRange>> {
    let index = graph
        .get_index(&document_uri)
        .ok_or_else(|| anyhow!("document `{uri}` not found in graph", uri = document_uri))?;

    let node = graph.get(index);
    let (root, lines) = match node.parse_state() {
        ParseState::Parsed { lines, root, .. } => {
            (SyntaxNode::new_root(root.clone()), lines.clone())
        }
        _ => bail!("document `{uri}` has not been parsed", uri = document_uri),
    };

    let mut ctx = FoldingContext::default();
    let mut ranges = Vec::new();
    for element in root.descendants_with_tokens() {
        let Some((range, kind)) = determine_folding_range(&mut ctx, element) else {
            continue;
        };

        let start_pos = position(&lines, range.start())?;
        let end_pos = position(&lines, range.end())?;

        ranges.push(FoldingRange {
            start_line: start_pos.line,
            start_character: Some(start_pos.character),
            end_line: end_pos.line,
            end_character: Some(end_pos.character),
            kind,
            collapsed_text: None,
        });
    }

    Ok(ranges)
}

/// Calculates the folding range for the given `element`.
fn determine_folding_range(
    ctx: &mut FoldingContext,
    element: SyntaxElement,
) -> Option<(TextRange, Option<FoldingRangeKind>)> {
    if ctx.visited_elements.contains(&element) {
        return None;
    }

    let mut range = element.text_range();
    let mut folding_kind = None;
    match element {
        SyntaxElement::Token(token) => {
            let comment = Comment::cast(token.clone())?;
            ctx.visited_elements.insert(SyntaxElement::Token(token));

            folding_kind = Some(FoldingRangeKind::Comment);
            let expected_kind = comment.kind();

            for sibling in comment.inner().siblings_with_tokens(Direction::Next) {
                let SyntaxElement::Token(token) = sibling else {
                    break;
                };

                if let Some(whitespace) = Whitespace::cast(token.clone()) {
                    // Ignore multiple newlines
                    //
                    // For example:
                    //
                    // # Foo
                    // # Bar
                    //
                    // # Baz
                    // # Qux
                    //
                    // Would be two comment groups
                    if whitespace.text().lines().count() > 1 {
                        break;
                    }

                    continue;
                }

                let Some(sibling_comment) = Comment::cast(token) else {
                    break;
                };

                // Don't group together comments of different kinds
                if sibling_comment.kind() != expected_kind {
                    break;
                }

                range = TextRange::new(range.start(), sibling_comment.inner().text_range().end());
                ctx.visited_elements
                    .insert(SyntaxElement::Token(sibling_comment.inner().clone()));
            }
        }
        SyntaxElement::Node(node) => match node.kind() {
            SyntaxKind::ImportStatementNode => {
                range = collect_contiguous_elements_of_type(ctx, node);
                folding_kind = Some(FoldingRangeKind::Imports);
            }
            SyntaxKind::TaskDefinitionNode => {
                let task = TaskDefinition::cast(node).expect("should cast");
                range = task.block_range();
            }
            SyntaxKind::WorkflowDefinitionNode => {
                let workflow = WorkflowDefinition::cast(node).expect("should cast");
                range = workflow.block_range();
            }
            SyntaxKind::MetadataSectionNode => {
                let meta = MetadataSection::cast(node).expect("should cast");
                range = meta.block_range();
            }
            SyntaxKind::ParameterMetadataSectionNode => {
                let meta = ParameterMetadataSection::cast(node).expect("should cast");
                range = meta.block_range();
            }
            SyntaxKind::InputSectionNode => {
                let input = InputSection::cast(node).expect("should cast");
                range = input.block_range();
            }
            SyntaxKind::OutputSectionNode => {
                let output = OutputSection::cast(node).expect("should cast");
                range = output.block_range();
            }
            SyntaxKind::CommandSectionNode => {
                let command = CommandSection::cast(node).expect("should cast");
                range = command.block_range();
            }
            SyntaxKind::RequirementsSectionNode => {
                let requirements = RequirementsSection::cast(node).expect("should cast");
                range = requirements.block_range();
            }
            SyntaxKind::TaskHintsSectionNode => {
                let hints = TaskHintsSection::cast(node).expect("should cast");
                range = hints.block_range();
            }
            SyntaxKind::WorkflowHintsSectionNode => {
                let hints = WorkflowHintsSection::cast(node).expect("should cast");
                range = hints.block_range();
            }
            SyntaxKind::RuntimeSectionNode => {
                let runtime = RuntimeSection::cast(node).expect("should cast");
                range = runtime.block_range();
            }
            SyntaxKind::PlaceholderNode => {
                let placeholder = Placeholder::cast(node).expect("should cast");
                range = placeholder.expr().inner().text_range();
            }

            _ => return None,
        },
    }

    Some((range, folding_kind))
}

/// Find all nodes of the same type that are separated by at most one newline.
fn collect_contiguous_elements_of_type(ctx: &mut FoldingContext, first: SyntaxNode) -> TextRange {
    let mut range = first.text_range();
    for sibling in first.siblings_with_tokens(Direction::Next) {
        match sibling {
            SyntaxElement::Token(token) => {
                if let Some(whitespace) = Whitespace::cast(token) {
                    if whitespace.text().lines().count() > 1 {
                        break;
                    }

                    continue;
                }

                break;
            }
            SyntaxElement::Node(node) => {
                if node.kind() == first.kind() {
                    range = TextRange::new(range.start(), node.text_range().end());
                    ctx.visited_elements.insert(SyntaxElement::Node(node));
                    continue;
                }

                break;
            }
        }
    }

    range
}
