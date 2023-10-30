use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::Parser as WdlParser;
use crate::Rule;

#[test]
fn it_fails_to_parse_an_empty_apply() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::apply,
        positives: vec![Rule::apply],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_an_apply_with_no_elements() {
    fails_with! {
        parser: WdlParser,
        input: "()",
        rule: Rule::apply,
        positives: vec![Rule::WHITESPACE, Rule::COMMENT, Rule::expression],
        negatives: vec![],
        pos: 1
    }
}

#[test]
fn it_fails_to_parse_an_apply_with_just_a_comma() {
    fails_with! {
        parser: WdlParser,
        input: "(,)",
        rule: Rule::apply,
        positives: vec![Rule::WHITESPACE, Rule::COMMENT, Rule::expression],
        negatives: vec![],
        pos: 1
    }
}

#[test]
fn it_fails_to_parse_a_value_that_is_not_apply() {
    fails_with! {
        parser: WdlParser,
        input: ".field",
        rule: Rule::apply,
        positives: vec![Rule::apply],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_an_apply_with_one_element() {
    parses_to! {
        parser: WdlParser,
        input: "(if true then a else b)",
        rule: Rule::apply,
        tokens: [apply(0, 23, [
            expression(1, 22, [
                core(1, 22, [
                    r#if(1, 22, [
                        WHITESPACE(3, 4, [INDENT(3, 4, [SPACE(3, 4)])]),
                        expression(4, 8, [
                            core(4, 8, [
                                literal(4, 8, [
                                    boolean(4, 8)
                                ])
                            ])
                        ]),
                        WHITESPACE(8, 9, [INDENT(8, 9, [SPACE(8, 9)])]),
                        WHITESPACE(13, 14, [INDENT(13, 14, [SPACE(13, 14)])]),
                        expression(14, 15, [
                            core(14, 15, [
                                literal(14, 15, [
                                    identifier(14, 15)
                                ])
                            ])
                        ]),
                        WHITESPACE(15, 16, [INDENT(15, 16, [SPACE(15, 16)])]),
                        WHITESPACE(20, 21, [INDENT(20, 21, [SPACE(20, 21)])]),
                        expression(21, 22, [
                            core(21, 22, [
                                literal(21, 22, [
                                    identifier(21, 22)
                                ])
                            ])
                        ]),
                    ])
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
        rule: Rule::apply,
        tokens: [apply(0, 24, [
            expression(1, 22, [
                core(1, 22, [
                    r#if(1, 22, [
                        WHITESPACE(3, 4, [INDENT(3, 4, [SPACE(3, 4)])]),
                        expression(4, 8, [
                            core(4, 8, [
                                literal(4, 8, [
                                    boolean(4, 8)
                                ])
                            ])
                        ]),
                        WHITESPACE(8, 9, [INDENT(8, 9, [SPACE(8, 9)])]),
                        WHITESPACE(13, 14, [INDENT(13, 14, [SPACE(13, 14)])]),
                        expression(14, 15, [
                            core(14, 15, [
                                literal(14, 15, [
                                    identifier(14, 15)
                                ])
                            ])
                        ]),
                        WHITESPACE(15, 16, [INDENT(15, 16, [SPACE(15, 16)])]),
                        WHITESPACE(20, 21, [INDENT(20, 21, [SPACE(20, 21)])]),
                        expression(21, 22, [
                            core(21, 22, [
                                literal(21, 22, [
                                    identifier(21, 22)
                                ])
                            ])
                        ]),
                    ])
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
        rule: Rule::apply,
        tokens: [apply(0, 23, [
            expression(1, 22, [
                core(1, 22, [
                    r#if(1, 22, [
                        WHITESPACE(3, 4, [INDENT(3, 4, [SPACE(3, 4)])]),
                        expression(4, 8, [
                            core(4, 8, [
                                literal(4, 8, [
                                    boolean(4, 8)
                                ])
                            ])
                        ]),
                        WHITESPACE(8, 9, [INDENT(8, 9, [SPACE(8, 9)])]),
                        WHITESPACE(13, 14, [INDENT(13, 14, [SPACE(13, 14)])]),
                        expression(14, 15, [
                            core(14, 15, [
                                literal(14, 15, [
                                    identifier(14, 15)
                                ])
                            ])
                        ]),
                        WHITESPACE(15, 16, [INDENT(15, 16, [SPACE(15, 16)])]),
                        WHITESPACE(20, 21, [INDENT(20, 21, [SPACE(20, 21)])]),
                        expression(21, 22, [
                            core(21, 22, [
                                literal(21, 22, [
                                    identifier(21, 22)
                                ])
                            ])
                        ]),
                    ])
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
        rule: Rule::apply,
        tokens: [apply(0, 30, [
            expression(1, 22, [
                core(1, 22, [
                    r#if(1, 22, [
                        WHITESPACE(3, 4, [INDENT(3, 4, [SPACE(3, 4)])]),
                        expression(4, 8, [
                            core(4, 8, [
                                literal(4, 8, [
                                    boolean(4, 8)
                                ])
                            ])
                        ]),
                        WHITESPACE(8, 9, [INDENT(8, 9, [SPACE(8, 9)])]),
                        WHITESPACE(13, 14, [INDENT(13, 14, [SPACE(13, 14)])]),
                        expression(14, 15, [
                            core(14, 15, [
                                literal(14, 15, [
                                    identifier(14, 15)
                                ])
                            ])
                        ]),
                        WHITESPACE(15, 16, [INDENT(15, 16, [SPACE(15, 16)])]),
                        WHITESPACE(20, 21, [INDENT(20, 21, [SPACE(20, 21)])]),
                        expression(21, 22, [
                            core(21, 22, [
                                literal(21, 22, [
                                    identifier(21, 22)
                                ])
                            ])
                        ]),
                    ])
                ])
            ]),
            COMMA(22, 23),
            WHITESPACE(23, 24, [INDENT(23, 24, [SPACE(23, 24)])]),
            expression(24, 29, [
                core(24, 29, [
                    literal(24, 29, [
                        identifier(24, 29)
                    ])
                ])
            ])
        ])]
    }
}
