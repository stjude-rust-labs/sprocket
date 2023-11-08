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
        rule: Rule::struct_literal,
        positives: vec![Rule::struct_literal],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_struct_literal_with_spaces_outside_the_expression() {
    fails_with! {
        parser: WdlParser,
        input: " struct { hello: true } ",
        rule: Rule::struct_literal,
        positives: vec![Rule::struct_literal],
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
                identifier(7, 12),
                expression(13, 17, [
                    core(13, 17, [
                        literal(13, 17, [
                            boolean(13, 17)
                        ])
                    ])
                ]),
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
                identifier(7, 12),
                expression(13, 17, [
                    core(13, 17, [
                        literal(13, 17, [
                            boolean(13, 17)
                        ])
                    ])
                ]),
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
                WHITESPACE(6, 7),
                WHITESPACE(8, 9),
                identifier(9, 14),
                WHITESPACE(14, 15),
                WHITESPACE(16, 17),
                expression(17, 21, [
                    core(17, 21, [
                        literal(17, 21, [
                            boolean(17, 21)
                        ])
                    ])
                ]),
                WHITESPACE(21, 22),
            ])
        ]
    }
}
