//! Formatting of WDL v1.x elements.

use std::rc::Rc;

use nonempty::NonEmpty;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxToken;

pub mod decl;
pub mod r#enum;
pub mod expr;
pub mod import;
pub mod meta;
pub mod sort;
pub mod r#struct;
pub mod task;
pub mod workflow;

use crate::PreToken;
use crate::TokenStream;
use crate::Writable as _;
use crate::element::FormatElement;

/// Formats an [`Ast`](wdl_ast::Ast).
///
/// This is the entry point for formatting WDL v1.x files.
///
/// # Panics
///
/// It will panic if the provided `element` is not a valid WDL v1.x AST.
pub fn format_ast(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    fn last_token_of_element(elem: &FormatElement) -> SyntaxToken {
        elem.element()
            .as_node()
            .expect("all children of an AST should be nodes")
            .inner()
            .last_token()
            .expect("nodes should have tokens")
    }

    let mut children = element.children().expect("AST children");

    let version_statement = children.next().expect("version statement");
    assert!(version_statement.element().kind() == SyntaxKind::VersionStatementNode);
    (&version_statement).write(stream);

    stream.blank_line();

    let mut imports = Vec::new();
    let mut remainder = Vec::new();

    for child in children {
        match child.element().kind() {
            SyntaxKind::ImportStatementNode => imports.push(child),
            _ => remainder.push(child),
        }
    }

    imports.sort_by(|a, b| {
        let a = a
            .element()
            .as_node()
            .expect("import statement node")
            .as_import_statement()
            .expect("import statement");
        let b = b
            .element()
            .as_node()
            .expect("import statement node")
            .as_import_statement()
            .expect("import statement");
        let a_uri = a
            .uri()
            .text()
            .expect("import uri should not be interpolated");
        let b_uri = b
            .uri()
            .text()
            .expect("import uri should not be interpolated");
        a_uri.text().cmp(b_uri.text())
    });

    stream.ignore_trailing_blank_lines();

    let mut trailing_comments = None;

    for import in imports {
        (&import).write(stream);

        if trailing_comments.is_none() {
            trailing_comments = find_trailing_comments(&last_token_of_element(import));
        }
    }

    stream.blank_line();

    for child in &remainder {
        (child).write(stream);

        if trailing_comments.is_none() {
            trailing_comments = find_trailing_comments(&last_token_of_element(child));
        }

        stream.blank_line();
    }

    if let Some(comments) = trailing_comments {
        stream.trim_end(&PreToken::BlankLine);
        for comment in comments {
            stream.push(PreToken::Trivia(crate::Trivia::Comment(
                crate::Comment::Preceding(Rc::new(comment.text().into())),
            )));
            stream.push(PreToken::LineEnd);
        }
    }
}

/// Finds any trailing comments at the end of a WDL document.
///
/// Trailing comments are unhandled as they don't fit neatly into the trivia
/// model used by this crate. [`crate::Comment`]s can only be "preceding" or
/// "inline", but non-inline comments at the end of a WDL document
/// have no following element to precede. This will find any such comments and
/// return them.
fn find_trailing_comments(token: &SyntaxToken) -> Option<NonEmpty<SyntaxToken>> {
    let mut next_token = token.next_token();
    let mut on_next_line = false;

    fn is_comment(token: &SyntaxToken) -> bool {
        matches!(token.kind(), SyntaxKind::Comment)
    }

    let mut encountered_comments = Vec::new();

    while let Some(next) = next_token {
        if !next.kind().is_trivia() {
            return None;
        }

        // skip if we are processing an inline comment of the input token
        let skip = !on_next_line && is_comment(&next);
        on_next_line = on_next_line || next.text().contains('\n');
        next_token = next.next_token();
        if skip {
            continue;
        }
        if is_comment(&next) {
            encountered_comments.push(next);
        }
    }

    NonEmpty::from_vec(encountered_comments)
}

/// Formats a [`VersionStatement`](wdl_ast::VersionStatement).
pub fn format_version_statement(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    let mut children = element.children().expect("version statement children");

    // We never want to start a WDL document with a blank line,
    // but the preceding trivia may include one that will
    // be added if we don't exclude it.
    let version_keyword = children.next().expect("version keyword");
    assert!(version_keyword.element().kind() == SyntaxKind::VersionKeyword);
    let mut buffer = TokenStream::<PreToken>::default();
    (&version_keyword).write(&mut buffer);
    let mut buff_iter = buffer.into_iter();
    let first_token = buff_iter.next().expect("at least one token");
    if !matches!(first_token, PreToken::Trivia(crate::Trivia::BlankLine)) {
        stream.push(first_token);
    }
    for remaining in buff_iter {
        stream.push(remaining);
    }
    stream.end_word();

    for child in children {
        (&child).write(stream);
        stream.end_word();
    }
    stream.end_line();
}

/// Formats an [`InputSection`](wdl_ast::v1::InputSection).
///
/// # Panics
///
/// This will panic if the provided `element` is not a valid WDL v1.x
/// input section.
pub fn format_input_section(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    let mut children = element.children().expect("input section children");

    let input_keyword = children.next().expect("input section input keyword");
    assert!(input_keyword.element().kind() == SyntaxKind::InputKeyword);
    (&input_keyword).write(stream);
    stream.end_word();

    let open_brace = children.next().expect("input section open brace");
    assert!(open_brace.element().kind() == SyntaxKind::OpenBrace);
    (&open_brace).write(stream);
    stream.increment_indent();

    let mut inputs = Vec::new();
    let mut close_brace = None;

    for child in children {
        match child.element().kind() {
            SyntaxKind::BoundDeclNode | SyntaxKind::UnboundDeclNode => inputs.push(child),
            SyntaxKind::CloseBrace => close_brace = Some(child),
            _ => panic!("unexpected input section child"),
        }
    }

    inputs.sort_by(|a, b| {
        let a_decl =
            wdl_ast::v1::Decl::cast(a.element().as_node().unwrap().inner().clone()).unwrap();
        let b_decl =
            wdl_ast::v1::Decl::cast(b.element().as_node().unwrap().inner().clone()).unwrap();
        sort::compare_decl(&a_decl, &b_decl)
    });
    for input in inputs {
        (&input).write(stream);
    }

    stream.decrement_indent();
    (&close_brace.expect("input section close brace")).write(stream);
    stream.end_line();
}

/// Formats an [`OutputSection`](wdl_ast::v1::OutputSection).
///
/// # Panics
///
/// This will panic if the provided `element` is not a valid WDL v1.x
/// output section.
pub fn format_output_section(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    let mut children = element.children().expect("output section children");

    let output_keyword = children.next().expect("output keyword");
    assert!(output_keyword.element().kind() == SyntaxKind::OutputKeyword);
    (&output_keyword).write(stream);
    stream.end_word();

    let open_brace = children.next().expect("output section open brace");
    assert!(open_brace.element().kind() == SyntaxKind::OpenBrace);
    (&open_brace).write(stream);
    stream.increment_indent();

    for child in children {
        if child.element().kind() == SyntaxKind::CloseBrace {
            stream.decrement_indent();
        } else {
            assert!(child.element().kind() == SyntaxKind::BoundDeclNode);
        }
        (&child).write(stream);
        stream.end_line();
    }
}

/// Formats a [`LiteralInputItem`](wdl_ast::v1::LiteralInputItem).
///
/// # Panics
///
/// This will panic if the provided `element` is not a valid WDL v1.x
/// literal input item.
pub fn format_literal_input_item(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    let mut children = element.children().expect("literal input item children");

    let key = children.next().expect("literal input item key");
    assert!(key.element().kind() == SyntaxKind::Ident);
    (&key).write(stream);

    let colon = children.next().expect("literal input item colon");
    assert!(colon.element().kind() == SyntaxKind::Colon);
    (&colon).write(stream);
    stream.end_word();

    let hints_node = children.next().expect("literal input item hints node");
    assert!(hints_node.element().kind() == SyntaxKind::LiteralHintsNode);
    (&hints_node).write(stream);
}

/// Formats a [`LiteralInput`](wdl_ast::v1::LiteralInput).
///
/// # Panics
///
/// This will panic if the provided `element` is not a valid WDL v1.x
/// literal input.
pub fn format_literal_input(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    let mut children = element.children().expect("literal input children");

    let input_keyword = children.next().expect("literal input keyword");
    assert!(input_keyword.element().kind() == SyntaxKind::InputKeyword);
    (&input_keyword).write(stream);
    stream.end_word();

    let open_brace = children.next().expect("literal input open brace");
    assert!(open_brace.element().kind() == SyntaxKind::OpenBrace);
    (&open_brace).write(stream);
    stream.increment_indent();

    for child in children {
        if child.element().kind() == SyntaxKind::CloseBrace {
            stream.decrement_indent();
        } else {
            assert!(child.element().kind() == SyntaxKind::LiteralInputItemNode);
        }
        (&child).write(stream);
    }
    stream.end_line();
}

/// Formats a [`LiteralHintsItem`](wdl_ast::v1::LiteralHintsItem).
///
/// # Panics
///
/// This will panic if the provided `element` is not a valid WDL v1.x
/// literal hints item.
pub fn format_literal_hints_item(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    let mut children = element.children().expect("literal hints item children");

    let key = children.next().expect("literal hints item key");
    assert!(key.element().kind() == SyntaxKind::Ident);
    (&key).write(stream);

    let colon = children.next().expect("literal hints item colon");
    assert!(colon.element().kind() == SyntaxKind::Colon);
    (&colon).write(stream);
    stream.end_word();

    let value = children.next().expect("literal hints item value");
    (&value).write(stream);
}

/// Formats a [`LiteralHints`](wdl_ast::v1::LiteralHints).
///
/// # Panics
///
/// This will panic if the provided `element` is not a valid WDL v1.x
/// literal hints.
pub fn format_literal_hints(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    let mut children = element.children().expect("literal hints children");

    let hints_keyword = children.next().expect("literal hints keyword");
    assert!(hints_keyword.element().kind() == SyntaxKind::HintsKeyword);
    (&hints_keyword).write(stream);
    stream.end_word();

    let open_brace = children.next().expect("literal hints open brace");
    assert!(open_brace.element().kind() == SyntaxKind::OpenBrace);
    (&open_brace).write(stream);
    stream.increment_indent();

    let mut items = Vec::new();
    let mut commas = Vec::new();
    let mut close_brace = None;

    for child in children {
        match child.element().kind() {
            SyntaxKind::LiteralHintsItemNode => items.push(child),
            SyntaxKind::Comma => commas.push(child),
            SyntaxKind::CloseBrace => close_brace = Some(child),
            _ => panic!("unexpected literal hints child"),
        }
    }

    let mut commas = commas.iter();
    for item in items {
        (&item).write(stream);
        if let Some(comma) = commas.next() {
            (comma).write(stream);
        } else {
            stream.push_literal(",".to_string(), SyntaxKind::Comma);
        }
        stream.end_line();
    }

    stream.decrement_indent();
    (&close_brace.expect("literal hints close brace")).write(stream);
}

/// Formats a [`LiteralOutputItem`](wdl_ast::v1::LiteralOutputItem).
///
/// # Panics
///
/// This will panic if the provided `element` is not a valid WDL v1.x
/// literal output item.
pub fn format_literal_output_item(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    let mut children = element.children().expect("literal output item children");

    for child in children.by_ref() {
        if matches!(child.element().kind(), SyntaxKind::Ident | SyntaxKind::Dot) {
            (&child).write(stream);
        } else {
            assert!(child.element().kind() == SyntaxKind::Colon);
            (&child).write(stream);
            stream.end_word();
            break;
        }
    }

    let value = children.next().expect("literal output item value");
    (&value).write(stream);
}

/// Formats a [`LiteralOutput`](wdl_ast::v1::LiteralOutput).
///
/// # Panics
///
/// This will panic if the provided `element` is not a valid WDL v1.x
/// literal output.
pub fn format_literal_output(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    let mut children = element.children().expect("literal output children");

    let output_keyword = children.next().expect("literal output keyword");
    assert!(output_keyword.element().kind() == SyntaxKind::OutputKeyword);
    (&output_keyword).write(stream);
    stream.end_word();

    let open_brace = children.next().expect("literal output open brace");
    assert!(open_brace.element().kind() == SyntaxKind::OpenBrace);
    (&open_brace).write(stream);
    stream.increment_indent();

    let mut items = Vec::new();
    let mut commas = Vec::new();
    let mut close_brace = None;

    for child in children {
        match child.element().kind() {
            SyntaxKind::LiteralOutputItemNode => items.push(child),
            SyntaxKind::Comma => commas.push(child),
            SyntaxKind::CloseBrace => close_brace = Some(child),
            _ => panic!("unexpected literal output child"),
        }
    }

    let mut commas = commas.iter();
    for item in items {
        (&item).write(stream);
        if let Some(comma) = commas.next() {
            (comma).write(stream);
        } else {
            stream.push_literal(",".to_string(), SyntaxKind::Comma);
        }
        stream.end_line();
    }

    stream.decrement_indent();
    (&close_brace.expect("literal output close brace")).write(stream);
}
