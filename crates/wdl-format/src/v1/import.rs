//! Formatting for imports.

use wdl_ast::SyntaxKind;

use crate::Config;
use crate::PreToken;
use crate::TokenStream;
use crate::Writable as _;
use crate::element::FormatElement;

/// Formats an [`ImportAlias`](wdl_ast::v1::ImportAlias).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_import_alias(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("import alias children") {
        (&child).write(stream, config);
        stream.end_word();
    }
}

/// Formats an [`ImportStatement`](wdl_ast::v1::ImportStatement).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_import_statement(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("import statement children") {
        (&child).write(stream, config);
        stream.end_word();
    }

    stream.end_line();
}

/// Formats a [`QuotedImport`](wdl_ast::v1::QuotedImport).
pub fn format_quoted_import(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("quoted import children") {
        (&child).write(stream, config);
        stream.end_word();
    }
}

/// Formats a [`SymbolicImport`](wdl_ast::v1::SymbolicImport).
///
/// Spaces are inserted around the `from` and `as` keywords. The module path
/// and the selected-members clause delegate to their own formatters.
pub fn format_symbolic_import(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("symbolic import children") {
        match child.element().kind() {
            SyntaxKind::FromKeyword | SyntaxKind::AsKeyword => {
                stream.end_word();
                (&child).write(stream, config);
                stream.end_word();
            }
            _ => {
                (&child).write(stream, config);
            }
        }
    }
}

/// Formats a [`SymbolicImportMembers`](wdl_ast::v1::SymbolicImportMembers).
///
/// Short lists render inline as `{ a, b, c }`. Lists whose inline width would
/// exceed the configured `max_line_length` render multiline, with each member
/// on its own line indented one level deeper than the surrounding statement.
/// A trailing comma before the closing brace is dropped in the inline form
/// and added in the multiline form.
pub fn format_symbolic_import_members(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    let children: Vec<_> = element
        .children()
        .expect("symbolic import members children")
        .collect();

    // Measure the inline width of the whole surrounding `ImportStatement`
    // so the members clause only breaks when the full statement overflows.
    let parent_width: usize = element
        .element()
        .as_node()
        .and_then(|n| n.inner().parent())
        .and_then(|p| p.parent())
        .map(|stmt| stmt.text_range().len().into())
        .unwrap_or_else(|| estimate_inline_width(&children));

    let overflows = config
        .max_line_length
        .get()
        .is_some_and(|max| parent_width > max);

    if overflows {
        format_symbolic_import_members_multiline(&children, stream, config);
    } else {
        format_symbolic_import_members_inline(&children, stream, config);
    }
}

/// Sums the rendered widths of every non-trivia child plus inter-token spacing
/// for an inline `{ a, b, c }` rendering.
fn estimate_inline_width(children: &[&FormatElement]) -> usize {
    let mut width = 0usize;
    let mut last_was_comma = false;
    for (i, child) in children.iter().enumerate() {
        let kind = child.element().kind();
        if kind.is_trivia() {
            continue;
        }
        let text_len: usize = child.element().inner().text_range().len().into();
        match kind {
            SyntaxKind::OpenBrace => {
                width += text_len + 1;
            }
            SyntaxKind::CloseBrace => {
                width += 1 + text_len;
            }
            SyntaxKind::Comma => {
                let next_is_close_brace = children
                    .iter()
                    .skip(i + 1)
                    .find(|c| !c.element().kind().is_trivia())
                    .map(|c| c.element().kind())
                    == Some(SyntaxKind::CloseBrace);
                if !next_is_close_brace {
                    width += text_len;
                    last_was_comma = true;
                }
                continue;
            }
            _ => {
                if last_was_comma {
                    width += 1;
                }
                width += text_len;
            }
        }
        last_was_comma = false;
    }
    width
}

/// Emits the inline `{ a, b, c }` form.
fn format_symbolic_import_members_inline(
    children: &[&FormatElement],
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for (i, child) in children.iter().enumerate() {
        let kind = child.element().kind();
        match kind {
            SyntaxKind::OpenBrace => {
                child.write(stream, config);
                stream.end_word();
            }
            SyntaxKind::CloseBrace => {
                stream.end_word();
                child.write(stream, config);
            }
            SyntaxKind::Comma => {
                let next_kind = children
                    .iter()
                    .skip(i + 1)
                    .find(|c| !c.element().kind().is_trivia())
                    .map(|c| c.element().kind());
                if next_kind == Some(SyntaxKind::CloseBrace) {
                    continue;
                }
                child.write(stream, config);
                stream.end_word();
            }
            _ => {
                child.write(stream, config);
            }
        }
    }
}

/// Emits the multiline form, with each member on its own indented line.
fn format_symbolic_import_members_multiline(
    children: &[&FormatElement],
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for (i, child) in children.iter().enumerate() {
        let kind = child.element().kind();
        match kind {
            SyntaxKind::OpenBrace => {
                child.write(stream, config);
                stream.increment_indent();
            }
            SyntaxKind::CloseBrace => {
                stream.decrement_indent();
                child.write(stream, config);
            }
            SyntaxKind::Comma => {
                let next_kind = children
                    .iter()
                    .skip(i + 1)
                    .find(|c| !c.element().kind().is_trivia())
                    .map(|c| c.element().kind());
                child.write(stream, config);
                if next_kind != Some(SyntaxKind::CloseBrace) {
                    stream.end_line();
                }
            }
            k if k.is_trivia() => {}
            _ => {
                child.write(stream, config);
                let next_kind = children
                    .iter()
                    .skip(i + 1)
                    .find(|c| !c.element().kind().is_trivia())
                    .map(|c| c.element().kind());
                if next_kind == Some(SyntaxKind::CloseBrace) {
                    // Emit a trailing comma for the last member in the
                    // multiline form.
                    stream.push_literal(",".to_string(), SyntaxKind::Comma);
                    stream.end_line();
                }
            }
        }
    }
}

/// Formats a [`SymbolicImportMember`](wdl_ast::v1::SymbolicImportMember).
///
/// A space surrounds the optional `as` keyword. The `.` in a
/// namespace-qualified member is written tight.
pub fn format_symbolic_import_member(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("symbolic import member children") {
        match child.element().kind() {
            SyntaxKind::AsKeyword => {
                stream.end_word();
                (&child).write(stream, config);
                stream.end_word();
            }
            _ => {
                (&child).write(stream, config);
            }
        }
    }
}

/// Formats a [`SymbolicModulePath`](wdl_ast::v1::SymbolicModulePath).
///
/// The path is emitted as a single literal so the post-processor's line-break
/// algorithm never breaks it at the `/` separators.
pub fn format_symbolic_module_path(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    _config: &Config,
) {
    let text = element
        .element()
        .inner()
        .as_node()
        .expect("symbolic module path should be a node")
        .text()
        .to_string();
    stream.push_literal(text, SyntaxKind::Ident);
}
