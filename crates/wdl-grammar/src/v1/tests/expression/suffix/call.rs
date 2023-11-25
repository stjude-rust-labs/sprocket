use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::v1::Parser as WdlParser;
use crate::v1::Rule;

#[test]
fn it_fails_to_parse_an_empty_call() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::call,
        positives: vec![Rule::call],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_an_apply_with_just_a_comma() {
    fails_with! {
        parser: WdlParser,
        input: "(,)",
        rule: Rule::call,
        positives: vec![Rule::WHITESPACE, Rule::COMMENT, Rule::expression],
        negatives: vec![],
        pos: 1
    }
}

#[test]
fn it_fails_to_parse_a_value_that_is_not_call() {
    fails_with! {
        parser: WdlParser,
        input: ".field",
        rule: Rule::call,
        positives: vec![Rule::call],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_an_apply_with_no_elements() {
    parses_to! {
        parser: WdlParser,
        input: "()",
        rule: Rule::call,
        tokens: [call(0, 2)]
    }
}

#[test]
fn it_successfully_parses_an_apply_with_one_element() {
    parses_to! {
        parser: WdlParser,
        input: "(if true then a else b)",
        rule: Rule::call,
        tokens: [call(0, 23, [
            expression(1, 22, [
                r#if(1, 22, [
                    WHITESPACE(3, 4, [SPACE(3, 4)]),
                    expression(4, 8, [
                        boolean(4, 8)
                    ]),
                    WHITESPACE(8, 9, [SPACE(8, 9)]),
                    WHITESPACE(13, 14, [SPACE(13, 14)]),
                    expression(14, 15, [
                        singular_identifier(14, 15)
                    ]),
                    WHITESPACE(15, 16, [SPACE(15, 16)]),
                    WHITESPACE(20, 21, [SPACE(20, 21)]),
                    expression(21, 22, [
                        singular_identifier(21, 22)
                    ]),
                ])
            ]),
        ])]
    }
}

#[test]
fn it_successfully_parses_an_apply_with_one_element_and_a_comma() {
    parses_to! {
        parser: WdlParser,
        input: "(if true then a else b,)",
        rule: Rule::call,
        tokens: [call(0, 24, [
            expression(1, 22, [
                r#if(1, 22, [
                    WHITESPACE(3, 4, [SPACE(3, 4)]),
                    expression(4, 8, [
                        boolean(4, 8)
                    ]),
                    WHITESPACE(8, 9, [SPACE(8, 9)]),
                    WHITESPACE(13, 14, [SPACE(13, 14)]),
                    expression(14, 15, [
                        singular_identifier(14, 15)
                    ]),
                    WHITESPACE(15, 16, [SPACE(15, 16)]),
                    WHITESPACE(20, 21, [SPACE(20, 21)]),
                    expression(21, 22, [
                        singular_identifier(21, 22)
                    ]),
                ])
            ]),
            COMMA(22, 23),
        ])]
    }
}

#[test]
fn it_successfully_parses_an_apply_with_one_element_without_including_the_trailing_space() {
    parses_to! {
        parser: WdlParser,
        input: "(if true then a else b) ",
        rule: Rule::call,
        tokens: [call(0, 23, [
            expression(1, 22, [
                r#if(1, 22, [
                    WHITESPACE(3, 4, [SPACE(3, 4)]),
                    expression(4, 8, [
                        boolean(4, 8)
                    ]),
                    WHITESPACE(8, 9, [SPACE(8, 9)]),
                    WHITESPACE(13, 14, [SPACE(13, 14)]),
                    expression(14, 15, [
                        singular_identifier(14, 15)
                    ]),
                    WHITESPACE(15, 16, [SPACE(15, 16)]),
                    WHITESPACE(20, 21, [SPACE(20, 21)]),
                    expression(21, 22, [
                        singular_identifier(21, 22)
                    ]),
                ])
            ]),
        ])]
    }
}

#[test]
fn it_successfully_parses_an_apply_with_two_elements() {
    parses_to! {
        parser: WdlParser,
        input: "(if true then a else b, world)",
        rule: Rule::call,
        tokens: [
            call(0, 30, [
                expression(1, 22, [
                    r#if(1, 22, [
                        WHITESPACE(3, 4, [SPACE(3, 4)]),
                        expression(4, 8, [
                            boolean(4, 8)
                        ]),
                        WHITESPACE(8, 9, [SPACE(8, 9)]),
                        WHITESPACE(13, 14, [SPACE(13, 14)]),
                        expression(14, 15, [
                            singular_identifier(14, 15)
                        ]),
                        WHITESPACE(15, 16, [SPACE(15, 16)]),
                        WHITESPACE(20, 21, [SPACE(20, 21)]),
                        expression(21, 22, [
                            singular_identifier(21, 22)
                        ]),
                    ])
                ]),
                COMMA(22, 23),
                WHITESPACE(23, 24, [SPACE(23, 24)]),
                expression(24, 29, [
                    singular_identifier(24, 29)
                ])
            ]),
        ]
    }
}
