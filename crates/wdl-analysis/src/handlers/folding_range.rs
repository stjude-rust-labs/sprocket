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
use wdl_ast::Whitespace;
use wdl_ast::v1::CloseBrace;
use wdl_ast::v1::CommandSection;
use wdl_ast::v1::InputSection;
use wdl_ast::v1::MetadataSection;
use wdl_ast::v1::OutputSection;
use wdl_ast::v1::ParameterMetadataSection;
use wdl_ast::v1::Placeholder;
use wdl_ast::v1::PlaceholderOpen;
use wdl_ast::v1::RequirementsSection;
use wdl_ast::v1::RuntimeSection;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::TaskHintsSection;
use wdl_ast::v1::WorkflowDefinition;
use wdl_ast::v1::WorkflowHintsSection;
use wdl_grammar::Span;
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
        let Some((range, kind, collapsed_text)) = determine_folding_range(&mut ctx, element) else {
            continue;
        };

        let start_pos = position(&lines, range.start())?;
        let end_pos = position(&lines, range.end())?;

        // Single line ranges don't make sense
        if start_pos.line == end_pos.line {
            continue;
        }

        ranges.push(FoldingRange {
            start_line: start_pos.line,
            start_character: Some(start_pos.character),
            end_line: end_pos.line,
            end_character: Some(end_pos.character),
            kind,
            collapsed_text: collapsed_text.map(Into::into),
        });
    }

    Ok(ranges)
}

/// The [`FoldingRange`] collapsed text for braced blocks.
pub const BRACED_COLLAPSED_TEXT: &str = "{...}";
/// The [`FoldingRange`] collapsed text for heredoc command blocks.
pub const HEREDOC_COLLAPSED_TEXT: &str = "<<<...>>>";
/// The [`FoldingRange`] collapsed text for tilde placeholder blocks.
pub const TILDE_PLACEHOLDER_COLLAPSED_TEXT: &str = "~{...}";
/// The [`FoldingRange`] collapsed text for dollar placeholder blocks.
pub const DOLLAR_PLACEHOLDER_COLLAPSED_TEXT: &str = "${...}";

/// Calculates the folding range for the given `element`.
fn determine_folding_range(
    ctx: &mut FoldingContext,
    element: SyntaxElement,
) -> Option<(TextRange, Option<FoldingRangeKind>, Option<&'static str>)> {
    if ctx.visited_elements.contains(&element) {
        return None;
    }

    let mut range = element.text_range();
    let mut folding_kind = None;
    let mut collapsed_text = None;
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
                    if whitespace.text().chars().filter(|&c| c == '\n').count() > 1 {
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
        SyntaxElement::Node(node) => {
            if node.kind() == SyntaxKind::ImportStatementNode {
                range = collect_contiguous_elements_of_type(ctx, node);
                folding_kind = Some(FoldingRangeKind::Imports);
            } else {
                collapsed_text = Some(BRACED_COLLAPSED_TEXT);
                let scope_span = match node.kind() {
                    SyntaxKind::TaskDefinitionNode => {
                        let task = TaskDefinition::cast(node).expect("should cast");
                        task.braced_scope_span(true)?
                    }
                    SyntaxKind::WorkflowDefinitionNode => {
                        let workflow = WorkflowDefinition::cast(node).expect("should cast");
                        workflow.braced_scope_span(true)?
                    }
                    SyntaxKind::MetadataSectionNode => {
                        let meta = MetadataSection::cast(node).expect("should cast");
                        meta.braced_scope_span(true)?
                    }
                    SyntaxKind::ParameterMetadataSectionNode => {
                        let meta = ParameterMetadataSection::cast(node).expect("should cast");
                        meta.braced_scope_span(true)?
                    }
                    SyntaxKind::InputSectionNode => {
                        let input = InputSection::cast(node).expect("should cast");
                        input.braced_scope_span(true)?
                    }
                    SyntaxKind::OutputSectionNode => {
                        let output = OutputSection::cast(node).expect("should cast");
                        output.braced_scope_span(true)?
                    }
                    SyntaxKind::CommandSectionNode => {
                        let command = CommandSection::cast(node).expect("should cast");
                        if command.is_heredoc() {
                            collapsed_text = Some(HEREDOC_COLLAPSED_TEXT);
                            command.heredoc_scope_span(true)?
                        } else {
                            command.braced_scope_span(true)?
                        }
                    }
                    SyntaxKind::RequirementsSectionNode => {
                        let requirements = RequirementsSection::cast(node).expect("should cast");
                        requirements.braced_scope_span(true)?
                    }
                    SyntaxKind::TaskHintsSectionNode => {
                        let hints = TaskHintsSection::cast(node).expect("should cast");
                        hints.braced_scope_span(true)?
                    }
                    SyntaxKind::WorkflowHintsSectionNode => {
                        let hints = WorkflowHintsSection::cast(node).expect("should cast");
                        hints.braced_scope_span(true)?
                    }
                    SyntaxKind::RuntimeSectionNode => {
                        let runtime = RuntimeSection::cast(node).expect("should cast");
                        runtime.braced_scope_span(true)?
                    }
                    SyntaxKind::PlaceholderNode => {
                        let placeholder = Placeholder::cast(node).expect("should cast");
                        if placeholder.has_tilde() {
                            collapsed_text = Some(TILDE_PLACEHOLDER_COLLAPSED_TEXT);
                        } else {
                            collapsed_text = Some(DOLLAR_PLACEHOLDER_COLLAPSED_TEXT);
                        }

                        let open = placeholder.token::<PlaceholderOpen>()?;
                        let close = placeholder.last_token::<CloseBrace>()?;
                        Span::new(
                            open.span().start(),
                            close.span().end() - open.span().start(),
                        )
                    }
                    _ => return None,
                };

                range = scope_span.try_into().ok()?;
            }
        }
    }

    Some((range, folding_kind, collapsed_text))
}

/// Find all nodes of the same type that are separated by at most one newline.
fn collect_contiguous_elements_of_type(ctx: &mut FoldingContext, first: SyntaxNode) -> TextRange {
    let mut range = first.text_range();
    for sibling in first.siblings_with_tokens(Direction::Next) {
        match sibling {
            SyntaxElement::Token(token) => {
                if let Some(whitespace) = Whitespace::cast(token) {
                    if whitespace.text().chars().filter(|&c| c == '\n').count() > 1 {
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
