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
        rule: Rule::pair_literal,
        positives: vec![Rule::pair_literal],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_pair_literal_with_spaces_outside_the_input() {
    fails_with! {
        parser: WdlParser,
        input: " (a, b) ",
        rule: Rule::pair_literal,
        positives: vec![Rule::pair_literal],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_pair_literal_with_one_member() {
    fails_with! {
        parser: WdlParser,
        input: "(a)",
        rule: Rule::pair_literal,
        positives: vec![
            Rule::WHITESPACE,
            Rule::COMMENT,
            Rule::COMMA,
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
        pos: 2
    }
}

#[test]
fn it_fails_to_parse_a_pair_literal_with_three_members() {
    fails_with! {
        parser: WdlParser,
        input: "(a, b, c)",
        rule: Rule::pair_literal,
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
        pos: 5
    }
}

#[test]
fn it_successfully_parses_a_pair_literal() {
    parses_to! {
        parser: WdlParser,
        input: "(a,b)",
        rule: Rule::pair_literal,
        tokens: [
            pair_literal(0, 5, [
                expression(1, 2, [
                    identifier(1, 2)
                ]),
                COMMA(2, 3),
                expression(3, 4, [
                    identifier(3, 4)
                ]),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_pair_literal_without_the_trailing_space() {
    parses_to! {
        parser: WdlParser,
        input: "(a,b) ",
        rule: Rule::pair_literal,
        tokens: [
            pair_literal(0, 5, [
                expression(1, 2, [
                    identifier(1, 2)
                ]),
                COMMA(2, 3),
                expression(3, 4, [
                    identifier(3, 4)
                ]),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_pair_literal_with_spaces_inside() {
    parses_to! {
        parser: WdlParser,
        input: "(a, b)",
        rule: Rule::pair_literal,
        tokens: [
            pair_literal(0, 6, [
                expression(1, 2, [
                    identifier(1, 2)
                ]),
                COMMA(2, 3),
                WHITESPACE(3, 4, [SPACE(3, 4)]),
                expression(4, 5, [
                    identifier(4, 5)
                ]),
            ])
        ]
    }
}
