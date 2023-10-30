use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::Parser as WdlParser;
use crate::Rule;

#[test]
fn it_fails_to_parse_an_empty_string() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::r#if,
        positives: vec![Rule::r#if],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_an_if_statement_with_no_spaces_between_the_if_and_the_identifier() {
    fails_with! {
        parser: WdlParser,
        input: "iftrue then a else b",
        rule: Rule::r#if,
        positives: vec![Rule::WHITESPACE, Rule::COMMENT],
        negatives: vec![],
        pos: 2
    }
}

#[test]
fn it_fails_to_parse_an_if_statement_with_no_spaces_between_the_identifier_and_the_then() {
    fails_with! {
        parser: WdlParser,
        input: "if truethen a else b",
        rule: Rule::r#if,
        positives: vec![Rule::WHITESPACE, Rule::COMMENT],
        negatives: vec![],
        pos: 12
    }
}

#[test]
fn it_fails_to_parse_an_if_statement_with_no_spaces_between_the_then_and_the_expression() {
    fails_with! {
        parser: WdlParser,
        input: "if true thena else b",
        rule: Rule::r#if,
        positives: vec![Rule::WHITESPACE, Rule::COMMENT],
        negatives: vec![],
        pos: 12
    }
}

#[test]
fn it_fails_to_parse_an_if_statement_with_no_spaces_between_the_expression_and_the_else() {
    fails_with! {
        parser: WdlParser,
        input: "if true then aelse b",
        rule: Rule::r#if,
        positives: vec![
            Rule::WHITESPACE,
            Rule::COMMENT,
            Rule::or,
            Rule::and,
            Rule::add,
            Rule::sub,
            Rule::mul,
            Rule::div,
            Rule::remainder,
            Rule::eq,
            Rule::neq,
            Rule::lte,
            Rule::gte,
            Rule::lt,
            Rule::gt,
            Rule::member,
            Rule::index,
            Rule::apply,
        ],
        negatives: vec![],
        pos: 19
    }
}

#[test]
fn it_fails_to_parse_an_if_statement_with_no_spaces_between_the_else_and_the_expression() {
    fails_with! {
        parser: WdlParser,
        input: "if true then a elseb",
        rule: Rule::r#if,
        positives: vec![Rule::WHITESPACE, Rule::COMMENT],
        negatives: vec![],
        pos: 19
    }
}

#[test]
fn it_fails_to_parse_an_if_statement_with_spaces_outside_the_group() {
    fails_with! {
        parser: WdlParser,
        input: " if true then a else b ",
        rule: Rule::r#if,
        positives: vec![Rule::r#if],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_an_if_statement() {
    parses_to! {
        parser: WdlParser,
        input: "if true then a else b",
        rule: Rule::r#if,
        tokens: [
            r#if(0, 21, [
                WHITESPACE(2, 3, [SPACE(2, 3)]),
                expression(3, 7, [
                    boolean(3, 7)
                ]),
                WHITESPACE(7, 8, [SPACE(7, 8)]),
                WHITESPACE(12, 13, [SPACE(12, 13)]),
                expression(13, 14, [
                    identifier(13, 14)
                ]),
                WHITESPACE(14, 15, [SPACE(14, 15)]),
                WHITESPACE(19, 20, [SPACE(19, 20)]),
                expression(20, 21, [
                    identifier(20, 21)
                ]),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_an_if_statement_without_including_the_trailing_space() {
    parses_to! {
        parser: WdlParser,
        input: "if true then a else b ",
        rule: Rule::r#if,
        tokens: [
            r#if(0, 21, [
                WHITESPACE(2, 3, [SPACE(2, 3)]),
                expression(3, 7, [
                    boolean(3, 7)
                ]),
                WHITESPACE(7, 8, [SPACE(7, 8)]),
                WHITESPACE(12, 13, [SPACE(12, 13)]),
                expression(13, 14, [
                    identifier(13, 14)
                ]),
                WHITESPACE(14, 15, [SPACE(14, 15)]),
                WHITESPACE(19, 20, [SPACE(19, 20)]),
                expression(20, 21, [
                    identifier(20, 21)
                ]),
            ])
        ]
    }
}
