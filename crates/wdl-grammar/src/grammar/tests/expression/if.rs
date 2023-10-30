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
                WHITESPACE(2, 3),
                expression(3, 7, [
                    core(3, 7, [
                        literal(3, 7, [
                            boolean(3, 7)
                        ])
                    ])
                ]),
                WHITESPACE(7, 8),
                WHITESPACE(12, 13),
                expression(13, 14, [
                    core(13, 14, [
                        literal(13, 14, [
                            identifier(13, 14)
                        ])
                    ])
                ]),
                WHITESPACE(14, 15),
                WHITESPACE(19, 20),
                expression(20, 21, [
                    core(20, 21, [
                        literal(20, 21, [
                            identifier(20, 21)
                        ])
                    ])
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
                WHITESPACE(2, 3),
                expression(3, 7, [
                    core(3, 7, [
                        literal(3, 7, [
                            boolean(3, 7)
                        ])
                    ])
                ]),
                WHITESPACE(7, 8),
                WHITESPACE(12, 13),
                expression(13, 14, [
                    core(13, 14, [
                        literal(13, 14, [
                            identifier(13, 14)
                        ])
                    ])
                ]),
                WHITESPACE(14, 15),
                WHITESPACE(19, 20),
                expression(20, 21, [
                    core(20, 21, [
                        literal(20, 21, [
                            identifier(20, 21)
                        ])
                    ])
                ]),
            ])
        ]
    }
}