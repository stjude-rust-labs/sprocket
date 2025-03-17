//! Formatting of WDL v1.x elements.

use wdl_ast::AstToken;
use wdl_ast::SyntaxKind;

pub mod decl;
pub mod expr;
pub mod import;
pub mod meta;
pub mod r#struct;
pub mod task;
pub mod workflow;

use crate::PreToken;
use crate::TokenStream;
use crate::Writable as _;
use crate::element::FormatElement;

/// Formats an [`Ast`](wdl_ast::Ast).
pub fn format_ast(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
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
        let a_uri = a.uri().text().expect("import uri");
        let b_uri = b.uri().text().expect("import uri");
        a_uri.text().cmp(b_uri.text())
    });

    stream.blank_lines_allowed_between_comments();
    for import in imports {
        (&import).write(stream);
    }

    stream.blank_line();

    for child in remainder {
        (&child).write(stream);
        stream.blank_line();
    }
}

/// Formats a [`VersionStatement`](wdl_ast::VersionStatement).
pub fn format_version_statement(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    for child in element.children().expect("version statement children") {
        (&child).write(stream);
        stream.end_word();
    }
    stream.end_line();
}

/// Formats an [`InputSection`](wdl_ast::v1::InputSection).
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

    // TODO: sort inputs
    for input in inputs {
        (&input).write(stream);
    }

    stream.decrement_indent();
    (&close_brace.expect("input section close brace")).write(stream);
    stream.end_line();
}

/// Formats an [`OutputSection`](wdl_ast::v1::OutputSection).
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

    assert!(children.next().is_none());
}

/// Formats a [`LiteralInput`](wdl_ast::v1::LiteralInput).
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

    assert!(children.next().is_none());
}

/// Formats a [`LiteralHints`](wdl_ast::v1::LiteralHints).
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
pub fn format_literal_output_item(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    let mut children = element
        .children()
        .expect("literal output item children")
        .peekable();

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

    assert!(children.next().is_none());
}

/// Formats a [`LiteralOutput`](wdl_ast::v1::LiteralOutput).
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
