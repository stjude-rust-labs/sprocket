use pest::consumes_to;
use pest::parses_to;

use crate::Parser as WdlParser;
use crate::Rule;

mod array_literal;
mod group;
mod r#if;
mod map_literal;
mod object_literal;
mod pair_literal;
mod struct_literal;

#[test]
fn it_successfully_parses_an_array_literal_with_spaces_inside() {
    parses_to! {
        parser: WdlParser,
        input: "[if a then b else c, \"Hello, world!\"]",
        rule: Rule::core,
        tokens: [
            core(0, 37, [
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
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_group_with_spaces() {
    parses_to! {
        parser: WdlParser,
        input: "( hello )",
        rule: Rule::core,
        tokens: [
            core(0, 9, [
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

#[test]
fn it_successfully_parses_an_if_statement() {
    parses_to! {
        parser: WdlParser,
        input: "if true then a else b",
        rule: Rule::core,
        tokens: [
            core(0, 21, [
                r#if(0, 21, [
                    WHITESPACE(2, 3, [INDENT(2, 3, [SPACE(2, 3)])]),
                    expression(3, 7, [
                        core(3, 7, [
                            literal(3, 7, [
                                boolean(3, 7)
                            ])
                        ])
                    ]),
                    WHITESPACE(7, 8, [INDENT(7, 8, [SPACE(7, 8)])]),
                    WHITESPACE(12, 13, [INDENT(12, 13, [SPACE(12, 13)])]),
                    expression(13, 14, [
                        core(13, 14, [
                            literal(13, 14, [
                                identifier(13, 14)
                            ])
                        ])
                    ]),
                    WHITESPACE(14, 15, [INDENT(14, 15, [SPACE(14, 15)])]),
                    WHITESPACE(19, 20, [INDENT(19, 20, [SPACE(19, 20)])]),
                    expression(20, 21, [
                        core(20, 21, [
                            literal(20, 21, [
                                identifier(20, 21)
                            ])
                        ])
                    ]),
                ])
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_map_with_an_expression_as_the_key() {
    parses_to! {
        parser: WdlParser,
        input: "{ if a then b else c : true }",
        rule: Rule::core,
        tokens: [
            core(0, 29, [
                map_literal(0, 29, [
                    WHITESPACE(1, 2, [INDENT(1, 2, [SPACE(1, 2)])]),
                    expression_based_kv_pair(2, 27, [
                        expression_based_kv_key(2, 20, [
                            expression(2, 20, [
                                core(2, 20, [
                                    r#if(2, 20, [
                                        WHITESPACE(4, 5, [INDENT(4, 5, [SPACE(4, 5)])]),
                                        expression(5, 6, [
                                            core(5, 6, [
                                                literal(5, 6, [
                                                    identifier(5, 6)
                                                ])
                                            ])
                                        ]),
                                        WHITESPACE(6, 7, [INDENT(6, 7, [SPACE(6, 7)])]),
                                        WHITESPACE(11, 12, [INDENT(11, 12, [SPACE(11, 12)])]),
                                        expression(12, 13, [
                                            core(12, 13, [
                                                literal(12, 13, [
                                                    identifier(12, 13)
                                                ])
                                            ])
                                        ]),
                                        WHITESPACE(13, 14, [INDENT(13, 14, [SPACE(13, 14)])]),
                                        WHITESPACE(18, 19, [INDENT(18, 19, [SPACE(18, 19)])]),
                                        expression(19, 20, [
                                            core(19, 20, [
                                                literal(19, 20, [
                                                    identifier(19, 20)
                                                ])
                                            ])
                                        ]),
                                    ])
                                ])
                            ])
                        ]),
                        WHITESPACE(20, 21, [INDENT(20, 21, [SPACE(20, 21)])]),
                        WHITESPACE(22, 23, [INDENT(22, 23, [SPACE(22, 23)])]),
                        kv_value(23, 27, [
                            expression(23, 27, [
                                core(23, 27, [
                                    literal(23, 27, [
                                        boolean(23, 27)
                                    ])
                                ])
                            ])
                        ])
                    ]),
                    WHITESPACE(27, 28, [INDENT(27, 28, [SPACE(27, 28)])]),
                ])
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_an_object_literal_with_spaces_inside_and_a_comma() {
    parses_to! {
        parser: WdlParser,
        input: "object { hello : true, }",
        rule: Rule::core,
        tokens: [
            core(0, 24, [
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
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_pair_literal_with_spaces_inside() {
    parses_to! {
        parser: WdlParser,
        input: "(a, b)",
        rule: Rule::core,
        tokens: [
            core(0, 6, [
                pair_literal(0, 6, [
                    expression(1, 2, [
                        core(1, 2, [
                            literal(1, 2, [
                                identifier(1, 2)
                            ])
                        ])
                    ]),
                    WHITESPACE(3, 4, [INDENT(3, 4, [SPACE(3, 4)])]),
                    expression(4, 5, [
                        core(4, 5, [
                            literal(4, 5, [
                                identifier(4, 5)
                            ])
                        ])
                    ]),
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
        rule: Rule::core,
        tokens: [
            core(0, 19, [
                struct_literal(0, 19, [
                    identifier(0, 6),
                    identifier_based_kv_pair(7, 17, [
                        identifier_based_kv_key(7, 12, [
                            identifier(7, 12),
                        ]),
                        kv_value(13, 17, [
                            expression(13, 17, [
                                core(13, 17, [
                                    literal(13, 17, [
                                        boolean(13, 17)
                                    ])
                                ])
                            ]),
                        ])
                    ]),
                    COMMA(17, 18)
                ])
            ])
        ]
    }
}
