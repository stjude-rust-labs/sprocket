//! Formatting for enums.

use wdl_ast::SyntaxKind;

use crate::Config;
use crate::PreToken;
use crate::TokenStream;
use crate::Writable as _;
use crate::element::FormatElement;

/// Formats an [`EnumDefinition`](wdl_ast::v1::EnumDefinition).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_enum_definition(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    let mut children = element.children().expect("enum definition children");

    let enum_keyword = children.next().expect("enum keyword");
    assert!(enum_keyword.element().kind() == SyntaxKind::EnumKeyword);
    (&enum_keyword).write(stream, config);
    stream.end_word();

    let name = children.next().expect("enum name");
    assert!(name.element().kind() == SyntaxKind::Ident);
    (&name).write(stream, config);

    let mut choices = Vec::new();
    let mut commas = Vec::new();
    let mut close_brace = None;

    for child in children {
        match child.element().kind() {
            SyntaxKind::EnumTypeParameterNode => {
                (&child).write(stream, config);
            }
            SyntaxKind::OpenBrace => {
                stream.end_word();
                (&child).write(stream, config);
                stream.end_line();
                stream.increment_indent();
            }
            SyntaxKind::EnumChoiceNode => {
                choices.push(child.clone());
            }
            SyntaxKind::Comma => {
                commas.push(child.clone());
            }
            SyntaxKind::CloseBrace => {
                close_brace = Some(child.clone());
            }
            _ => {
                unreachable!(
                    "unexpected child in enum definition: {:?}",
                    child.element().kind()
                );
            }
        }
    }

    let mut commas = commas.iter();
for choice in choices {
        (&choice).write(stream, config);
main
        if let Some(comma) = commas.next() {
            (comma).write(stream, config);
        } else {
            stream.push_literal(",".to_string(), SyntaxKind::Comma);
        }
        stream.end_line();
    }

    stream.decrement_indent();
    (&close_brace.expect("enum definition close brace")).write(stream, config);
    stream.end_line();
}

/// Formats an [`EnumChoice`](wdl_ast::v1::EnumChoice).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_enum_choice(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    let mut children = element.children().expect("enum choice children");
 main

    let name = children.next().expect("enum choice name");
    assert!(name.element().kind() == SyntaxKind::Ident);
    (&name).write(stream, config);

    for child in children {
        match child.element().kind() {
            SyntaxKind::Assignment => {
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

/// Formats an [`EnumTypeParameter`](wdl_ast::v1::EnumTypeParameter).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_enum_type_parameter(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("enum type parameter children") {
        (&child).write(stream, config);
    }
}
