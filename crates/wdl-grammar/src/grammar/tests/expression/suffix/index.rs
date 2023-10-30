use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::Parser as WdlParser;
use crate::Rule;

#[test]
fn it_fails_to_parse_an_empty_index() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::index,
        positives: vec![Rule::index],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_value_that_is_not_index() {
    fails_with! {
        parser: WdlParser,
        input: ".field",
        rule: Rule::index,
        positives: vec![Rule::index],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_an_index() {
    parses_to! {
        parser: WdlParser,
        input: "[if true then a else b]",
        rule: Rule::index,
        tokens: [index(0, 23, [
            expression(1, 22, [
                r#if(1, 22, [
                    WHITESPACE(3, 4, [SPACE(3, 4)]),
                    expression(4, 8, [
                        boolean(4, 8)
                    ]),
                    WHITESPACE(8, 9, [SPACE(8, 9)]),
                    WHITESPACE(13, 14, [SPACE(13, 14)]),
                    expression(14, 15, [
                        identifier(14, 15)
                    ]),
                    WHITESPACE(15, 16, [SPACE(15, 16)]),
                    WHITESPACE(20, 21, [SPACE(20, 21)]),
                    expression(21, 22, [
                        identifier(21, 22)
                    ]),
                ])
            ])
        ])]
    }
}

#[test]
fn it_successfully_parses_an_index_without_including_the_trailing_space() {
    parses_to! {
        parser: WdlParser,
        input: "[if true then a else b] ",
        rule: Rule::index,
        tokens: [index(0, 23, [
            expression(1, 22, [
                r#if(1, 22, [
                    WHITESPACE(3, 4, [SPACE(3, 4)]),
                    expression(4, 8, [
                        boolean(4, 8)
                    ]),
                    WHITESPACE(8, 9, [SPACE(8, 9)]),
                    WHITESPACE(13, 14, [SPACE(13, 14)]),
                    expression(14, 15, [
                        identifier(14, 15)
                    ]),
                    WHITESPACE(15, 16, [SPACE(15, 16)]),
                    WHITESPACE(20, 21, [SPACE(20, 21)]),
                    expression(21, 22, [
                        identifier(21, 22)
                    ]),
                ])
            ])
        ])]
    }
}
