//! Formatting for structs.

use wdl_ast::SyntaxKind;

use crate::Config;
use crate::PreToken;
use crate::TokenStream;
use crate::Writable as _;
use crate::element::FormatElement;

/// Formats a [`StructDefinition`](wdl_ast::v1::StructDefinition).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_struct_definition(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    let mut children = element.children().expect("struct definition children");

    let struct_keyword = children.next().expect("struct keyword");
    assert!(struct_keyword.element().kind() == SyntaxKind::StructKeyword);
    (&struct_keyword).write(stream, config);
    stream.end_word();

    let name = children.next().expect("struct name");
    assert!(name.element().kind() == SyntaxKind::Ident);
    (&name).write(stream, config);
    stream.end_word();

    let open_brace = children.next().expect("open brace");
    assert!(open_brace.element().kind() == SyntaxKind::OpenBrace);
    (&open_brace).write(stream, config);
    stream.end_line();
    stream.increment_indent();

    let mut meta = None;
    let mut parameter_meta = None;
    let mut members = Vec::new();
    let mut close_brace = None;

    for child in children {
        match child.element().kind() {
            SyntaxKind::MetadataSectionNode => {
                meta = Some(child.clone());
            }
            SyntaxKind::ParameterMetadataSectionNode => {
                parameter_meta = Some(child.clone());
            }
            SyntaxKind::UnboundDeclNode => {
                members.push(child.clone());
            }
            SyntaxKind::CloseBrace => {
                close_brace = Some(child.clone());
            }
            _ => {
                unreachable!(
                    "unexpected child in struct definition: {:?}",
                    child.element().kind()
                );
            }
        }
    }

    if let Some(meta) = meta {
        (&meta).write(stream, config);
        stream.blank_line();
    }

    if let Some(parameter_meta) = parameter_meta {
        (&parameter_meta).write(stream, config);
        stream.blank_line();
    }

    for member in members {
        (&member).write(stream, config);
    }

    stream.decrement_indent();
    (&close_brace.expect("struct definition close brace")).write(stream, config);
    stream.end_line();
}

/// Formats a [`LiteralStructItem`](wdl_ast::v1::LiteralStructItem).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_literal_struct_item(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    let mut children = element.children().expect("literal struct item children");

    let key = children.next().expect("literal struct item key");
    assert!(key.element().kind() == SyntaxKind::Ident);
    (&key).write(stream, config);

    let colon = children.next().expect("literal struct item colon");
    assert!(colon.element().kind() == SyntaxKind::Colon);
    (&colon).write(stream, config);
    stream.end_word();

    for child in children {
        (&child).write(stream, config);
    }
}

/// Formats a [`LiteralStruct`](wdl_ast::v1::LiteralStruct).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_literal_struct(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    let mut children = element.children().expect("literal struct children");

    let name = children.next().expect("literal struct name");
    assert!(name.element().kind() == SyntaxKind::Ident);
    (&name).write(stream, config);
    stream.end_word();

    let open_brace = children.next().expect("literal struct open brace");
    assert!(open_brace.element().kind() == SyntaxKind::OpenBrace);
    (&open_brace).write(stream, config);
    stream.increment_indent();

    let mut members = Vec::new();
    let mut commas = Vec::new();
    let mut close_brace = None;

    for child in children {
        match child.element().kind() {
            SyntaxKind::LiteralStructItemNode => {
                members.push(child.clone());
            }
            SyntaxKind::Comma => {
                commas.push(child.clone());
            }
            SyntaxKind::CloseBrace => {
                close_brace = Some(child.clone());
            }
            _ => {
                unreachable!(
                    "unexpected child in literal struct: {:?}",
                    child.element().kind()
                );
            }
        }
    }

    let mut commas = commas.iter();
    for member in members {
        (&member).write(stream, config);
        if let Some(comma) = commas.next() {
            (comma).write(stream, config);
        } else {
            stream.push_literal(",".to_string(), SyntaxKind::Comma);
        }
        stream.end_line();
    }

    stream.decrement_indent();
    (&close_brace.expect("literal struct close brace")).write(stream, config);
}
