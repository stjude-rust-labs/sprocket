//! Utilities for generating documentation for LSP handlers.

use std::fmt::Write;

use lsp_types::Documentation;
use lsp_types::MarkupContent;
use rowan::TextSize;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_ast::v1::EnumDefinition;
use wdl_ast::v1::MetadataSection;
use wdl_ast::v1::MetadataValue;
use wdl_ast::v1::StructDefinition;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::WorkflowDefinition;
use wdl_ast::v1::display::format_meta_value;
use wdl_ast::v1::display::get_param_meta;
use wdl_ast::v1::display::write_input_section;
use wdl_ast::v1::display::write_output_section;

use crate::document::Enum;
use crate::document::Struct;
use crate::document::Task;
use crate::document::Workflow;
use crate::types::CompoundType;
use crate::types::Type;

/// Makes a LSP documentation from a definition text.
pub fn make_md_docs(definition: String) -> Option<Documentation> {
    Some(Documentation::MarkupContent(MarkupContent {
        kind: lsp_types::MarkupKind::Markdown,
        value: definition,
    }))
}

/// Extracts doc comments (`##`) from the previous siblings of a syntax node.
///
/// Returns `None` if no doc comments are found. Doc comments are collected
/// by walking backwards through siblings, skipping whitespace, and
/// collecting consecutive `##` comment tokens. The `##` prefix (and one
/// optional leading space) is stripped from each line.
pub(crate) fn extract_doc_comment(node: &SyntaxNode) -> Option<String> {
    let mut lines = Vec::new();
    let mut current = node.prev_sibling_or_token();

    while let Some(sibling) = current {
        match sibling {
            rowan::NodeOrToken::Token(ref token) => {
                if token.kind() == SyntaxKind::Whitespace {
                    current = sibling.prev_sibling_or_token();
                    continue;
                }
                if let Some(comment) = Comment::cast(token.clone())
                    && comment.is_doc_comment()
                {
                    let text = comment.inner().text();
                    let stripped = text
                        .strip_prefix("## ")
                        .unwrap_or(text.strip_prefix("##").unwrap_or(text));
                    lines.push(stripped.to_string());
                    current = sibling.prev_sibling_or_token();
                    continue;
                }
                break;
            }
            rowan::NodeOrToken::Node(_) => break,
        }
    }

    if lines.is_empty() {
        None
    } else {
        lines.reverse();
        Some(lines.join("\n"))
    }
}

/// Reads the `description` key from a metadata section as a plain string.
fn read_meta_description(meta: Option<MetadataSection>) -> Option<String> {
    let meta = meta?;
    let desc = meta.items().find(|i| i.name().text() == "description")?;
    if let MetadataValue::String(s) = desc.value() {
        s.text().map(|t| t.text().to_string())
    } else {
        None
    }
}

/// Renders markdown documentation for a task definition.
///
/// Doc comments are preferred over `meta.description` for the description
/// paragraph. Input and output sections always use `parameter_meta`.
fn render_task_doc(n: &TaskDefinition, syntax: &SyntaxNode) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "```wdl\ntask {}\n```\n---", n.name().text());

    let description =
        extract_doc_comment(syntax).or_else(|| read_meta_description(n.metadata()));
    if let Some(desc) = description {
        let _ = writeln!(s, "{}\n", desc);
    }

    let _ = write_input_section(&mut s, n.input().as_ref(), n.parameter_metadata().as_ref());
    let _ = write_output_section(&mut s, n.output().as_ref(), n.parameter_metadata().as_ref());

    s
}

/// Renders markdown documentation for a workflow definition.
///
/// Doc comments are preferred over `meta.description` for the description
/// paragraph. Input and output sections always use `parameter_meta`.
fn render_workflow_doc(n: &WorkflowDefinition, syntax: &SyntaxNode) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "```wdl\nworkflow {}\n```\n---", n.name().text());

    let description =
        extract_doc_comment(syntax).or_else(|| read_meta_description(n.metadata()));
    if let Some(desc) = description {
        let _ = writeln!(s, "{}\n", desc);
    }

    let _ = write_input_section(&mut s, n.input().as_ref(), n.parameter_metadata().as_ref());
    let _ = write_output_section(&mut s, n.output().as_ref(), n.parameter_metadata().as_ref());

    s
}

/// Renders markdown documentation for a struct definition.
///
/// Doc comments are preferred over `meta.description` for the description
/// paragraph. Members always use `parameter_meta`.
fn render_struct_doc(n: &StructDefinition, syntax: &SyntaxNode) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "```wdl");
    let _ = writeln!(s, "{n}");
    let _ = writeln!(s, "```\n---");

    let description =
        extract_doc_comment(syntax).or_else(|| read_meta_description(n.metadata().next()));
    if let Some(desc) = description {
        let _ = writeln!(s, "{}\n", desc);
    }

    let members: Vec<_> = n.members().collect();
    if !members.is_empty() {
        let _ = writeln!(s, "\n**Members**");
        for member in members {
            let name = member.name();
            let _ = write!(s, "- **{}**: `{}`", name.text(), member.ty().inner().text());
            if let Some(meta_val) =
                get_param_meta(name.text(), n.parameter_metadata().next().as_ref())
            {
                let _ = writeln!(s);
                let _ = format_meta_value(&mut s, &meta_val, 2);
            }
            let _ = writeln!(s);
        }
    }

    s
}

/// Renders markdown documentation for an enum definition.
///
/// Doc comments are used for the description paragraph if present.
fn render_enum_doc(
    n: &EnumDefinition,
    syntax: &SyntaxNode,
    computed_type: Option<&str>,
) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "```wdl");
    let _ = write!(s, "{}", n.display(computed_type));
    let _ = write!(s, "```");

    if let Some(desc) = extract_doc_comment(syntax) {
        let _ = write!(s, "\n\n---\n{}\n", desc);
    }

    s
}

/// Provides documentation for tasks which includes `inputs`, `outputs`,
/// `metadata`, `runtime`
pub fn provide_task_documentation(task: &Task, root: &wdl_ast::Document) -> Option<String> {
    match TextSize::try_from(task.name_span().start()) {
        Ok(offset) => root
            .inner()
            .token_at_offset(offset)
            .left_biased()
            .and_then(|t| t.parent_ancestors().find_map(TaskDefinition::cast))
            .map(|n| render_task_doc(&n, n.inner())),
        Err(_) => None,
    }
}

/// Provides documentation for workflows which includes `inputs`, `outputs`,
/// `metadata`
pub fn provide_workflow_documentation(
    workflow: &Workflow,
    root: &wdl_ast::Document,
) -> Option<String> {
    match TextSize::try_from(workflow.name_span().start()) {
        Ok(offset) => root
            .inner()
            .token_at_offset(offset)
            .left_biased()
            .and_then(|t| t.parent_ancestors().find_map(WorkflowDefinition::cast))
            .map(|n| render_workflow_doc(&n, n.inner())),
        Err(_) => None,
    }
}

/// Provides documentation for structs.
pub fn provide_struct_documentation(
    struct_info: &Struct,
    root: &wdl_ast::Document,
) -> Option<String> {
    match TextSize::try_from(struct_info.name_span().start()) {
        Ok(offset) => root
            .inner()
            .token_at_offset(offset)
            .left_biased()
            .and_then(|t| t.parent_ancestors().find_map(StructDefinition::cast))
            .map(|n| render_struct_doc(&n, n.inner())),
        Err(_) => None,
    }
}

/// Provides documentation for enums.
pub fn provide_enum_documentation(enum_info: &Enum, root: &wdl_ast::Document) -> Option<String> {
    match TextSize::try_from(enum_info.name_span().start()) {
        Ok(offset) => root
            .inner()
            .token_at_offset(offset)
            .left_biased()
            .and_then(|t| t.parent_ancestors().find_map(EnumDefinition::cast))
            .map(|n| {
                let computed_type = enum_info.ty().and_then(|ty| {
                    if let Type::Compound(CompoundType::Custom(custom_ty), _) = ty {
                        custom_ty
                            .as_enum()
                            .map(|enum_ty| enum_ty.inner_value_type().to_string())
                    } else {
                        None
                    }
                });
                render_enum_doc(&n, n.inner(), computed_type.as_deref())
            }),
        Err(_) => None,
    }
}
