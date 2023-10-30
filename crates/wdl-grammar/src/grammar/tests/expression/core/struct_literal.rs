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
        rule: Rule::struct_literal,
        positives: vec![Rule::identifier],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_struct_literal_with_spaces_outside_the_input() {
    fails_with! {
        parser: WdlParser,
        input: " struct { hello: true } ",
        rule: Rule::struct_literal,
        positives: vec![Rule::identifier],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_a_struct_literal() {
    parses_to! {
        parser: WdlParser,
        input: "struct{hello:true}",
        rule: Rule::struct_literal,
        tokens: [
            struct_literal(0, 18, [
                identifier(0, 6),
                identifier_based_kv_pair(7, 17, [
                    identifier_based_kv_key(7, 12, [
                        identifier(7, 12),
                    ]),
                    kv_value(13, 17, [
                        expression(13, 17, [
                            boolean(13, 17)
                        ]),
                    ])
                ])
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_struct_literal_with_a_comma() {
    parses_to! {
        parser: WdlParser,
        input: "struct{hello:true,}",
        rule: Rule::struct_literal,
        tokens: [
            struct_literal(0, 19, [
                identifier(0, 6),
                identifier_based_kv_pair(7, 17, [
                    identifier_based_kv_key(7, 12, [
                        identifier(7, 12),
                    ]),
                    kv_value(13, 17, [
                        expression(13, 17, [
                            boolean(13, 17)
                        ]),
                    ])
                ]),
                COMMA(17, 18)
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_struct_literal_without_the_trailing_space() {
    parses_to! {
        parser: WdlParser,
        input: "struct{hello:true} ",
        rule: Rule::struct_literal,
        tokens: [
            struct_literal(0, 18, [
                identifier(0, 6),
                identifier_based_kv_pair(7, 17, [
                    identifier_based_kv_key(7, 12, [
                        identifier(7, 12),
                    ]),
                    kv_value(13, 17, [
                        expression(13, 17, [
                            boolean(13, 17)
                        ]),
                    ])
                ])
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_struct_literal_with_spaces_inside() {
    parses_to! {
        parser: WdlParser,
        input: "struct { hello : true }",
        rule: Rule::struct_literal,
        tokens: [
            struct_literal(0, 23, [
                identifier(0, 6),
                WHITESPACE(6, 7, [SPACE(6, 7)]),
                WHITESPACE(8, 9, [SPACE(8, 9)]),
                identifier_based_kv_pair(9, 21, [
                    identifier_based_kv_key(9, 14, [
                        identifier(9, 14),
                    ]),
                    WHITESPACE(14, 15, [SPACE(14, 15)]),
                    WHITESPACE(16, 17, [SPACE(16, 17)]),
                    kv_value(17, 21, [
                        expression(17, 21, [
                            boolean(17, 21)
                        ]),
                    ]),
                ]),
                WHITESPACE(21, 22, [SPACE(21, 22)]),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_struct_literal_with_spaces_inside_and_a_comma() {
    parses_to! {
        parser: WdlParser,
        input: "struct { hello : true, }",
        rule: Rule::struct_literal,
        tokens: [
            struct_literal(0, 24, [
                identifier(0, 6),
                WHITESPACE(6, 7, [SPACE(6, 7)]),
                WHITESPACE(8, 9, [SPACE(8, 9)]),
                identifier_based_kv_pair(9, 21, [
                    identifier_based_kv_key(9, 14, [
                        identifier(9, 14),
                    ]),
                    WHITESPACE(14, 15, [SPACE(14, 15)]),
                    WHITESPACE(16, 17, [SPACE(16, 17)]),
                    kv_value(17, 21, [
                        expression(17, 21, [
                            boolean(17, 21)
                        ]),
                    ]),
                ]),
                COMMA(21, 22),
                WHITESPACE(22, 23, [SPACE(22, 23)]),
            ])
        ]
    }
}
