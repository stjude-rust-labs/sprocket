use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::v1::Parser as WdlParser;
use crate::v1::Rule;

#[test]
fn it_fails_to_parse_an_empty_string() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::map_literal,
        positives: vec![Rule::map_literal],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_map_literal_with_spaces_outside_the_input() {
    fails_with! {
        parser: WdlParser,
        input: " { hello: true } ",
        rule: Rule::map_literal,
        positives: vec![Rule::map_literal],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_a_map_literal() {
    parses_to! {
        parser: WdlParser,
        input: "{hello:true}",
        rule: Rule::map_literal,
        tokens: [
            map_literal(0, 12, [
                expression_based_kv_pair(1, 11, [
                    expression_based_kv_key(1, 6, [
                        expression(1, 6, [
                            identifier(1, 6)
                        ])
                    ]),
                    kv_value(7, 11, [
                        expression(7, 11, [
                            boolean(7, 11)
                        ])
                    ])
                ])
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_map_literal_with_a_comma() {
    parses_to! {
        parser: WdlParser,
        input: "{hello:true,}",
        rule: Rule::map_literal,
        tokens: [
            map_literal(0, 13, [
                expression_based_kv_pair(1, 11, [
                    expression_based_kv_key(1, 6, [
                        expression(1, 6, [
                            identifier(1, 6)
                        ])
                    ]),
                    kv_value(7, 11, [
                        expression(7, 11, [
                            boolean(7, 11)
                        ])
                    ])
                ]),
                COMMA(11, 12)
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_map_literal_without_the_trailing_space() {
    parses_to! {
        parser: WdlParser,
        input: "{hello:true} ",
        rule: Rule::map_literal,
        tokens: [
            map_literal(0, 12, [
                expression_based_kv_pair(1, 11, [
                    expression_based_kv_key(1, 6, [
                        expression(1, 6, [
                            identifier(1, 6)
                        ])
                    ]),
                    kv_value(7, 11, [
                        expression(7, 11, [
                            boolean(7, 11)
                        ])
                    ])
                ])
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_map_literal_with_spaces_inside() {
    parses_to! {
        parser: WdlParser,
        input: "{ hello : true }",
        rule: Rule::map_literal,
        tokens: [
            map_literal(0, 16, [
                WHITESPACE(1, 2, [SPACE(1, 2)]),
                expression_based_kv_pair(2, 14, [
                    expression_based_kv_key(2, 7, [
                        expression(2, 7, [
                            identifier(2, 7)
                        ])
                    ]),
                    WHITESPACE(7, 8, [SPACE(7, 8)]),
                    WHITESPACE(9, 10, [SPACE(9, 10)]),
                    kv_value(10, 14, [
                        expression(10, 14, [
                            boolean(10, 14)
                        ])
                    ])
                ]),
                WHITESPACE(14, 15, [SPACE(14, 15)]),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_map_literal_with_spaces_inside_and_a_comma() {
    parses_to! {
        parser: WdlParser,
        input: "{ hello : true, }",
        rule: Rule::map_literal,
        tokens: [
            map_literal(0, 17, [
                WHITESPACE(1, 2, [SPACE(1, 2)]),
                expression_based_kv_pair(2, 14, [
                    expression_based_kv_key(2, 7, [
                        expression(2, 7, [
                            identifier(2, 7)
                        ])
                    ]),
                    WHITESPACE(7, 8, [SPACE(7, 8)]),
                    WHITESPACE(9, 10, [SPACE(9, 10)]),
                    kv_value(10, 14, [
                        expression(10, 14, [
                            boolean(10, 14)
                        ])
                    ])
                ]),
                COMMA(14, 15),
                WHITESPACE(15, 16, [SPACE(15, 16)]),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_an_expression_as_the_key() {
    parses_to! {
        parser: WdlParser,
        input: "{ if a then b else c : true }",
        rule: Rule::map_literal,
        tokens: [
            map_literal(0, 29, [
                WHITESPACE(1, 2, [SPACE(1, 2)]),
                expression_based_kv_pair(2, 27, [
                    expression_based_kv_key(2, 20, [
                        expression(2, 20, [
                            r#if(2, 20, [
                                WHITESPACE(4, 5, [SPACE(4, 5)]),
                                expression(5, 6, [
                                    identifier(5, 6)
                                ]),
                                WHITESPACE(6, 7, [SPACE(6, 7)]),
                                WHITESPACE(11, 12, [SPACE(11, 12)]),
                                expression(12, 13, [
                                    identifier(12, 13)
                                ]),
                                WHITESPACE(13, 14, [SPACE(13, 14)]),
                                WHITESPACE(18, 19, [SPACE(18, 19)]),
                                expression(19, 20, [
                                    identifier(19, 20)
                                ]),
                            ])
                        ])
                    ]),
                    WHITESPACE(20, 21, [SPACE(20, 21)]),
                    WHITESPACE(22, 23, [SPACE(22, 23)]),
                    kv_value(23, 27, [
                        expression(23, 27, [
                            boolean(23, 27)
                        ])
                    ])
                ]),
                WHITESPACE(27, 28, [SPACE(27, 28)]),
            ])
        ]
    }
}
