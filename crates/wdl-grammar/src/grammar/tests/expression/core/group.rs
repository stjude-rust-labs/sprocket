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
        rule: Rule::group,
        positives: vec![Rule::group],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_group_with_spaces_outside_the_group() {
    fails_with! {
        parser: WdlParser,
        input: " (hello) ",
        rule: Rule::group,
        positives: vec![Rule::group],
        negatives: vec![],
        pos: 0
    }
}

/// According to the specification, a group _must_ include an expression—it
/// cannot be empty.
#[test]
fn it_fails_to_parse_an_empty_group() {
    fails_with! {
        parser: WdlParser,
        input: "()",
        rule: Rule::group,
        positives: vec![Rule::WHITESPACE, Rule::COMMENT, Rule::expression],
        negatives: vec![],
        pos: 1
    }
}

#[test]
fn it_successfully_parses_a_group() {
    parses_to! {
        parser: WdlParser,
        input: "(hello)",
        rule: Rule::group,
        tokens: [
            group(0, 7, [
                expression(1, 6, [
                    core(1, 6, [
                        literal(1, 6, [
                            identifier(1, 6)
                        ])
                    ])
                ]),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_group_with_spaces() {
    parses_to! {
        parser: WdlParser,
        input: "( hello )",
        rule: Rule::group,
        tokens: [
            group(0, 9, [
                WHITESPACE(1, 2, [INDENT(1, 2, [SPACE(1, 2)])]),
                expression(2, 7, [
                    core(2, 7, [
                        literal(2, 7, [
                            identifier(2, 7)
                        ])
                    ])
                ]),
                WHITESPACE(7, 8, [INDENT(7, 8, [SPACE(7, 8)])]),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_group_without_including_the_trailing_space() {
    parses_to! {
        parser: WdlParser,
        input: "(hello) ",
        rule: Rule::group,
        tokens: [
            group(0, 7, [
                expression(1, 6, [
                    core(1, 6, [
                        literal(1, 6, [
                            identifier(1, 6)
                        ])
                    ])
                ]),
            ])
        ]
    }
}
