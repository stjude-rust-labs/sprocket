//! A lint rule for spacing of expressions.

use rowan::Direction;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::ToSpan;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;
use wdl_ast::v1::Expr;
use wdl_ast::v1::LiteralExpr;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the expression spacing rule.
const ID: &str = "ExpressionSpacing";

/// Reports disallowed whitespace after prefix operators.
fn prefix_whitespace(span: Span) -> Diagnostic {
    Diagnostic::note("prefix operators may not contain whitespace")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("remove the internal whitespace")
}

/// Reports missing following whitespace around operators
fn missing_surrounding_whitespace(span: Span) -> Diagnostic {
    Diagnostic::note("operators must be surrounded by whitespace")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("add a space before and after this operator")
}

/// Reports missing preceding whitespace around operators
fn missing_preceding_whitespace(span: Span) -> Diagnostic {
    Diagnostic::note("operators must be preceded by whitespace")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("add a space before this operator")
}

/// Reports missing following whitespace around operators
fn missing_following_whitespace(span: Span) -> Diagnostic {
    Diagnostic::note("operators must be followed by whitespace")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("add a space after this operator")
}

/// Report disallowed space
fn disallowed_space(span: Span) -> Diagnostic {
    Diagnostic::note("this space is not allowed")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("remove the space")
}

/// Reports missing preceding whitespace around assignments
fn assignment_missing_preceding_whitespace(span: Span) -> Diagnostic {
    Diagnostic::note("assignments must be preceded by whitespace")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("add a space before this assignment")
}

/// Reports missing following whitespace around assignments
fn assignment_missing_following_whitespace(span: Span) -> Diagnostic {
    Diagnostic::note("assignments must be followed by whitespace")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("add a space after this assignment")
}

/// Reports missing following whitespace around assignments
fn assignment_missing_surrounding_whitespace(span: Span) -> Diagnostic {
    Diagnostic::note("assignments must be surrounded by whitespace")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("add a space before and after this assignment")
}

/// Reports missing open paren for multiline if...then...else constructs
fn multiline_if_open_paren(span: Span) -> Diagnostic {
    Diagnostic::note("multi-line if...then...else must have a preceding parenthesis and newline")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("add a open parenthesis and newline prior to this if")
}

/// Reports missing newline prior to then in multiline if...then...else
/// constructs
fn multiline_then_space(span: Span) -> Diagnostic {
    Diagnostic::note("multi-line if...then...else must have a preceding space")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("add a newline before the then keyword")
}

/// Reports missing newline prior to else in multiline if...then...else
/// constructs
fn multiline_else_space(span: Span) -> Diagnostic {
    Diagnostic::note("multi-line if...then...else must have a preceding space")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("add a newline before the else keyword")
}

/// Reports missing close paren for multiline if...then...else constructs
fn multiline_if_close_paren(span: Span) -> Diagnostic {
    Diagnostic::note("multi-line if...then...else must have a following newline and parenthesis")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("add a newline and close parenthesis after to this else clause")
}

/// Reports missing newline following open brace/bracket/paren for multiline
/// array/map/object literals
fn multiline_literal_open_newline(span: Span) -> Diagnostic {
    Diagnostic::note(
        "multi-line array/map/object literals must have a newline following the opening token",
    )
    .with_rule(ID)
    .with_highlight(span)
    .with_fix("add a newline after this token")
}

/// Reports missing newline before close brace/bracket/paren for multiline
/// array/map/object literals
fn multiline_literal_close_newline(span: Span) -> Diagnostic {
    Diagnostic::note(
        "multi-line array/map/object literals must have a newline preceding the closing token",
    )
    .with_rule(ID)
    .with_highlight(span)
    .with_fix("add a newline before this token")
}

/// Detects improperly spaced expressions.
#[derive(Default, Debug, Clone, Copy)]
pub struct ExpressionSpacingRule;

impl Rule for ExpressionSpacingRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that WDL expressions are properly spaced."
    }

    fn explanation(&self) -> &'static str {
        "Proper spacing is important for readability and consistency. This rule ensures that \
         expressions are spaced properly.
         
         The following tokens should be surrounded by whitespace when used as an infix: `=`, `==`, \
         `!=`, `&&`, `||`, `<`, `<=`, `>`, `>=`, `+`, `-`, `*`, `/`, and `%`.
         
         The following tokens should not be followed by whitespace when used as a prefix: `-`, and \
         `!`.
         
         Opening brackets (`(`, `[`, and `{`) should not be followed by a space, but may be \
         followed by a newline. Closing brackets (`)`, `]`, and `}`) should not be preceded by a \
         space, but may be preceded by a newline.
         
         Sometimes a long expression will exceed the maximum line width. In these cases, one or \
         more linebreaks must be introduced. Line continuations should be indented one more level \
         than the beginning of the expression. There should never be more than one level of \
         indentation change per-line.
         
         If bracketed content (things between `()`, `[]`, or `{}`) must be split onto multiple \
         lines, a newline should follow the opening bracket, the contents should be indented an \
         additional level, then the closing bracket should be de-indented to match the indentation \
         of the opening bracket. If you are line splitting an expression on an infix operator, the \
         operator and at least the beginning of the RHS operand should be on the continued line. \
         (i.e. an operator should not be on a line by itself.)
         
         If you are using the `if...then...else...` construct as part of your expression and it \
         needs to be line split, the entire construct should be wrapped in parentheses (`()`). The \
         opening parenthesis should be immediately followed by a newline. `if`, `then`, and `else` \
         should all start a line one more level of indentation than the wrapping parentheses. The \
         closing parenthesis should be on the same level of indentation as the opening \
         parenthesis. If you are using the `if...then...else...` construct on one line, it does \
         not need to be wrapped in parentheses. However, if any of the 3 clauses are more complex \
         than a single identifier, they should be wrapped in parentheses.
         
         Sometimes a developer will choose to line split an expression despite it being able to \
         all fit on one line that is <=90 characters wide. That is perfectly acceptable. There is \
         'wiggle' room allowed by the above rules. This is intentional, and allows developers to \
         choose a more compact or a more spaced out expression."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Spacing])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        None
    }
}

impl Visitor for ExpressionSpacingRule {
    type State = Diagnostics;

    fn document(
        &mut self,
        _: &mut Self::State,
        reason: VisitReason,
        _: &Document,
        _: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Reset the visitor upon document entry
        *self = Default::default();
    }

    fn expr(&mut self, state: &mut Self::State, reason: VisitReason, expr: &Expr) {
        if reason == VisitReason::Exit {
            return;
        }

        match expr {
            Expr::LogicalNot(_) | Expr::Negation(_) => {
                // No following spacing allowed
                if expr
                    .syntax()
                    .children_with_tokens()
                    .filter(|t| t.kind() == SyntaxKind::Whitespace)
                    .count()
                    > 0
                {
                    state.exceptable_add(
                        prefix_whitespace(expr.syntax().text_range().to_span()),
                        SyntaxElement::from(expr.syntax().clone()),
                        &self.exceptable_nodes(),
                    );
                }
            }
            Expr::Parenthesized(_) => {
                // Find the actual open and close parentheses
                let open = expr
                    .syntax()
                    .children_with_tokens()
                    .find(|t| t.kind() == SyntaxKind::OpenParen)
                    .expect("parenthesized expression should have an opening parenthesis");
                let close = expr
                    .syntax()
                    .children_with_tokens()
                    .find(|t| t.kind() == SyntaxKind::CloseParen)
                    .expect("parenthesized expression should have an closing parenthesis");

                // The opening parenthesis can be preceded by whitespace, another open
                // parenthesis, or a negation (!). The parenthesized expression
                // can be the first thing at its level if it is wrapped in a
                // EqualityExpressionNode.
                let mut prev = expr.syntax().prev_sibling_or_token();
                if prev.is_none() {
                    // No prior elements, so we need to go up a level.
                    if let Some(parent) = expr.syntax().parent() {
                        if let Some(parent_prev) = parent.prev_sibling_or_token() {
                            prev = Some(parent_prev);
                        }
                    } else {
                        unreachable!(
                            "parenthesized expression should have a prior sibling or a parent"
                        );
                    }
                }

                if let Some(prev) = prev {
                    match prev.kind() {
                        SyntaxKind::Whitespace
                        | SyntaxKind::OpenParen
                        | SyntaxKind::NegationExprNode
                        | SyntaxKind::Exclamation
                        | SyntaxKind::NameRefNode // Function calls can precede without whitespace
                        | SyntaxKind::PlaceholderOpen // Opening placeholders can precede a paren
                        | SyntaxKind::Plus // This and all below will report on those tokens.
                        | SyntaxKind::Minus
                        | SyntaxKind::Asterisk
                        | SyntaxKind::Exponentiation
                        | SyntaxKind::Slash
                        | SyntaxKind::Less
                        | SyntaxKind::LessEqual
                        | SyntaxKind::Greater
                        | SyntaxKind::GreaterEqual
                        | SyntaxKind::Percent
                        | SyntaxKind::LogicalAnd
                        | SyntaxKind::LogicalOr => {}
                        _ => {
                            // opening parens should be preceded by whitespace
                            state.exceptable_add(missing_preceding_whitespace(open.text_range().to_span()), SyntaxElement::from(expr.syntax().clone()), &self.exceptable_nodes());
                        }
                    }
                }

                // Opening parenthesis cannot be followed by a space, but can be followed by a
                // newline. Except in the case of an in-line comment.
                if let Some(open_next) = open.next_sibling_or_token() {
                    if open_next.kind() == SyntaxKind::Whitespace {
                        let token = open_next.as_token().expect("should be a token");
                        if token.text().starts_with(' ')
                            && token
                                .next_sibling_or_token()
                                .is_some_and(|t| t.kind() != SyntaxKind::Comment)
                        {
                            // opening parens should not be followed by non-newline whitespace
                            state.exceptable_add(
                                disallowed_space(token.text_range().to_span()),
                                SyntaxElement::from(expr.syntax().clone()),
                                &self.exceptable_nodes(),
                            );
                        }
                    }
                }

                // Closing parenthesis should not be preceded by a space, but can be preceded by
                // a newline.
                if let Some(close_prev) = close.prev_sibling_or_token() {
                    if close_prev.kind() == SyntaxKind::Whitespace
                        && !close_prev
                            .as_token()
                            .expect("should be a token")
                            .text()
                            .contains('\n')
                    {
                        // closing parenthesis should not be preceded by whitespace without a
                        // newline
                        state.exceptable_add(
                            disallowed_space(close_prev.text_range().to_span()),
                            SyntaxElement::from(expr.syntax().clone()),
                            &self.exceptable_nodes(),
                        );
                    }
                }
            }
            Expr::LogicalAnd(_) | Expr::LogicalOr(_) => {
                // find the operator
                let op = expr
                    .syntax()
                    .children_with_tokens()
                    .find(|t| matches!(t.kind(), SyntaxKind::LogicalAnd | SyntaxKind::LogicalOr))
                    .expect("expression node should have an operator");

                check_required_surrounding_ws(state, &op, &self.exceptable_nodes());
            }
            Expr::Equality(_) | Expr::Inequality(_) => {
                // find the operator
                let op = expr
                    .syntax()
                    .children_with_tokens()
                    .find(|t| matches!(t.kind(), SyntaxKind::Equal | SyntaxKind::NotEqual))
                    .expect("expression node should have an operator");

                check_required_surrounding_ws(state, &op, &self.exceptable_nodes());
            }
            Expr::Addition(_)
            | Expr::Subtraction(_)
            | Expr::Multiplication(_)
            | Expr::Division(_)
            | Expr::Modulo(_)
            | Expr::Exponentiation(_) => {
                // find the operator
                let op = expr
                    .syntax()
                    .children_with_tokens()
                    .find(|t| {
                        matches!(
                            t.kind(),
                            SyntaxKind::Plus
                                | SyntaxKind::Minus
                                | SyntaxKind::Asterisk
                                | SyntaxKind::Slash
                                | SyntaxKind::Percent
                                | SyntaxKind::Exponentiation
                        )
                    })
                    .expect("expression node should have an operator");

                // Infix operators must be surrounded by whitespace
                check_required_surrounding_ws(state, &op, &self.exceptable_nodes());
            }
            Expr::Less(_) | Expr::LessEqual(_) | Expr::Greater(_) | Expr::GreaterEqual(_) => {
                // find the operator
                let op = expr
                    .syntax()
                    .children_with_tokens()
                    .find(|t| {
                        matches!(
                            t.kind(),
                            SyntaxKind::Less
                                | SyntaxKind::LessEqual
                                | SyntaxKind::Greater
                                | SyntaxKind::GreaterEqual
                        )
                    })
                    .expect("expression node should have an operator");

                check_required_surrounding_ws(state, &op, &self.exceptable_nodes());
            }
            Expr::If(_) => {
                // find the if keyword
                let if_keyword = expr
                    .syntax()
                    .children_with_tokens()
                    .find(|t| t.kind() == SyntaxKind::IfKeyword)
                    .expect("if expression node should have an if keyword");
                let then_keyword = expr
                    .syntax()
                    .children_with_tokens()
                    .find(|t| t.kind() == SyntaxKind::ThenKeyword)
                    .expect("if expression node should have a then keyword");
                let else_keyword = expr
                    .syntax()
                    .children_with_tokens()
                    .find(|t| t.kind() == SyntaxKind::ElseKeyword)
                    .expect("if expression node should have an else keyword");

                let newlines = expr
                    .syntax()
                    .descendants_with_tokens()
                    .filter(|t| {
                        if t.kind() == SyntaxKind::Whitespace
                            && t.as_token()
                                .expect("should be a token")
                                .text()
                                .contains('\n')
                        {
                            return true;
                        }
                        false
                    })
                    .count();

                // If..then..else expression contains newlines, so we need to check the
                // formatting.
                if newlines > 0 {
                    // If expression should be preceded by a opening parenthesis and a newline (plus
                    // indentation whitespace).
                    let prior = expr.syntax().siblings_with_tokens(Direction::Prev).skip(1);

                    let mut newline = false;
                    let mut open_paren = false;

                    for t in prior {
                        // if keyword in a multi-line if..then..else should only be preceded by
                        // whitespace, an opening parenthesis, or a comment.
                        if !matches!(
                            t.kind(),
                            SyntaxKind::Whitespace | SyntaxKind::OpenParen | SyntaxKind::Comment
                        ) {
                            // if should be preceded by an opening parenthesis and a newline
                            break;
                        } else if t.kind() == SyntaxKind::Whitespace
                            && t.as_token()
                                .expect("should be a token")
                                .text()
                                .contains('\n')
                        {
                            newline = true;
                        } else if t.kind() == SyntaxKind::OpenParen {
                            open_paren = true;
                            break;
                        }
                    }
                    if !open_paren || !newline {
                        state.exceptable_add(
                            multiline_if_open_paren(if_keyword.text_range().to_span()),
                            SyntaxElement::from(expr.syntax().clone()),
                            &self.exceptable_nodes(),
                        );
                    }

                    // check the then keyword
                    let then_ws = then_keyword
                        .prev_sibling_or_token()
                        .expect("should have a prior sibling");

                    if then_ws.kind() != SyntaxKind::Whitespace
                        || !then_ws
                            .as_token()
                            .expect("should be a token")
                            .text()
                            .contains('\n')
                    {
                        // then should be preceded by a newline
                        state.exceptable_add(
                            multiline_then_space(then_keyword.text_range().to_span()),
                            SyntaxElement::from(expr.syntax().clone()),
                            &self.exceptable_nodes(),
                        );
                    }

                    // else keyword should be preceded by a newline
                    let else_prior = else_keyword
                        .prev_sibling_or_token()
                        .expect("should have a prior sibling");
                    if else_prior.kind() != SyntaxKind::Whitespace
                        || !else_prior
                            .as_token()
                            .expect("should be a token")
                            .text()
                            .contains('\n')
                    {
                        // then should be preceded by a newline
                        state.exceptable_add(
                            multiline_else_space(else_keyword.text_range().to_span()),
                            SyntaxElement::from(expr.syntax().clone()),
                            &self.exceptable_nodes(),
                        );
                    }

                    // check the closing parenthesis
                    let next_tokens = expr.syntax().siblings_with_tokens(Direction::Next).skip(1);

                    let mut newline = false;
                    let mut close_paren = false;
                    for t in next_tokens {
                        // else keyword in a multi-line if..then..else should only be followed by
                        // whitespace or a comment, then a closing parenthesis.
                        if !matches!(
                            t.kind(),
                            SyntaxKind::Whitespace | SyntaxKind::CloseParen | SyntaxKind::Comment
                        ) {
                            // if should be preceded by an closing parenthesis and a newline
                            break;
                        } else if t.kind() == SyntaxKind::Whitespace
                            && t.as_token()
                                .expect("should be a token")
                                .text()
                                .contains('\n')
                        {
                            newline = true;
                        } else if t.kind() == SyntaxKind::CloseParen {
                            close_paren = true;
                            break;
                        }
                    }
                    if !close_paren || !newline {
                        state.exceptable_add(
                            multiline_if_close_paren(else_keyword.text_range().to_span()),
                            SyntaxElement::from(expr.syntax().clone()),
                            &self.exceptable_nodes(),
                        );
                    }
                }
            }
            Expr::Index(_) => {
                let open_bracket = expr
                    .syntax()
                    .children_with_tokens()
                    .find(|t| t.kind() == SyntaxKind::OpenBracket)
                    .expect("index expression node should have an opening bracket");
                let close_bracket = expr
                    .syntax()
                    .children_with_tokens()
                    .find(|t| t.kind() == SyntaxKind::CloseBracket)
                    .expect("index expression node should have a closing bracket");

                let checks = [
                    open_bracket
                        .prev_sibling_or_token()
                        .filter(|t| t.kind() == SyntaxKind::Whitespace),
                    open_bracket
                        .next_sibling_or_token()
                        .filter(|t| t.kind() == SyntaxKind::Whitespace),
                    close_bracket
                        .prev_sibling_or_token()
                        .filter(|t| t.kind() == SyntaxKind::Whitespace),
                    close_bracket
                        .next_sibling_or_token()
                        .filter(|t| t.kind() == SyntaxKind::Whitespace),
                ];

                checks.iter().for_each(|f| {
                    if let Some(ws) = f {
                        state.exceptable_add(
                            disallowed_space(ws.text_range().to_span()),
                            SyntaxElement::from(expr.syntax().clone()),
                            &self.exceptable_nodes(),
                        );
                    }
                });
            }
            Expr::Access(acc) => {
                let op = acc
                    .syntax()
                    .children_with_tokens()
                    .find(|t| t.kind() == SyntaxKind::Dot)
                    .expect("access expression node should have a dot operator");
                let before_ws = op
                    .prev_sibling_or_token()
                    .filter(|t| t.kind() == SyntaxKind::Whitespace);
                let after_ws = op
                    .next_sibling_or_token()
                    .filter(|t| t.kind() == SyntaxKind::Whitespace);

                if let Some(ws) = before_ws {
                    state.exceptable_add(
                        disallowed_space(ws.text_range().to_span()),
                        SyntaxElement::from(acc.syntax().clone()),
                        &self.exceptable_nodes(),
                    );
                }
                if let Some(ws) = after_ws {
                    state.exceptable_add(
                        disallowed_space(ws.text_range().to_span()),
                        SyntaxElement::from(acc.syntax().clone()),
                        &self.exceptable_nodes(),
                    );
                }
            }
            Expr::Literal(l) => {
                match l {
                    LiteralExpr::Array(_) | LiteralExpr::Map(_) | LiteralExpr::Object(_) => {
                        let newlines = l
                            .syntax()
                            .descendants_with_tokens()
                            .filter(|t| {
                                if t.kind() == SyntaxKind::Whitespace
                                    && t.as_token()
                                        .expect("should be a token")
                                        .text()
                                        .contains('\n')
                                {
                                    return true;
                                }
                                false
                            })
                            .count();

                        if newlines > 0 {
                            // Find the opening and closing brackets
                            let open_bracket = expr
                                .syntax()
                                .children_with_tokens()
                                .find(|t| {
                                    matches!(
                                        t.kind(),
                                        SyntaxKind::OpenBracket
                                            | SyntaxKind::OpenBrace
                                            | SyntaxKind::OpenParen
                                    )
                                })
                                .expect("literal expression node should have an opening bracket");

                            let close_bracket = expr
                                .syntax()
                                .children_with_tokens()
                                .find(|t| {
                                    matches!(
                                        t.kind(),
                                        SyntaxKind::CloseBracket
                                            | SyntaxKind::CloseBrace
                                            | SyntaxKind::CloseParen
                                    )
                                })
                                .expect("literal expression node should have a closing bracket");

                            let open_bracket_next = open_bracket
                                .as_token()
                                .expect("should be a token")
                                .siblings_with_tokens(Direction::Next)
                                .skip(1);

                            let mut newline = false;
                            for t in open_bracket_next {
                                // The opening bracket/brace/paren must be followed by a `\n`, with
                                // optional additional whitespace or comment tokens.
                                if !matches!(t.kind(), SyntaxKind::Whitespace | SyntaxKind::Comment)
                                {
                                    break;
                                } else if t.kind() == SyntaxKind::Whitespace
                                    && t.as_token()
                                        .expect("should be a token")
                                        .text()
                                        .contains('\n')
                                {
                                    newline = true;
                                    break;
                                }
                            }
                            if !newline {
                                state.exceptable_add(
                                    multiline_literal_open_newline(
                                        open_bracket.text_range().to_span(),
                                    ),
                                    SyntaxElement::from(expr.syntax().clone()),
                                    &self.exceptable_nodes(),
                                );
                            }

                            let close_bracket_prior = close_bracket
                                .as_token()
                                .expect("should be a token")
                                .siblings_with_tokens(Direction::Prev)
                                .skip(1);

                            let mut newline = false;
                            for t in close_bracket_prior {
                                // The closing bracket/brace/paren must be preceded by a `\n`, with
                                // optional additional whitespace or comment tokens.
                                if !matches!(t.kind(), SyntaxKind::Whitespace | SyntaxKind::Comment)
                                {
                                    // if should be preceded by an opening parenthesis and a newline
                                    break;
                                } else if t.kind() == SyntaxKind::Whitespace
                                    && t.as_token()
                                        .expect("should be a token")
                                        .text()
                                        .contains('\n')
                                {
                                    newline = true;
                                    break;
                                }
                            }
                            if !newline {
                                state.exceptable_add(
                                    multiline_literal_close_newline(
                                        close_bracket.text_range().to_span(),
                                    ),
                                    SyntaxElement::from(expr.syntax().clone()),
                                    &self.exceptable_nodes(),
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
            // Expr::Literal, Expr::Name, Expr::Call
            _ => {}
        }
    }

    fn bound_decl(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        decl: &wdl_ast::v1::BoundDecl,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Some(assign) = decl
            .syntax()
            .descendants_with_tokens()
            .find(|t| t.kind() == SyntaxKind::Assignment)
        {
            let before_ws =
                assign.prev_sibling_or_token().map(|t| t.kind()) == Some(SyntaxKind::Whitespace);
            let after_ws =
                assign.next_sibling_or_token().map(|t| t.kind()) == Some(SyntaxKind::Whitespace);

            if !before_ws && !after_ws {
                // assignments must be surrounded by whitespace
                state.exceptable_add(
                    assignment_missing_surrounding_whitespace(assign.text_range().to_span()),
                    SyntaxElement::from(decl.syntax().clone()),
                    &self.exceptable_nodes(),
                );
            } else if !before_ws {
                // assignments must be preceded by whitespace
                state.exceptable_add(
                    assignment_missing_preceding_whitespace(assign.text_range().to_span()),
                    SyntaxElement::from(decl.syntax().clone()),
                    &self.exceptable_nodes(),
                );
            } else if !after_ws {
                // assignments must be followed by whitespace
                state.exceptable_add(
                    assignment_missing_following_whitespace(assign.text_range().to_span()),
                    SyntaxElement::from(decl.syntax().clone()),
                    &self.exceptable_nodes(),
                );
            }
        }
    }
}

/// Checks to ensure a token is surrounded by whitespace.
fn check_required_surrounding_ws(
    state: &mut Diagnostics,
    op: &SyntaxElement,
    exceptable_nodes: &Option<&'static [SyntaxKind]>,
) {
    let before_ws = op.prev_sibling_or_token().map(|t| t.kind()) == Some(SyntaxKind::Whitespace);
    let after_ws = op.next_sibling_or_token().map(|t| t.kind()) == Some(SyntaxKind::Whitespace);

    if !before_ws && !after_ws {
        // must be surrounded by whitespace
        state.exceptable_add(
            missing_surrounding_whitespace(op.text_range().to_span()),
            op.clone(),
            exceptable_nodes,
        );
    } else if !before_ws {
        // must be preceded by whitespace
        state.exceptable_add(
            missing_preceding_whitespace(op.text_range().to_span()),
            op.clone(),
            exceptable_nodes,
        );
    } else if !after_ws {
        // must be followed by whitespace
        state.exceptable_add(
            missing_following_whitespace(op.text_range().to_span()),
            op.clone(),
            exceptable_nodes,
        );
    }
}
