//! Formatting for workflow calls.

use wdl_ast::SyntaxKind;

use crate::Config;
use crate::FitOrSplitEndingLiterals;
use crate::PreToken;
use crate::SplitAlternative;
use crate::TokenStream;
use crate::Writable as _;
use crate::element::FormatElement;

/// Formats a [`CallTarget`](wdl_ast::v1::CallTarget).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_call_target(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("call target children") {
        (&child).write(stream, config);
    }
}

/// Formats a [`CallAlias`](wdl_ast::v1::CallAlias).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_call_alias(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("call alias children") {
        (&child).write(stream, config);
        stream.end_word();
    }
}

/// Formats a [`CallAfter`](wdl_ast::v1::CallAfter).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_call_after(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("call after children") {
        (&child).write(stream, config);
        stream.end_word();
    }
}

/// Formats a [`CallInputItem`](wdl_ast::v1::CallInputItem).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_call_input_item(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    let mut children = element.children().expect("call input item children");

    let name = children.next().expect("call input item name");
    (&name).write(stream, config);
    // Don't call end_word() here in case the name is alone in which case it should
    // be followed by a comma.

    if let Some(equals) = children.next() {
        stream.end_word();
        (&equals).write(stream, config);
        stream.end_word();

        let value = children.next().expect("call input item value");
        (&value).write(stream, config);
    }
}

/// Formats a [`CallStatement`](wdl_ast::v1::CallStatement).
///
/// The call statement's input clause (braced `{...}` content) will be dropped
/// if possible. Dropping is possible when there are no inputs specified and
/// there are no comments attached to or within the braces.
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_call_statement(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    let mut drop_input_clause = true;

    let mut children = element.children().expect("call statement children");

    let call_keyword = children.next().expect("call keyword");
    assert!(call_keyword.element().kind() == SyntaxKind::CallKeyword);
    (&call_keyword).write(stream, config);
    stream.end_word();

    let target = children.next().expect("call target");
    (&target).write(stream, config);
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
                drop_input_clause &= !child.has_comment();
            }
            SyntaxKind::InputKeyword => {
                input_keyword = Some(child.clone());
                drop_input_clause &= !child.has_comment();
            }
            SyntaxKind::Colon => {
                colon = Some(child.clone());
                drop_input_clause &= !child.has_comment();
            }
            SyntaxKind::CallInputItemNode => {
                inputs.push(child.clone());
                drop_input_clause = false;
            }
            SyntaxKind::Comma => {
                commas.push(child.clone());
            }
            SyntaxKind::CloseBrace => {
                close_brace = Some(child.clone());
                drop_input_clause &= !child.has_comment();
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
        (&alias).write(stream, config);
        stream.end_word();
    }

    for after in afters {
        (&after).write(stream, config);
        stream.end_word();
    }

    if drop_input_clause {
        stream.end_line();
        return;
    }

    if let Some(open_brace) = open_brace {
        (&open_brace).write(stream, config);

        if let Some(input_keyword) = input_keyword {
            stream.end_word();
            (&input_keyword).write(stream, config);
            (&colon.expect("colon")).write(stream, config);
        }
        stream.fit_or_split_start(SplitAlternative::Space);

        let mut inputs = inputs.iter().peekable();
        let mut commas = commas.iter();
        let mut trailing_comma_inserted = false;
        while let Some(input) = inputs.next() {
            (&input).write(stream, config);

            if let Some(comma) = commas.next()
                && (inputs.peek().is_some() || comma.has_comment())
            {
                (comma).write(stream, config);
                if inputs.peek().is_none() {
                    trailing_comma_inserted = true;
                }
            }

            if inputs.peek().is_some() {
                stream.potential_split(SplitAlternative::Space);
            }
        }

        let trailing_literals = FitOrSplitEndingLiterals {
            fit: Some(" ".to_string().into()),
            split: if config.trailing_commas && !trailing_comma_inserted {
                Some(",".to_string().into())
            } else {
                None
            },
        };
        stream.fit_or_split_end(trailing_literals);
        (&close_brace.expect("close brace")).write(stream, config);
    }
    stream.end_line();
}
