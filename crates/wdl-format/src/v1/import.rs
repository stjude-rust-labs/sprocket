//! Formatting for imports.

use wdl_ast::SyntaxKind;

use crate::Config;
use crate::FitOrSplitEndingLiterals;
use crate::PreToken;
use crate::SplitAlternative;
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
}

/// Formats a [`ImportMembers`](wdl_ast::v1::ImportMembers).
pub fn format_import_members(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    let mut children = element.children().expect("import members children");

    let open_brace = children.next().expect("import member open brace");
    assert!(open_brace.element().kind() == SyntaxKind::OpenBrace);
    (open_brace).write(stream, config);
    stream.fit_or_split_start(SplitAlternative::Space);

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

    let mut items = items.iter().peekable();
    let mut commas = commas.iter();
    let mut trailing_comma_inserted = false;
    while let Some(item) = items.next() {
        (&item).write(stream, config);
        if let Some(comma) = commas.next()
            && (items.peek().is_some() || comma.has_comment())
        {
            (comma).write(stream, config);
            if items.peek().is_none() {
                trailing_comma_inserted = true;
            }
        }
        if items.peek().is_some() {
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
    (&close_brace.expect("import members close brace")).write(stream, config);
}

/// Formats an [`ImportMember`](wdl_ast::v1::ImportMember).
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
