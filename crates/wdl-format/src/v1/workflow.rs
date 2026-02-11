//! Formatting for workflows.

pub mod call;

use wdl_ast::SyntaxKind;
use wdl_ast::Token;

use crate::PreToken;
use crate::TokenStream;
use crate::Trivia;
use crate::Writable as _;
use crate::element::FormatElement;

/// Formats a [`ConditionalStatement`](wdl_ast::v1::ConditionalStatement).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_conditional_statement(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    for child in element.children().expect("conditional statement children") {
        (&child).write(stream);
    }
    stream.end_line();
}

/// Formats a [`ConditionalStatementClause`](wdl_ast::v1::ConditionalStatementClause).
pub fn format_conditional_statement_clause(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
) {
    let mut children = element
        .children()
        .expect("conditional statement clause children")
        .peekable();

    let mut has_condition = false;

    while let Some(el) = children.peek() {
        // If the format element doesn't contain a token, it's not a keyword, so
        // break.
        let Some(token) = el.element().as_token() else {
            break;
        };

        // Write the token if it's an `if` or an `else`.
        match token {
            Token::IfKeyword(_) => {
                has_condition = true;
                el.write(stream);
                stream.end_word();
            }
            Token::ElseKeyword(_) => {
                el.write(stream);
                stream.end_word();
            }
            _ => break,
        }

        // Take the child token we just processed.
        children.next();
    }

    // If the ConditionalStatementClause contains a condition, we need to process
    // the parens and all elements inside!
    if has_condition {
        let open_paren = children.next().expect("open paren");
        assert!(open_paren.element().kind() == SyntaxKind::OpenParen);
        (&open_paren).write(stream);

        for child in children.by_ref() {
            (&child).write(stream);
            if child.element().kind() == SyntaxKind::CloseParen {
                stream.end_word();
                break;
            }
        }
    }

    let open_brace = children.next().expect("open brace");
    assert!(open_brace.element().kind() == SyntaxKind::OpenBrace);
    (&open_brace).write(stream);
    stream.increment_indent();

    for child in children {
        if child.element().kind() == SyntaxKind::CloseBrace {
            stream.decrement_indent();
        }
        (&child).write(stream);
    }
    stream.end_word();
}

/// Formats a [`ScatterStatement`](wdl_ast::v1::ScatterStatement).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_scatter_statement(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    let mut children = element.children().expect("scatter statement children");

    let scatter_keyword = children.next().expect("scatter keyword");
    assert!(scatter_keyword.element().kind() == SyntaxKind::ScatterKeyword);
    (&scatter_keyword).write(stream);
    stream.end_word();

    let open_paren = children.next().expect("open paren");
    assert!(open_paren.element().kind() == SyntaxKind::OpenParen);
    (&open_paren).write(stream);

    let variable = children.next().expect("scatter variable");
    assert!(variable.element().kind() == SyntaxKind::Ident);
    (&variable).write(stream);
    stream.end_word();

    let in_keyword = children.next().expect("in keyword");
    assert!(in_keyword.element().kind() == SyntaxKind::InKeyword);
    (&in_keyword).write(stream);
    stream.end_word();

    for child in children.by_ref() {
        (&child).write(stream);
        if child.element().kind() == SyntaxKind::CloseParen {
            stream.end_word();
            break;
        }
    }

    let open_brace = children.next().expect("open brace");
    assert!(open_brace.element().kind() == SyntaxKind::OpenBrace);
    (&open_brace).write(stream);
    stream.end_line();
    stream.increment_indent();

    for child in children {
        if child.element().kind() == SyntaxKind::CloseBrace {
            stream.decrement_indent();
        }
        (&child).write(stream);
    }
    stream.end_line();
}

/// Formats a [`WorkflowDefinition`](wdl_ast::v1::WorkflowDefinition).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_workflow_definition(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    let mut children = element.children().expect("workflow definition children");

    stream.ignore_trailing_blank_lines();

    let workflow_keyword = children.next().expect("workflow keyword");
    assert!(workflow_keyword.element().kind() == SyntaxKind::WorkflowKeyword);
    (&workflow_keyword).write(stream);
    stream.end_word();

    let name = children.next().expect("workflow name");
    assert!(name.element().kind() == SyntaxKind::Ident);
    (&name).write(stream);
    stream.end_word();

    let open_brace = children.next().expect("open brace");
    assert!(open_brace.element().kind() == SyntaxKind::OpenBrace);
    (&open_brace).write(stream);
    stream.increment_indent();

    let mut meta = None;
    let mut parameter_meta = None;
    let mut input = None;
    let mut body = Vec::new();
    let mut output = None;
    let mut hints = None;
    let mut close_brace = None;

    for child in children {
        match child.element().kind() {
            SyntaxKind::MetadataSectionNode => {
                meta = Some(child.clone());
            }
            SyntaxKind::ParameterMetadataSectionNode => {
                parameter_meta = Some(child.clone());
            }
            SyntaxKind::InputSectionNode => {
                input = Some(child.clone());
            }
            SyntaxKind::BoundDeclNode => {
                body.push(child.clone());
            }
            SyntaxKind::CallStatementNode => {
                body.push(child.clone());
            }
            SyntaxKind::ConditionalStatementNode => {
                body.push(child.clone());
            }
            SyntaxKind::ScatterStatementNode => {
                body.push(child.clone());
            }
            SyntaxKind::OutputSectionNode => {
                output = Some(child.clone());
            }
            SyntaxKind::WorkflowHintsSectionNode => {
                hints = Some(child.clone());
            }
            SyntaxKind::CloseBrace => {
                close_brace = Some(child.clone());
            }
            _ => {
                unreachable!(
                    "unexpected child in workflow definition: {:?}",
                    child.element().kind()
                );
            }
        }
    }

    if let Some(meta) = meta {
        (&meta).write(stream);
        stream.blank_line();
    }

    if let Some(parameter_meta) = parameter_meta {
        (&parameter_meta).write(stream);
        stream.blank_line();
    }

    if let Some(input) = input {
        (&input).write(stream);
        stream.blank_line();
    }

    stream.allow_blank_lines();
    for child in body {
        (&child).write(stream);
    }
    stream.ignore_trailing_blank_lines();
    stream.blank_line();

    if let Some(output) = output {
        (&output).write(stream);
        stream.blank_line();
    }

    if let Some(hints) = hints {
        (&hints).write(stream);
        stream.blank_line();
    }

    stream.trim_while(|t| matches!(t, PreToken::BlankLine | PreToken::Trivia(Trivia::BlankLine)));

    stream.decrement_indent();
    (&close_brace.expect("workflow close brace")).write(stream);
    stream.end_line();
}

/// Formats a [`WorkflowHintsArray`](wdl_ast::v1::WorkflowHintsArray).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_workflow_hints_array(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    let mut children = element.children().expect("workflow hints array children");

    let open_bracket = children.next().expect("open bracket");
    assert!(open_bracket.element().kind() == SyntaxKind::OpenBracket);
    (&open_bracket).write(stream);
    stream.increment_indent();

    let mut items = Vec::new();
    let mut commas = Vec::new();
    let mut close_bracket = None;

    for child in children {
        match child.element().kind() {
            SyntaxKind::Comma => {
                commas.push(child.clone());
            }
            SyntaxKind::CloseBracket => {
                close_bracket = Some(child.clone());
            }
            _ => {
                items.push(child.clone());
            }
        }
    }

    let mut commas = commas.into_iter();
    for item in items {
        (&item).write(stream);
        match commas.next() {
            Some(comma) => {
                (&comma).write(stream);
            }
            _ => {
                stream.push_literal(",".to_string(), SyntaxKind::Comma);
            }
        }
        stream.end_line();
    }

    stream.decrement_indent();
    (&close_bracket.expect("workflow hints array close bracket")).write(stream);
}

/// Formats a [`WorkflowHintsItem`](wdl_ast::v1::WorkflowHintsItem).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_workflow_hints_item(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    let mut children = element.children().expect("workflow hints item children");

    let key = children.next().expect("workflow hints item key");
    assert!(key.element().kind() == SyntaxKind::Ident);
    (&key).write(stream);

    let colon = children.next().expect("workflow hints item colon");
    assert!(colon.element().kind() == SyntaxKind::Colon);
    (&colon).write(stream);
    stream.end_word();

    let value = children.next().expect("workflow hints item value");
    (&value).write(stream);

    stream.end_line();
}

/// Formats a [`WorkflowHintsObjectItem`](wdl_ast::v1::WorkflowHintsObjectItem).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_workflow_hints_object_item(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
) {
    let mut children = element
        .children()
        .expect("workflow hints object item children");

    let key = children.next().expect("workflow hints object item key");
    assert!(key.element().kind() == SyntaxKind::Ident);
    (&key).write(stream);

    let colon = children.next().expect("workflow hints object item colon");
    assert!(colon.element().kind() == SyntaxKind::Colon);
    (&colon).write(stream);
    stream.end_word();

    let value = children.next().expect("workflow hints object item value");
    (&value).write(stream);

    stream.end_line();
}

/// Formats a [`WorkflowHintsObject`](wdl_ast::v1::WorkflowHintsObject).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_workflow_hints_object(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    let mut children = element.children().expect("workflow hints object children");

    let open_brace = children.next().expect("open brace");
    assert!(open_brace.element().kind() == SyntaxKind::OpenBrace);
    (&open_brace).write(stream);
    stream.increment_indent();

    for child in children {
        if child.element().kind() == SyntaxKind::CloseBrace {
            stream.decrement_indent();
        }
        (&child).write(stream);
        stream.end_line();
    }
}

/// Formats a [`WorkflowHintsSection`](wdl_ast::v1::WorkflowHintsSection).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_workflow_hints_section(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    let mut children = element.children().expect("workflow hints section children");

    let hints_keyword = children.next().expect("hints keyword");
    assert!(hints_keyword.element().kind() == SyntaxKind::HintsKeyword);
    (&hints_keyword).write(stream);
    stream.end_word();

    let open_brace = children.next().expect("open brace");
    assert!(open_brace.element().kind() == SyntaxKind::OpenBrace);
    (&open_brace).write(stream);
    stream.increment_indent();

    for child in children {
        if child.element().kind() == SyntaxKind::CloseBrace {
            stream.decrement_indent();
        }
        (&child).write(stream);
        stream.end_line();
    }
}
