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
        rule: Rule::object_literal,
        positives: vec![Rule::object_literal],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_an_object_literal_with_spaces_outside_the_input() {
    fails_with! {
        parser: WdlParser,
        input: " object { hello: true } ",
        rule: Rule::object_literal,
        positives: vec![Rule::object_literal],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_an_object_literal() {
    parses_to! {
        parser: WdlParser,
        input: "object{hello:true}",
        rule: Rule::object_literal,
        tokens: [
            object_literal(0, 18, [
                identifier_based_kv_pair(7, 17, [
                    identifier_based_kv_key(7, 12, [
                        identifier(7, 12)
                    ]),
                    kv_value(13, 17, [
                        expression(13, 17, [
                            core(13, 17, [
                                literal(13, 17, [
                                    boolean(13, 17)
                                ])
                            ])
                        ])
                    ])
                ])
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_an_object_literal_with_a_comma() {
    parses_to! {
        parser: WdlParser,
        input: "object{hello:true,}",
        rule: Rule::object_literal,
        tokens: [
            object_literal(0, 19, [
                identifier_based_kv_pair(7, 17, [
                    identifier_based_kv_key(7, 12, [
                        identifier(7, 12)
                    ]),
                    kv_value(13, 17, [
                        expression(13, 17, [
                            core(13, 17, [
                                literal(13, 17, [
                                    boolean(13, 17)
                                ])
                            ])
                        ])
                    ]),
                ]),
                COMMA(17, 18)
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_an_object_literal_without_the_trailing_space() {
    parses_to! {
        parser: WdlParser,
        input: "object{hello:true} ",
        rule: Rule::object_literal,
        tokens: [
            object_literal(0, 18, [
                identifier_based_kv_pair(7, 17, [
                    identifier_based_kv_key(7, 12, [
                        identifier(7, 12)
                    ]),
                    kv_value(13, 17, [
                        expression(13, 17, [
                            core(13, 17, [
                                literal(13, 17, [
                                    boolean(13, 17)
                                ])
                            ])
                        ])
                    ])
                ])
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_an_object_literal_with_spaces_inside() {
    parses_to! {
        parser: WdlParser,
        input: "object { hello : true }",
        rule: Rule::object_literal,
        tokens: [
            object_literal(0, 23, [
                WHITESPACE(6, 7, [INDENT(6, 7, [SPACE(6, 7)])]),
                WHITESPACE(8, 9, [INDENT(8, 9, [SPACE(8, 9)])]),
                identifier_based_kv_pair(9, 21, [
                    identifier_based_kv_key(9, 14, [
                        identifier(9, 14)
                    ]),
                    WHITESPACE(14, 15, [INDENT(14, 15, [SPACE(14, 15)])]),
                    WHITESPACE(16, 17, [INDENT(16, 17, [SPACE(16, 17)])]),
                    kv_value(17, 21, [
                        expression(17, 21, [
                            core(17, 21, [
                                literal(17, 21, [
                                    boolean(17, 21)
                                ])
                            ])
                        ])
                    ])
                ]),
                WHITESPACE(21, 22, [INDENT(21, 22, [SPACE(21, 22)])]),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_an_object_literal_with_spaces_inside_and_a_comma() {
    parses_to! {
        parser: WdlParser,
        input: "object { hello : true, }",
        rule: Rule::object_literal,
        tokens: [
            object_literal(0, 24, [
                WHITESPACE(6, 7, [INDENT(6, 7, [SPACE(6, 7)])]),
                WHITESPACE(8, 9, [INDENT(8, 9, [SPACE(8, 9)])]),
                identifier_based_kv_pair(9, 21, [
                    identifier_based_kv_key(9, 14, [
                        identifier(9, 14)
                    ]),
                    WHITESPACE(14, 15, [INDENT(14, 15, [SPACE(14, 15)])]),
                    WHITESPACE(16, 17, [INDENT(16, 17, [SPACE(16, 17)])]),
                    kv_value(17, 21, [
                        expression(17, 21, [
                            core(17, 21, [
                                literal(17, 21, [
                                    boolean(17, 21)
                                ])
                            ])
                        ])
                    ])
                ]),
                COMMA(21, 22),
                WHITESPACE(22, 23, [INDENT(22, 23, [SPACE(22, 23)])]),
            ])
        ]
    }
}
