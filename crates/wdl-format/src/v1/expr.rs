//! Formatting of WDL v1.x expression elements.

use wdl_ast::AstNode as _;
use wdl_ast::Direction;
use wdl_ast::Element;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_ast::v1::Expr;

use crate::Config;
use crate::MaxLineLength;
use crate::PostToken;
use crate::Postprocessor;
use crate::PreToken;
use crate::SPACE;
use crate::TokenStream;
use crate::Writable as _;
use crate::element::AstElementFormatExt as _;
use crate::element::FormatElement;

/// Formats a [`SepOption`](wdl_ast::v1::SepOption).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_sep_option(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    let mut children = element.children().expect("sep option children");

    let sep_keyword = children.next().expect("sep keyword");
    assert_eq!(sep_keyword.element().kind(), SyntaxKind::Ident);
    (&sep_keyword).write(stream, config);

    let equals = children.next().expect("sep equals");
    assert_eq!(equals.element().kind(), SyntaxKind::Assignment);
    (&equals).write(stream, config);

    let sep_value = children.next().expect("sep value");
    assert_eq!(sep_value.element().kind(), SyntaxKind::LiteralStringNode);
    (&sep_value).write(stream, config);
}

/// Formats a [`DefaultOption`](wdl_ast::v1::DefaultOption).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_default_option(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    let mut children = element.children().expect("default option children");

    let default_keyword = children.next().expect("default keyword");
    assert_eq!(default_keyword.element().kind(), SyntaxKind::Ident);
    (&default_keyword).write(stream, config);

    let equals = children.next().expect("default equals");
    assert_eq!(equals.element().kind(), SyntaxKind::Assignment);
    (&equals).write(stream, config);

    let default_value = children.next().expect("default value");
    (&default_value).write(stream, config);
}

/// Formats a [`TrueFalseOption`](wdl_ast::v1::TrueFalseOption).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_true_false_option(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    let mut children = element.children().expect("true false option children");

    let first_keyword = children.next().expect("true false option first keyword");
    let first_keyword_kind = first_keyword.element().kind();
    assert!(
        first_keyword_kind == SyntaxKind::TrueKeyword
            || first_keyword_kind == SyntaxKind::FalseKeyword
    );

    let first_equals = children.next().expect("true false option first equals");
    assert_eq!(first_equals.element().kind(), SyntaxKind::Assignment);

    let first_value = children.next().expect("true false option first value");

    let second_keyword = children.next().expect("true false option second keyword");
    let second_keyword_kind = second_keyword.element().kind();
    assert!(
        second_keyword_kind == SyntaxKind::TrueKeyword
            || second_keyword_kind == SyntaxKind::FalseKeyword
    );

    let second_equals = children.next().expect("true false option second equals");
    assert_eq!(second_equals.element().kind(), SyntaxKind::Assignment);

    let second_value = children.next().expect("true false option second value");

    if first_keyword_kind == SyntaxKind::TrueKeyword {
        assert_eq!(second_keyword_kind, SyntaxKind::FalseKeyword);
        (&first_keyword).write(stream, config);
        (&first_equals).write(stream, config);
        (&first_value).write(stream, config);
        stream.end_word();
        (&second_keyword).write(stream, config);
        (&second_equals).write(stream, config);
        (&second_value).write(stream, config);
    } else {
        assert_eq!(second_keyword_kind, SyntaxKind::TrueKeyword);
        (&second_keyword).write(stream, config);
        (&second_equals).write(stream, config);
        (&second_value).write(stream, config);
        stream.end_word();
        (&first_keyword).write(stream, config);
        (&first_equals).write(stream, config);
        (&first_value).write(stream, config);
    }
}

/// Formats a [`Placeholder`](wdl_ast::v1::Placeholder).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_placeholder(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    let mut children = element.children().expect("placeholder children");

    let open = children.next().expect("placeholder open");
    assert_eq!(open.element().kind(), SyntaxKind::PlaceholderOpen);
    let syntax = open.element().inner();
    let text = syntax.as_token().expect("token").text();
    match text {
        "${" => {
            stream.push_literal_in_place_of_token(
                open.element().as_token().expect("token"),
                "~{".to_owned(),
            );
        }
        "~{" => {
            (&open).write(stream, config);
        }
        _ => {
            unreachable!("unexpected placeholder open: {:?}", text);
        }
    }

    if let Some(first) = children.next() {
        // do not end_word() before the first child
        (&first).write(stream, config);
    }
    for child in children {
        if child.element().kind() != SyntaxKind::CloseBrace {
            stream.end_word();
        }
        (&child).write(stream, config);
    }
}

/// Formats a [`LiteralString`](wdl_ast::v1::LiteralString).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_literal_string(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("literal string children") {
        match child.element().kind() {
            SyntaxKind::SingleQuote => {
                stream.push_literal_in_place_of_token(
                    child.element().as_token().expect("token"),
                    "\"".to_owned(),
                );
            }
            SyntaxKind::OpenHeredoc | SyntaxKind::CloseHeredoc | SyntaxKind::DoubleQuote => {
                (&child).write(stream, config);
            }
            SyntaxKind::LiteralStringText => {
                let mut replacement = String::new();
                let syntax = child.element().inner();
                let mut chars = syntax.as_token().expect("token").text().chars().peekable();
                let mut prev_c = None;
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            if let Some(next_c) = chars.peek()
                                && *next_c == '\''
                            {
                                // Do not write this backslash as single quotes don't need
                                // escaping in a double-quoted string (and we format all
                                // LiteralStrings as double-quoted strings).
                                prev_c = Some(c);
                                continue;
                            }
                            replacement.push(c);
                        }
                        '"' => {
                            if prev_c.is_none_or(|c| c != '\\') {
                                // This double quote sign is not escaped, so we need to escape
                                // it. This happens when a single quoted string is re-formatted
                                // as a double quoted string.
                                replacement.push('\\');
                            }
                            replacement.push(c);
                        }
                        _ => {
                            replacement.push(c);
                        }
                    }
                    prev_c = Some(c);
                }

                stream.push_literal_in_place_of_token(
                    child.element().as_token().expect("token"),
                    replacement,
                );
            }
            SyntaxKind::PlaceholderNode => {
                (&child).write(stream, config);
            }
            _ => {
                unreachable!(
                    "unexpected child in literal string: {:?}",
                    child.element().kind()
                );
            }
        }
    }
}

/// Formats a [`LiteralNone`](wdl_ast::v1::LiteralNone).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_literal_none(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    let mut children = element.children().expect("literal none children");
    let none = children.next().expect("literal none token");
    assert_eq!(none.element().kind(), SyntaxKind::NoneKeyword);
    (&none).write(stream, config);
}

/// Formats a [`LiteralPair`](wdl_ast::v1::LiteralPair).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_literal_pair(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    let mut children = element.children().expect("literal pair children");

    let open_paren = children.next().expect("literal pair open paren");
    assert_eq!(open_paren.element().kind(), SyntaxKind::OpenParen);
    (&open_paren).write(stream, config);

    let left = children.next().expect("literal pair left");
    (&left).write(stream, config);

    let comma = children.next().expect("literal pair comma");
    assert_eq!(comma.element().kind(), SyntaxKind::Comma);
    (&comma).write(stream, config);
    stream.end_word();

    let right = children.next().expect("literal pair right");
    (&right).write(stream, config);

    let close_paren = children.next().expect("literal pair close paren");
    assert_eq!(close_paren.element().kind(), SyntaxKind::CloseParen);
    (&close_paren).write(stream, config);
}

/// Formats a [`LiteralBoolean`](wdl_ast::v1::LiteralBoolean).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_literal_boolean(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    let mut children = element.children().expect("literal boolean children");
    let bool = children.next().expect("literal boolean token");
    (&bool).write(stream, config);
}

/// Formats a [`NegationExpr`](wdl_ast::v1::NegationExpr).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_negation_expr(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    let mut children = element.children().expect("negation expr children");
    let minus = children.next().expect("negation expr minus");
    assert_eq!(minus.element().kind(), SyntaxKind::Minus);
    (&minus).write(stream, config);

    let expr = children.next().expect("negation expr expr");
    (&expr).write(stream, config);
}

/// Formats a [`LiteralInteger`](wdl_ast::v1::LiteralInteger).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_literal_integer(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("literal integer children") {
        (&child).write(stream, config);
    }
}

/// Formats a [`LiteralFloat`](wdl_ast::v1::LiteralFloat).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_literal_float(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("literal float children") {
        (&child).write(stream, config);
    }
}

/// Formats a [`NameRefExpr`](wdl_ast::v1::NameRefExpr).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_name_ref_expr(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    let mut children = element.children().expect("name ref children");
    let name = children.next().expect("name ref name");
    (&name).write(stream, config);
}

/// Formats a [`LiteralArray`](wdl_ast::v1::LiteralArray).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_literal_array(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    // Decided before any of the array's own tokens are written, as the fit test
    // takes the array's starting column from the stream as it stands here.
    let multiline =
        contains_element_requiring_multiline(element) || overflows_line(element, stream, config);

    let mut children = element.children().expect("literal array children");

    let open_bracket = children.next().expect("literal array open bracket");
    assert_eq!(open_bracket.element().kind(), SyntaxKind::OpenBracket);
    (&open_bracket).write(stream, config);

    let mut items = Vec::new();
    let mut commas = Vec::new();
    let mut close_bracket = None;

    for child in children {
        match child.element().kind() {
            SyntaxKind::CloseBracket => {
                close_bracket = Some(child.to_owned());
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
    if multiline && !empty {
        stream.increment_indent();
    }

    let mut items = items.iter().peekable();
    let mut commas = commas.iter();
    while let Some(item) = items.next() {
        (item).write(stream, config);
        if multiline {
            if let Some(comma) = commas.next()
                && (items.peek().is_some() || comma.has_comment())
            {
                (comma).write(stream, config);
                if items.peek().is_some() {
                    stream.end_line();
                }
            } else if config.trailing_commas {
                stream.push_literal(",".into(), SyntaxKind::Comma);
            }
        } else if items.peek().is_some() {
            stream.push_literal(",".into(), SyntaxKind::Comma);
            stream.end_word();
        }
    }

    if multiline && !empty {
        stream.decrement_indent();
    }
    (&close_bracket.expect("literal array close bracket")).write(stream, config);
}

/// Returns `true` if the underlying syntax for `element` contains a direct
/// child (comment, map/object/struct literal) that forces the array to be
/// formatted multiline.
fn contains_element_requiring_multiline(element: &FormatElement) -> bool {
    let Some(node) = element.element().as_node() else {
        return false;
    };

    node.inner().children_with_tokens().any(|c| {
        matches!(
            c.kind(),
            SyntaxKind::Comment
                | SyntaxKind::LiteralMapNode
                | SyntaxKind::LiteralObjectNode
                | SyntaxKind::LiteralStructNode
        )
    })
}

/// Returns `true` if writing `element` on a single line would take that line
/// past the maximum line length.
///
/// This is the sum of the column the array starts at, the width of the array
/// itself, and the width of whatever is written after it on the same line.
/// `stream` must not yet contain any of the array's tokens.
fn overflows_line(
    element: &FormatElement,
    stream: &TokenStream<PreToken>,
    config: &Config,
) -> bool {
    let Some(max) = config.max_line_length.get() else {
        return false;
    };

    let (column, open) = line_position(stream, config);
    let (width, wraps) = first_line_width(element, config);

    // An array with a nested element of its own that has to be written across
    // several lines cannot be written on one, whatever its width.
    wraps || column + width + array_suffix_width(element, open, config) > max
}

/// Returns the width of everything that would be written after the array's
/// closing bracket on the same line.
///
/// None of this has been written to the stream yet, so it is measured from the
/// syntax tree: the siblings that follow the array, then the siblings that
/// follow each of its ancestors, for as long as those ancestors are still on
/// the array's line. `open` is the number of delimiters left open on that line
/// and so bounds how far up the walk can go.
fn array_suffix_width(element: &FormatElement, mut open: usize, config: &Config) -> usize {
    let mut width = 0;
    let mut current = element
        .element()
        .as_node()
        .expect("literal array node")
        .inner()
        .clone();

    while let Some(parent) = current.parent() {
        let parent_kind = parent.kind();

        if opens_delimiter(parent_kind) {
            match open.checked_sub(1) {
                // Climbing the syntax tree, delimiter-opening ancestors are encountered
                // innermost-first. The first `open` of them are therefore the ones whose openers
                // are on the array's line, and whose closers are still to be written after the
                // array.
                Some(remaining) => open = remaining,
                // The delimiter `parent` opens is not on the array's line, so
                // the line began within it and ends before it is closed.
                None => return width + line_remainder_width(&current, config),
            }
        }

        // Whitespace is written between two children, so the left one of the
        // pair is tracked as the siblings are walked.
        let mut left_kind = current.kind();

        let mut next = current.next_sibling_or_token();
        while let Some(sibling) = next {
            next = sibling.next_sibling_or_token();
            // Neither kind of trivia is measured here: source whitespace is replaced by
            // what `trailing_space_width` reports below, and a comment on the array's line
            // would be written as inline trivia of the token it follows, so it would
            // already have been measured with that token.
            if sibling.kind().is_trivia() {
                continue;
            }
            if starts_new_line(parent_kind, sibling.kind()) {
                return width;
            }

            let element = Element::cast(sibling.clone()).into_format_element();
            let (sibling_width, wraps) = first_line_width(&element, config);
            width += trailing_space_width(parent_kind, left_kind) + sibling_width;
            if wraps {
                // The line ends inside this sibling, so its first line is the last thing
                // written on the array's line.
                return width;
            }
            left_kind = sibling.kind();
        }

        if ends_line(parent_kind) {
            // The array's line ends where `parent` does, so the walk climbs no
            // further. What the enclosing list writes between `parent` and that
            // break is still on the line: the comma separating `parent` from the
            // next item, and any comment trailing that comma.
            return width + line_remainder_width(&parent, config);
        }

        current = parent;
    }

    width
}

/// Returns the width of what is written after `node` before its line ends: the
/// comma separating it from the next item of the list it belongs to, any
/// comment trailing that comma, or nothing if no comma follows.
fn line_remainder_width(node: &SyntaxNode, config: &Config) -> usize {
    let comma = node
        .siblings_with_tokens(Direction::Next)
        // The first sibling yielded is `node` itself, skip this.
        .skip(1)
        .find(|sibling| !sibling.kind().is_trivia())
        .filter(|sibling| sibling.kind() == SyntaxKind::Comma);

    match comma {
        // There could be a comment trailing the comma, so call `first_line_width` to be able to
        // account for that.
        Some(comma) => first_line_width(&Element::cast(comma).into_format_element(), config).0,
        // The last item of a list has no comma of its own, but is given one.
        None if config.trailing_commas
            && node
                .parent()
                .is_some_and(|list| writes_trailing_commas(list.kind())) =>
        {
            ",".len()
        }
        None => 0,
    }
}

/// Returns `true` if a node of `kind` writes each of its items followed by a
/// comma and a line break.
fn writes_trailing_commas(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::CallStatementNode
            | SyntaxKind::LiteralArrayNode
            | SyntaxKind::LiteralMapNode
            | SyntaxKind::LiteralObjectNode
            | SyntaxKind::LiteralStructNode
    )
}

/// Returns `true` if a node of `kind` opens a delimiter that is closed only
/// after all of its children have been written.
fn opens_delimiter(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::CallExprNode
            | SyntaxKind::LiteralArrayNode
            | SyntaxKind::LiteralMapNode
            | SyntaxKind::LiteralObjectNode
            | SyntaxKind::LiteralPairNode
            | SyntaxKind::LiteralStructNode
            | SyntaxKind::ParenthesizedExprNode
            | SyntaxKind::PlaceholderNode
    )
}

/// Returns `true` if a node of `kind` is written as a line of its own, so that
/// the line ends where the node does.
fn ends_line(kind: SyntaxKind) -> bool {
    // Expressions are fragments of a larger line; every other construct the
    // walk climbs through is line-sized. The clauses of an `if` expression
    // break internally, which `starts_new_line` accounts for.
    !(Expr::<SyntaxNode>::can_cast(kind) || kind == SyntaxKind::PlaceholderNode)
}

/// Returns `true` if a new line is started before a child of kind `child`
/// within a node of `parent`.
fn starts_new_line(parent: SyntaxKind, child: SyntaxKind) -> bool {
    // Each clause of an `if` expression is placed on its own line.
    parent == SyntaxKind::IfExprNode
        && matches!(child, SyntaxKind::ThenKeyword | SyntaxKind::ElseKeyword)
}

/// Returns the width of the whitespace a node of `parent` writes after a child
/// of kind `left_child`, before the child that follows it.
fn trailing_space_width(parent: SyntaxKind, left_child: SyntaxKind) -> usize {
    match parent {
        // These are written as an unbroken run of their children.
        SyntaxKind::AccessExprNode
        | SyntaxKind::IndexExprNode
        | SyntaxKind::ParenthesizedExprNode
        | SyntaxKind::PlaceholderNode
        | SyntaxKind::LiteralStringNode => 0,
        // Only the comma between two elements is followed by a space.
        SyntaxKind::CallExprNode | SyntaxKind::LiteralArrayNode | SyntaxKind::LiteralPairNode => {
            usize::from(left_child == SyntaxKind::Comma)
        }
        // Operators are surrounded by spaces, as is anything not handled
        // above, so that this never reports less than the true width.
        _ => SPACE.len(),
    }
}

/// Returns a copy of `config` with no maximum line length, so that nested
/// constructs are laid out flat and the postprocessor inserts no line breaks
/// on account of width.
fn flat_config(config: &Config) -> Config {
    // `Config` is `Copy`, and `MaxLineLength(None)` means "unlimited".
    let mut flat = *config;
    flat.max_line_length =
        MaxLineLength::try_new(None).expect("`None` is a valid maximum line length");
    flat
}

/// Returns the formatted width of the first line `element` would be written
/// across, and whether it would be written across more than one line.
fn first_line_width(element: &FormatElement, config: &Config) -> (usize, bool) {
    let flat_config = flat_config(config);

    let mut raw_pre_stream = TokenStream::<PreToken>::default();
    element.write(&mut raw_pre_stream, &flat_config);

    // Comments preceding the element are written on lines of their own, ahead
    // of the element, and so are not part of its width.
    let mut trimmed_pre_stream = TokenStream::<PreToken>::default();
    for token in raw_pre_stream
        .iter()
        .skip_while(|token| matches!(token, PreToken::Trivia(_) | PreToken::BlankLine))
    {
        trimmed_pre_stream.push(token.clone());
    }
    // `end_line()` here as we are measuring just the width of this element, no
    // additional space.
    trimmed_pre_stream.end_line();

    let post_stream = Postprocessor::default().run(trimmed_pre_stream, &flat_config);
    let mut post_iter = post_stream.iter();

    let width = post_iter
        .by_ref()
        .take_while(|token| !matches!(token, PostToken::Newline))
        .map(|token| token.width(&flat_config))
        .sum();

    // The `end_line()` above guarantees a trailing newline, so tokens left
    // after the first one mean a second line.
    (width, post_iter.next().is_some())
}

/// Sums the rendered width of `stream`, laid out with no width breaks.
fn flat_width(stream: TokenStream<PreToken>, config: &Config) -> usize {
    let config = flat_config(config);

    Postprocessor::default()
        .run(stream, &config)
        .iter()
        .map(|t| t.width(&config))
        .sum()
}

/// Returns the column the next token written to `stream` would occupy, and the
/// number of delimiters left open on that line.
fn line_position(stream: &TokenStream<PreToken>, config: &Config) -> (usize, usize) {
    let mut level = 0usize;
    let mut tail_start = 0usize;
    for (i, t) in stream.iter().enumerate() {
        match t {
            PreToken::IndentStart => level += 1,
            PreToken::IndentEnd => level = level.saturating_sub(1),
            PreToken::LineEnd | PreToken::BlankLine | PreToken::Trivia(_) => tail_start = i + 1,
            _ => {}
        }
    }

    let mut tail = TokenStream::<PreToken>::default();
    let mut open = 0usize;
    for t in stream.iter().skip(tail_start) {
        // The postprocessor line breaks for indent tokens, which we don't want here;
        // `level` calculated above already accounts for indentation.
        if matches!(t, PreToken::IndentStart | PreToken::IndentEnd) {
            continue;
        }
        if let PreToken::Literal(_, kind) = t {
            match kind {
                SyntaxKind::OpenParen
                | SyntaxKind::OpenBracket
                | SyntaxKind::OpenBrace
                | SyntaxKind::PlaceholderOpen => {
                    open += 1;
                }
                SyntaxKind::CloseParen | SyntaxKind::CloseBracket | SyntaxKind::CloseBrace => {
                    open = open.saturating_sub(1);
                }
                _ => {}
            }
        }
        tail.push(t.clone());
    }
    // The `end_line()` below trims any trailing whitespace before its newline,
    // which would drop the space preceding the array literal
    // (`TokenStream::end_line` and `Postprocessor::run`). To avoid this, anchor
    // that whitespace with the array open bracket, then subtract the bracket's
    // width.
    tail.push_literal("[".to_string(), SyntaxKind::OpenBracket);
    tail.end_line();

    let prefix = flat_width(tail, config).saturating_sub("[".len());
    (level * config.indent.num() + prefix, open)
}

/// Formats a [`LiteralMapItem`](wdl_ast::v1::LiteralMapItem).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_literal_map_item(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    let mut children = element.children().expect("literal map item children");

    let key = children.next().expect("literal map item key");
    (&key).write(stream, config);

    let colon = children.next().expect("literal map item colon");
    assert_eq!(colon.element().kind(), SyntaxKind::Colon);
    (&colon).write(stream, config);
    stream.end_word();

    let value = children.next().expect("literal map item value");
    (&value).write(stream, config);
}

/// Formats a [`LiteralMap`](wdl_ast::v1::LiteralMap).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_literal_map(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    let mut children = element.children().expect("literal map children");

    let open_brace = children.next().expect("literal map open brace");
    assert_eq!(open_brace.element().kind(), SyntaxKind::OpenBrace);
    (&open_brace).write(stream, config);
    stream.increment_indent();

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
    while let Some(item) = items.next() {
        (item).write(stream, config);

        if let Some(comma) = commas.next()
            && (items.peek().is_some() || comma.has_comment())
        {
            (comma).write(stream, config);
            if items.peek().is_some() {
                stream.end_line();
            }
        } else if config.trailing_commas {
            stream.push_literal(",".into(), SyntaxKind::Comma);
        }
    }

    stream.decrement_indent();
    (&close_brace.expect("literal map close brace")).write(stream, config);
}

/// Formats a [`LiteralObjectItem`](wdl_ast::v1::LiteralObjectItem).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_literal_object_item(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    let mut children = element.children().expect("literal object item children");

    let key = children.next().expect("literal object item key");
    assert_eq!(key.element().kind(), SyntaxKind::Ident);
    (&key).write(stream, config);

    let colon = children.next().expect("literal object item colon");
    assert_eq!(colon.element().kind(), SyntaxKind::Colon);
    (&colon).write(stream, config);
    stream.end_word();

    let value = children.next().expect("literal object item value");
    (&value).write(stream, config);
}

/// Formats a [`LiteralObject`](wdl_ast::v1::LiteralObject).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_literal_object(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    let mut children = element.children().expect("literal object children");

    let object_keyword = children.next().expect("literal object keyword");
    assert_eq!(object_keyword.element().kind(), SyntaxKind::ObjectKeyword);
    (&object_keyword).write(stream, config);
    stream.end_word();

    let open_brace = children.next().expect("literal object open brace");
    assert_eq!(open_brace.element().kind(), SyntaxKind::OpenBrace);
    (&open_brace).write(stream, config);
    stream.increment_indent();

    let mut members = Vec::new();
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
                members.push(child.to_owned());
            }
        }
    }

    let mut items = members.iter().peekable();
    let mut commas = commas.iter();
    while let Some(item) = items.next() {
        (item).write(stream, config);

        if let Some(comma) = commas.next()
            && (items.peek().is_some() || comma.has_comment())
        {
            (comma).write(stream, config);
            if items.peek().is_some() {
                stream.end_line();
            }
        } else if config.trailing_commas {
            stream.push_literal(",".into(), SyntaxKind::Comma);
        }
    }

    stream.decrement_indent();
    (&close_brace.expect("literal object close brace")).write(stream, config);
}

/// Formats a [`AccessExpr`](wdl_ast::v1::AccessExpr).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_access_expr(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("access expr children") {
        (&child).write(stream, config);
    }
}

/// Formats a [`CallExpr`](wdl_ast::v1::CallExpr).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_call_expr(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("call expr children") {
        (&child).write(stream, config);
        if child.element().kind() == SyntaxKind::Comma {
            stream.end_word();
        }
    }
}

/// Formats an [`IndexExpr`](wdl_ast::v1::IndexExpr).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_index_expr(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("index expr children") {
        (&child).write(stream, config);
    }
}

/// Formats an [`AdditionExpr`](wdl_ast::v1::AdditionExpr).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_addition_expr(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("addition expr children") {
        let whitespace_wrapped = child.element().kind() == SyntaxKind::Plus;
        if whitespace_wrapped {
            stream.end_word();
        }
        (&child).write(stream, config);
        if whitespace_wrapped {
            stream.end_word();
        }
    }
}

/// Formats a [`SubtractionExpr`](wdl_ast::v1::SubtractionExpr).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_subtraction_expr(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("subtraction expr children") {
        let whitespace_wrapped = child.element().kind() == SyntaxKind::Minus;
        if whitespace_wrapped {
            stream.end_word();
        }
        (&child).write(stream, config);
        if whitespace_wrapped {
            stream.end_word();
        }
    }
}

/// Formats a [`MultiplicationExpr`](wdl_ast::v1::MultiplicationExpr).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_multiplication_expr(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("multiplication expr children") {
        let whitespace_wrapped = child.element().kind() == SyntaxKind::Asterisk;
        if whitespace_wrapped {
            stream.end_word();
        }
        (&child).write(stream, config);
        if whitespace_wrapped {
            stream.end_word();
        }
    }
}

/// Formats a [`DivisionExpr`](wdl_ast::v1::DivisionExpr).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_division_expr(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("division expr children") {
        let whitespace_wrapped = child.element().kind() == SyntaxKind::Slash;
        if whitespace_wrapped {
            stream.end_word();
        }
        (&child).write(stream, config);
        if whitespace_wrapped {
            stream.end_word();
        }
    }
}

/// Formats a [`ModuloExpr`](wdl_ast::v1::ModuloExpr).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_modulo_expr(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("modulo expr children") {
        let whitespace_wrapped = child.element().kind() == SyntaxKind::Percent;
        if whitespace_wrapped {
            stream.end_word();
        }
        (&child).write(stream, config);
        if whitespace_wrapped {
            stream.end_word();
        }
    }
}

/// Formats an [`ExponentiationExpr`](wdl_ast::v1::ExponentiationExpr).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_exponentiation_expr(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("exponentiation expr children") {
        let whitespace_wrapped = child.element().kind() == SyntaxKind::Exponentiation;
        if whitespace_wrapped {
            stream.end_word();
        }
        (&child).write(stream, config);
        if whitespace_wrapped {
            stream.end_word();
        }
    }
}

/// Formats a [`LogicalAndExpr`](wdl_ast::v1::LogicalAndExpr).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_logical_and_expr(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("logical and expr children") {
        let whitespace_wrapped = child.element().kind() == SyntaxKind::LogicalAnd;
        if whitespace_wrapped {
            stream.end_word();
        }
        (&child).write(stream, config);
        if whitespace_wrapped {
            stream.end_word();
        }
    }
}

/// Formats a [`LogicalNotExpr`](wdl_ast::v1::LogicalNotExpr).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_logical_not_expr(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    let mut children = element.children().expect("logical not expr children");
    let not = children.next().expect("logical not expr not");
    assert_eq!(not.element().kind(), SyntaxKind::Exclamation);
    (&not).write(stream, config);

    let expr = children.next().expect("logical not expr expr");
    (&expr).write(stream, config);
}

/// Formats a [`LogicalOrExpr`](wdl_ast::v1::LogicalOrExpr).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_logical_or_expr(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("logical or expr children") {
        let whitespace_wrapped = child.element().kind() == SyntaxKind::LogicalOr;
        if whitespace_wrapped {
            stream.end_word();
        }
        (&child).write(stream, config);
        if whitespace_wrapped {
            stream.end_word();
        }
    }
}

/// Formats an [`EqualityExpr`](wdl_ast::v1::EqualityExpr).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_equality_expr(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("equality expr children") {
        let whitespace_wrapped = child.element().kind() == SyntaxKind::Equal;
        if whitespace_wrapped {
            stream.end_word();
        }
        (&child).write(stream, config);
        if whitespace_wrapped {
            stream.end_word();
        }
    }
}

/// Formats a [`InequalityExpr`](wdl_ast::v1::InequalityExpr).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_inequality_expr(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("inequality expr children") {
        let whitespace_wrapped = child.element().kind() == SyntaxKind::NotEqual;
        if whitespace_wrapped {
            stream.end_word();
        }
        (&child).write(stream, config);
        if whitespace_wrapped {
            stream.end_word();
        }
    }
}

/// Formats a [`LessExpr`](wdl_ast::v1::LessExpr).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_less_expr(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("less expr children") {
        let whitespace_wrapped = child.element().kind() == SyntaxKind::Less;
        if whitespace_wrapped {
            stream.end_word();
        }
        (&child).write(stream, config);
        if whitespace_wrapped {
            stream.end_word();
        }
    }
}

/// Formats a [`LessEqualExpr`](wdl_ast::v1::LessEqualExpr).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_less_equal_expr(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("less equal expr children") {
        let whitespace_wrapped = child.element().kind() == SyntaxKind::LessEqual;
        if whitespace_wrapped {
            stream.end_word();
        }
        (&child).write(stream, config);
        if whitespace_wrapped {
            stream.end_word();
        }
    }
}

/// Formats a [`GreaterExpr`](wdl_ast::v1::GreaterExpr).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_greater_expr(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("greater expr children") {
        let whitespace_wrapped = child.element().kind() == SyntaxKind::Greater;
        if whitespace_wrapped {
            stream.end_word();
        }
        (&child).write(stream, config);
        if whitespace_wrapped {
            stream.end_word();
        }
    }
}

/// Formats a [`GreaterEqualExpr`](wdl_ast::v1::GreaterEqualExpr).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_greater_equal_expr(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("greater equal expr children") {
        let whitespace_wrapped = child.element().kind() == SyntaxKind::GreaterEqual;
        if whitespace_wrapped {
            stream.end_word();
        }
        (&child).write(stream, config);
        if whitespace_wrapped {
            stream.end_word();
        }
    }
}

/// Formats a [`ParenthesizedExpr`](wdl_ast::v1::ParenthesizedExpr).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_parenthesized_expr(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    for child in element.children().expect("parenthesized expr children") {
        (&child).write(stream, config);
    }
}

/// Formats an [`IfExpr`](wdl_ast::v1::IfExpr).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_if_expr(
    element: &FormatElement,
    stream: &mut TokenStream<PreToken>,
    config: &Config,
) {
    let in_chain = {
        let mut cur = element.element().inner();
        let mut result = false;
        while let Some(prev) = cur.prev_sibling_or_token() {
            cur = prev;
            if cur.kind().is_trivia() {
                continue;
            }
            // only match on `else`; `then` could be considered for "chaining" but that
            // makes it harder to read IMO (a-frantz).
            result = matches!(cur.kind(), SyntaxKind::ElseKeyword);
            break;
        }
        result
    };

    let mut children = element.children().expect("if expr children").peekable();
    while let Some(child) = children.next() {
        match child.element().kind() {
            SyntaxKind::ThenKeyword => {
                if !in_chain {
                    stream.increment_indent();
                } else {
                    stream.end_line();
                }
            }
            SyntaxKind::ElseKeyword => {
                stream.end_line();
            }
            _ => {}
        }
        (child).write(stream, config);
        if children.peek().is_some() {
            stream.end_word();
        }
    }

    if !in_chain {
        stream.decrement_indent();
    }
}
