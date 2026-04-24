//! Formatting for imports.

use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::ImportStatement;
use wdl_ast::v1::SymbolicImportMember;
use wdl_ast::v1::SymbolicImportMembers;

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
/// Spaces are inserted around the `from` and `as` keywords. Other top-level
/// children (keyword, selection clause, source, aliases) delegate to their
/// own formatters.
pub fn format_import_statement(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("import statement children") {
        match child.element().kind() {
            SyntaxKind::FromKeyword | SyntaxKind::AsKeyword => {
                stream.end_word();
                (&child).write(stream, config);
                stream.end_word();
            }
            _ => {
                (&child).write(stream, config);
                stream.end_word();
            }
        }
    }

    stream.end_line();
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

    // Decide on inline vs multiline by measuring the canonical inline width
    // the formatter would emit, not the width of the source span. This makes
    // the decision independent of incoming whitespace.
    let overflows = config
        .max_line_length
        .get()
        .and_then(|max| canonical_import_width(element).map(|w| w > max))
        .unwrap_or(false);

    if overflows {
        format_symbolic_import_members_multiline(&children, stream, config);
    } else {
        format_symbolic_import_members_inline(&children, stream, config);
    }
}

/// Computes the canonical inline width of the enclosing `ImportStatement`
/// as this formatter would emit it, ignoring trivia in the source span.
///
/// Returns `None` if the members clause is not enclosed in a recognizable
/// import statement, in which case the caller falls back to the inline form.
fn canonical_import_width(element: &FormatElement) -> Option<usize> {
    let node = element.element().as_node()?.inner().clone();
    let members = SymbolicImportMembers::cast(node)?;
    let stmt = ImportStatement::cast(members.inner().parent()?)?;

    let mut width = "import ".len();

    let member_list: Vec<_> = members.members().collect();
    width += "{ ".len();
    for (i, m) in member_list.iter().enumerate() {
        if i > 0 {
            width += ", ".len();
        }
        width += symbolic_import_member_width(m);
    }
    width += " }".len();

    width += " from ".len();
    if let Some(uri) = stmt.uri() {
        let text = uri.text().map(|t| t.text().to_string()).unwrap_or_default();
        width += 2 + text.len();
    } else if let Some(path) = stmt.module_path() {
        width += path.text().len();
    }

    Some(width)
}

/// Computes the canonical inline width of a single selected-member entry.
fn symbolic_import_member_width(member: &SymbolicImportMember) -> usize {
    let mut width = 0usize;
    let mut first = true;
    for component in member.components() {
        if !first {
            width += ".".len();
        }
        width += component.text().len();
        first = false;
    }
    if let Some(alias) = member.alias() {
        width += " as ".len();
        width += alias.text().len();
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
