//! Formatting for imports.

use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::ImportMember;
use wdl_ast::v1::ImportMembers;
use wdl_ast::v1::ImportSource;
use wdl_ast::v1::ImportStatement;

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

/// Formats an [`ImportStatement`].
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

/// Formats a [`ImportMembers`].
///
/// Short lists render inline as `{ a, b, c }`. Lists whose inline width would
/// exceed the configured `max_line_length` render multiline, with each member
/// on its own line indented one level deeper than the surrounding statement.
/// If a comment is detected anywhere within the [`ImportMembers`], the
/// multiline format will be used.
pub fn format_import_members(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    let overflows = config
        .max_line_length
        .get()
        .map(|max| canonical_import_width(element) > max)
        .unwrap_or(false);

    if overflows || element.has_comment() {
        format_import_members_multiline(element, stream, config);
    } else {
        format_import_members_inline(element, stream, config);
    }
}

/// Computes the canonical inline width of the enclosing `ImportStatement`
/// as this formatter would emit it, ignoring trivia in the source span.
fn canonical_import_width(element: &FormatElement) -> usize {
    let node = element
        .element()
        .as_node()
        .expect("`ImportMembers` element should be a node")
        .inner()
        .clone();
    let members = ImportMembers::cast(node).expect("element should cast to `ImportMembers`");
    let stmt = ImportStatement::cast(
        members
            .inner()
            .parent()
            .expect("`ImportMembers` should have a parent"),
    )
    .expect("parent should cast to `ImportStatement`");

    let mut width = "import ".len();

    let member_list: Vec<_> = members.members().collect();
    width += "{ ".len();
    for (i, m) in member_list.iter().enumerate() {
        if i > 0 {
            width += ", ".len();
        }
        width += import_member_width(m);
    }
    width += " }".len();

    width += " from ".len();
    match stmt.source() {
        ImportSource::Uri(uri) => {
            let text = uri.text().map(|t| t.text().to_string()).unwrap_or_default();
            // NOTE: the `+ 2` accounts for the surrounding quote characters
            // around the URI text in the rendered output.
            width += 2 + text.len();
        }
        ImportSource::ModulePath(path) => {
            width += path.text().len();
        }
    }

    width
}

/// Computes the canonical inline width of a single selected-member entry.
fn import_member_width(member: &ImportMember) -> usize {
    let mut width = member.name().text().len();
    if let Some(alias) = member.alias() {
        width += " as ".len();
        width += alias.text().len();
    }
    width
}

/// Emits the inline `{ a, b, c }` form.
fn format_import_members_inline(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    let mut children = element.children().expect("import members children");

    let open_brace = children.next().expect("import member open brace");
    assert!(open_brace.element().kind() == SyntaxKind::OpenBrace);
    (open_brace).write(stream, config);

    let mut items = Vec::new();
    let mut commas = Vec::new();
    let mut close_brace = None;

    for child in children {
        match child.element().kind() {
            SyntaxKind::CloseBrace => {
                close_brace = Some(child.to_owned());
            }
            SyntaxKind::Comma => {
                commas.push(child.to_owned());
            }
            _ => {
                items.push(child.to_owned());
            }
        }
    }

    if !items.is_empty() {
        stream.end_word();
    }
    let mut items = items.iter().peekable();
    let mut commas = commas.iter();
    while let Some(item) = items.next() {
        (&item).write(stream, config);
        if let Some(comma) = commas.next() {
            // check if this comma can be dropped. Comma can be dropped iff this is the last
            // item and the comma does not have a comment.
            if items.peek().is_some() || comma.has_comment() {
                (comma).write(stream, config);
            }
        }
        stream.end_word();
    }

    (&close_brace.expect("import members close brace")).write(stream, config);
}

/// Emits the multiline form, with each member on its own indented line.
fn format_import_members_multiline(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    let mut children = element.children().expect("import members children");

    let open_brace = children.next().expect("import member open brace");
    assert!(open_brace.element().kind() == SyntaxKind::OpenBrace);
    (open_brace).write(stream, config);

    let mut items = Vec::new();
    let mut commas = Vec::new();
    let mut close_brace = None;

    for child in children {
        match child.element().kind() {
            SyntaxKind::CloseBrace => {
                close_brace = Some(child.to_owned());
            }
            SyntaxKind::Comma => {
                commas.push(child.to_owned());
            }
            _ => {
                items.push(child.to_owned());
            }
        }
    }

    let empty = items.is_empty();
    if !empty {
        stream.increment_indent();
    }
    let mut items = items.iter().peekable();
    let mut commas = commas.iter();
    while let Some(item) = items.next() {
        (&item).write(stream, config);
        if let Some(comma) = commas.next() {
            // check if this comma can be dropped when trailing commas are disabled. Comma
            // can be dropped iff this is the last item and the comma does not
            // have a comment.
            if config.trailing_commas || items.peek().is_some() || comma.has_comment() {
                (comma).write(stream, config);
            }
        } else if config.trailing_commas {
            stream.push_literal(",".to_string(), SyntaxKind::Comma);
        }
        stream.end_line();
    }

    if !empty {
        stream.decrement_indent();
    }
    (&close_brace.expect("import members close brace")).write(stream, config);
}

/// Formats an [`ImportMember`].
///
/// A space surrounds the optional `as` keyword.
pub fn format_import_member(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("import member children") {
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
pub fn format_symbolic_module_path(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("symbolic module path children") {
        match child.element().kind() {
            SyntaxKind::Ident => {
                (&child).write(stream, config);
            }
            SyntaxKind::Slash => {
                // `SyntaxKind::Slash` is a "linebreakable" token, but we don't want module
                // paths to get line broken; so we push this token as a
                // `LiteralStringText` to prevent that.
                stream.push_ast_token_as(
                    child.element().as_token().expect("slash should be token"),
                    SyntaxKind::LiteralStringText,
                );
            }
            _ => {
                unreachable!("unexpected symbolic module path child");
            }
        }
    }
}
