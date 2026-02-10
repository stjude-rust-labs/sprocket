//! Formatting for workflow calls.

use wdl_ast::SyntaxKind;

use crate::PreToken;
use crate::TokenStream;
use crate::Writable as _;
use crate::element::FormatElement;

/// Formats a [`CallTarget`](wdl_ast::v1::CallTarget).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_call_target(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    for child in element.children().expect("call target children") {
        (&child).write(stream, None);
    }
}

/// Formats a [`CallAlias`](wdl_ast::v1::CallAlias).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_call_alias(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    for child in element.children().expect("call alias children") {
        (&child).write(stream, None);
        stream.end_word();
    }
}

/// Formats a [`CallAfter`](wdl_ast::v1::CallAfter).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_call_after(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    for child in element.children().expect("call after children") {
        (&child).write(stream, None);
        stream.end_word();
    }
}

/// Formats a [`CallInputItem`](wdl_ast::v1::CallInputItem).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_call_input_item(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    let mut children = element.children().expect("call input item children");

    let name = children.next().expect("call input item name");
    (&name).write(stream, None);
    // Don't call end_word() here in case the name is alone in which case it should
    // be followed by a comma.

    if let Some(equals) = children.next() {
        stream.end_word();
        (&equals).write(stream, None);
        stream.end_word();

        let value = children.next().expect("call input item value");
        (&value).write(stream, None);
    }
}

/// Formats a [`CallStatement`](wdl_ast::v1::CallStatement).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_call_statement(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    let mut children = element.children().expect("call statement children");

    let call_keyword = children.next().expect("call keyword");
    assert!(call_keyword.element().kind() == SyntaxKind::CallKeyword);
    (&call_keyword).write(stream, None);
    stream.end_word();

    let target = children.next().expect("call target");
    (&target).write(stream, None);
    stream.end_word();

    let mut alias = None;
    let mut afters = Vec::new();
    let mut open_brace = None;
    let mut input_keyword = None;
    let mut colon = None;
    let mut inputs = Vec::new();
    let mut commas = Vec::new();
    let mut close_brace = None;

    for child in children {
        match child.element().kind() {
            SyntaxKind::CallAliasNode => {
                alias = Some(child.clone());
            }
            SyntaxKind::CallAfterNode => {
                afters.push(child.clone());
            }
            SyntaxKind::OpenBrace => {
                open_brace = Some(child.clone());
            }
            SyntaxKind::InputKeyword => {
                input_keyword = Some(child.clone());
            }
            SyntaxKind::Colon => {
                colon = Some(child.clone());
            }
            SyntaxKind::CallInputItemNode => {
                inputs.push(child.clone());
            }
            SyntaxKind::Comma => {
                commas.push(child.clone());
            }
            SyntaxKind::CloseBrace => {
                close_brace = Some(child.clone());
            }
            _ => {
                unreachable!(
                    "unexpected child in call statement: {:?}",
                    child.element().kind()
                );
            }
        }
    }

    if let Some(alias) = alias {
        (&alias).write(stream, None);
        stream.end_word();
    }

    for after in afters {
        (&after).write(stream, None);
        stream.end_word();
    }

    if let Some(open_brace) = open_brace {
        (&open_brace).write(stream, None);
        stream.end_word();

        if let Some(input_keyword) = input_keyword {
            (&input_keyword).write(stream, None);
            (&colon.expect("colon")).write(stream, None);
            stream.end_word();
        }

        stream.increment_indent();

        let mut commas = commas.iter();
        for input in inputs {
            (&input).write(stream, None);

            if let Some(comma) = commas.next() {
                (comma).write(stream, None);
            } else {
                stream.push_literal(",".to_string(), SyntaxKind::Comma);
            }

            stream.end_line();
        }

        stream.decrement_indent();
        (&close_brace.expect("close brace")).write(stream, None);
        stream.end_line();
    }
}
