//! Utilities for generating documentation for LSP handlers.

use std::fmt::Write;

use lsp_types::Documentation;
use lsp_types::MarkupContent;
use rowan::TextSize;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::Documented;
use wdl_ast::v1::Decl;
use wdl_ast::v1::EnumDefinition;
use wdl_ast::v1::InputSection;
use wdl_ast::v1::MetadataSection;
use wdl_ast::v1::MetadataValue;
use wdl_ast::v1::OutputSection;
use wdl_ast::v1::ParameterMetadataSection;
use wdl_ast::v1::StructDefinition;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::WorkflowDefinition;
use wdl_ast::v1::format_meta_value;
use wdl_ast::v1::get_param_meta;

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

/// Converts a list of doc comments to a Markdown string, stripping the `## `
/// prefix.
///
/// Returns `None` if the list is empty. All paragraphs are included.
pub(crate) fn comments_to_string(comments: Vec<Comment>) -> Option<String> {
    if comments.is_empty() {
        return None;
    }
    let lines: Vec<String> = comments
        .iter()
        .map(|c| {
            let text = c.inner().text();
            text.strip_prefix("## ")
                .or_else(|| text.strip_prefix("##"))
                .unwrap_or(text)
                .to_string()
        })
        .collect();
    Some(lines.join("\n"))
}

/// Returns the first paragraph of doc comments as a Markdown string, stripping
/// the `## ` prefix.
///
/// A paragraph ends at the first empty `##` comment (i.e., `##` with no
/// following text). Returns `None` if the list is empty.
fn first_paragraph_doc(comments: Vec<Comment>) -> Option<String> {
    if comments.is_empty() {
        return None;
    }
    let mut lines: Vec<String> = Vec::new();
    for c in comments {
        let text = c.inner().text();
        let stripped = text
            .strip_prefix("## ")
            .or_else(|| text.strip_prefix("##"))
            .unwrap_or(text);
        if stripped.is_empty() {
            break;
        }
        lines.push(stripped.to_string());
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

/// Formats the input section with doc comments preferred over parameter
/// metadata for each declaration's description.
fn write_documented_inputs(
    f: &mut impl Write,
    input: Option<&InputSection>,
    param_meta: Option<&ParameterMetadataSection>,
) -> std::fmt::Result {
    let Some(input) = input else {
        return Ok(());
    };
    if input.declarations().next().is_none() {
        return Ok(());
    }
    writeln!(f, "\n**Inputs**")?;
    for decl in input.declarations() {
        let name_text = decl.name().text().to_string();
        let ty_text = decl.ty().inner().text().to_string();
        let default_val = decl.expr().map(|e| e.text().to_string());
        let doc = match &decl {
            Decl::Unbound(u) => first_paragraph_doc(u.doc_comments().unwrap_or_default()),
            Decl::Bound(b) => first_paragraph_doc(b.doc_comments().unwrap_or_default()),
        };
        write!(f, "- **{}**: `{}`", name_text, ty_text)?;
        if let Some(val) = &default_val {
            write!(f, " = *`{}`*", val.trim_start_matches(" = "))?;
        }
        if let Some(doc_str) = doc {
            writeln!(f)?;
            writeln!(f, "  {doc_str}")?;
        } else if let Some(meta_val) = get_param_meta(&name_text, param_meta) {
            writeln!(f)?;
            format_meta_value(f, &meta_val, 2)?;
            writeln!(f)?;
        } else {
            writeln!(f)?;
        }
    }
    Ok(())
}

/// Formats the output section with doc comments preferred over parameter
/// metadata for each declaration's description.
fn write_documented_outputs(
    f: &mut impl Write,
    output: Option<&OutputSection>,
    param_meta: Option<&ParameterMetadataSection>,
) -> std::fmt::Result {
    let Some(output) = output else {
        return Ok(());
    };
    if output.declarations().next().is_none() {
        return Ok(());
    }
    writeln!(f, "\n**Outputs**")?;
    for decl in output.declarations() {
        let name_text = decl.name().text().to_string();
        let ty_text = decl.ty().inner().text().to_string();
        let doc = first_paragraph_doc(decl.doc_comments().unwrap_or_default());
        write!(f, "- **{}**: `{}`", name_text, ty_text)?;
        if let Some(doc_str) = doc {
            writeln!(f)?;
            writeln!(f, "  {doc_str}")?;
        } else if let Some(meta_val) = get_param_meta(&name_text, param_meta) {
            writeln!(f)?;
            format_meta_value(f, &meta_val, 2)?;
            writeln!(f)?;
        } else {
            writeln!(f)?;
        }
    }
    Ok(())
}

/// Shared rendering logic for task and workflow definitions.
///
/// Renders the code fence header, optional description paragraph, and input /
/// output sections.
fn render_runnable_doc(
    keyword: &str,
    name: &str,
    doc: Option<String>,
    input: Option<&InputSection>,
    output: Option<&OutputSection>,
    param_meta: Option<&ParameterMetadataSection>,
) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "```wdl\n{keyword} {name}\n```\n---");
    if let Some(desc) = doc {
        let _ = writeln!(s, "{desc}\n");
    }
    let _ = write_documented_inputs(&mut s, input, param_meta);
    let _ = write_documented_outputs(&mut s, output, param_meta);
    s
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
/// paragraph. Inputs and outputs prefer doc comments over `parameter_meta`.
fn render_task_doc(n: &TaskDefinition) -> String {
    let doc = comments_to_string(n.doc_comments().unwrap_or_default())
        .or_else(|| read_meta_description(n.metadata()));
    render_runnable_doc(
        "task",
        n.name().text(),
        doc,
        n.input().as_ref(),
        n.output().as_ref(),
        n.parameter_metadata().as_ref(),
    )
}

/// Renders markdown documentation for a workflow definition.
///
/// Doc comments are preferred over `meta.description` for the description
/// paragraph. Inputs and outputs prefer doc comments over `parameter_meta`.
fn render_workflow_doc(n: &WorkflowDefinition) -> String {
    let doc = comments_to_string(n.doc_comments().unwrap_or_default())
        .or_else(|| read_meta_description(n.metadata()));
    render_runnable_doc(
        "workflow",
        n.name().text(),
        doc,
        n.input().as_ref(),
        n.output().as_ref(),
        n.parameter_metadata().as_ref(),
    )
}

/// Renders markdown documentation for a struct definition.
///
/// Doc comments are preferred over `meta.description` for the description
/// paragraph. Members prefer doc comments (first paragraph) over
/// `parameter_meta`.
fn render_struct_doc(n: &StructDefinition) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "```wdl");
    let _ = writeln!(s, "{n}");
    let _ = writeln!(s, "```\n---");

    let description = comments_to_string(n.doc_comments().unwrap_or_default())
        .or_else(|| read_meta_description(n.metadata().next()));
    if let Some(desc) = description {
        let _ = writeln!(s, "{desc}\n");
    }

    let members: Vec<_> = n.members().collect();
    if !members.is_empty() {
        let _ = writeln!(s, "\n**Members**");
        for member in members {
            let name = member.name();
            let _ = write!(s, "- **{}**: `{}`", name.text(), member.ty().inner().text());
            let doc =
                first_paragraph_doc(member.doc_comments().unwrap_or_default()).or_else(|| {
                    get_param_meta(name.text(), n.parameter_metadata().next().as_ref()).map(
                        |meta_val| {
                            let mut buf = String::new();
                            let _ = format_meta_value(&mut buf, &meta_val, 2);
                            buf
                        },
                    )
                });
            if let Some(d) = doc {
                let _ = writeln!(s);
                let _ = write!(s, "{d}");
            }
            let _ = writeln!(s);
        }
    }

    s
}

/// Renders markdown documentation for an enum definition.
///
/// Doc comments are used for the description paragraph if present.
fn render_enum_doc(n: &EnumDefinition, computed_type: Option<&str>) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "```wdl");
    let _ = write!(s, "{}", n.display(computed_type));
    let _ = write!(s, "```");
    if let Some(desc) = comments_to_string(n.doc_comments().unwrap_or_default()) {
        let _ = write!(s, "\n\n---\n{desc}\n");
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
            .map(|n| render_task_doc(&n)),
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
            .map(|n| render_workflow_doc(&n)),
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
            .map(|n| render_struct_doc(&n)),
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
                render_enum_doc(&n, computed_type.as_deref())
            }),
        Err(_) => None,
    }
}
