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
        rule: Rule::array_literal,
        positives: vec![Rule::array_literal],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_an_array_literal_with_spaces_outside_the_input() {
    fails_with! {
        parser: WdlParser,
        input: " [if a then b else c, \"Hello, world!\"] ",
        rule: Rule::array_literal,
        positives: vec![Rule::array_literal],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_an_array_literal() {
    parses_to! {
        parser: WdlParser,
        input: "[if a then b else c,\"Hello, world!\"] ",
        rule: Rule::array_literal,
        tokens: [
            array_literal(0, 36, [
                expression(1, 19, [
                    core(1, 19, [
                        r#if(1, 19, [
                            WHITESPACE(3, 4, [INDENT(3, 4, [SPACE(3, 4)])]),
                            expression(4, 5, [
                                core(4, 5, [
                                    literal(4, 5, [
                                        identifier(4, 5)
                                    ])
                                ])
                            ]),
                            WHITESPACE(5, 6, [INDENT(5, 6, [SPACE(5, 6)])]),
                            WHITESPACE(10, 11, [INDENT(10, 11, [SPACE(10, 11)])]),
                            expression(11, 12, [
                                core(11, 12, [
                                    literal(11, 12, [
                                        identifier(11, 12)
                                    ])
                                ])
                            ]),
                            WHITESPACE(12, 13, [INDENT(12, 13, [SPACE(12, 13)])]),
                            WHITESPACE(17, 18, [INDENT(17, 18, [SPACE(17, 18)])]),
                            expression(18, 19, [
                                core(18, 19, [
                                    literal(18, 19, [
                                        identifier(18, 19)
                                    ])
                                ])
                            ])
                        ])
                    ])
                ]),
                expression(20, 35, [
                    core(20, 35, [
                        literal(20, 35, [
                            string(20, 35, [
                                double_quoted_string(20, 35)
                            ])
                        ])
                    ])
                ])
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_an_array_literal_without_the_trailing_space() {
    parses_to! {
        parser: WdlParser,
        input: "[if a then b else c, \"Hello, world!\"] ",
        rule: Rule::array_literal,
        tokens: [
            array_literal(0, 37, [
                expression(1, 19, [
                    core(1, 19, [
                        r#if(1, 19, [
                            WHITESPACE(3, 4, [INDENT(3, 4, [SPACE(3, 4)])]),
                            expression(4, 5, [
                                core(4, 5, [
                                    literal(4, 5, [
                                        identifier(4, 5)
                                    ])
                                ])
                            ]),
                            WHITESPACE(5, 6, [INDENT(5, 6, [SPACE(5, 6)])]),
                            WHITESPACE(10, 11, [INDENT(10, 11, [SPACE(10, 11)])]),
                            expression(11, 12, [
                                core(11, 12, [
                                    literal(11, 12, [
                                        identifier(11, 12)
                                    ])
                                ])
                            ]),
                            WHITESPACE(12, 13, [INDENT(12, 13, [SPACE(12, 13)])]),
                            WHITESPACE(17, 18, [INDENT(17, 18, [SPACE(17, 18)])]),
                            expression(18, 19, [
                                core(18, 19, [
                                    literal(18, 19, [
                                        identifier(18, 19)
                                    ])
                                ])
                            ])
                        ])
                    ])
                ]),
                WHITESPACE(20, 21, [INDENT(20, 21, [SPACE(20, 21)])]),
                expression(21, 36, [
                    core(21, 36, [
                        literal(21, 36, [
                            string(21, 36, [
                                double_quoted_string(21, 36)
                            ])
                        ])
                    ])
                ])
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_an_array_literal_with_spaces_inside() {
    parses_to! {
        parser: WdlParser,
        input: "[if a then b else c, \"Hello, world!\"]",
        rule: Rule::array_literal,
        tokens: [
            array_literal(0, 37, [
                expression(1, 19, [
                    core(1, 19, [
                        r#if(1, 19, [
                            WHITESPACE(3, 4, [INDENT(3, 4, [SPACE(3, 4)])]),
                            expression(4, 5, [
                                core(4, 5, [
                                    literal(4, 5, [
                                        identifier(4, 5)
                                    ])
                                ])
                            ]),
                            WHITESPACE(5, 6, [INDENT(5, 6, [SPACE(5, 6)])]),
                            WHITESPACE(10, 11, [INDENT(10, 11, [SPACE(10, 11)])]),
                            expression(11, 12, [
                                core(11, 12, [
                                    literal(11, 12, [
                                        identifier(11, 12)
                                    ])
                                ])
                            ]),
                            WHITESPACE(12, 13, [INDENT(12, 13, [SPACE(12, 13)])]),
                            WHITESPACE(17, 18, [INDENT(17, 18, [SPACE(17, 18)])]),
                            expression(18, 19, [
                                core(18, 19, [
                                    literal(18, 19, [
                                        identifier(18, 19)
                                    ])
                                ])
                            ])
                        ])
                    ])
                ]),
                WHITESPACE(20, 21, [INDENT(20, 21, [SPACE(20, 21)])]),
                expression(21, 36, [
                    core(21, 36, [
                        literal(21, 36, [
                            string(21, 36, [
                                double_quoted_string(21, 36)
                            ])
                        ])
                    ])
                ])
            ])
        ]
    }
}
