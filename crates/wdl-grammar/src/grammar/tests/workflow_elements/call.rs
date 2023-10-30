use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::Parser as WdlParser;
use crate::Rule;

#[test]
fn it_fails_to_parse_an_empty_call() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::workflow_call,
        positives: vec![Rule::workflow_call],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_just_call() {
    fails_with! {
        parser: WdlParser,
        input: "call ",
        rule: Rule::workflow_call,
        positives: vec![
            Rule::WHITESPACE,
            Rule::COMMENT,
            Rule::identifier,
        ],
        negatives: vec![],
        pos: 5
    }
}

#[test]
fn it_successfully_parses_plain_call() {
    parses_to! {
        parser: WdlParser,
        input: "call my_task",
        rule: Rule::workflow_call,
        tokens: [workflow_call(0, 12, [
            WHITESPACE(4, 5, [SPACE(4, 5)]), identifier(5, 12)
        ])]
    }
}

#[test]
fn it_successfully_excludes_trailing_whitespace() {
    parses_to! {
        parser: WdlParser,
        input: "call my_task   ",
        rule: Rule::workflow_call,
        tokens: [workflow_call(0, 12, [
            WHITESPACE(4, 5, [SPACE(4, 5)]), identifier(5, 12)
        ])]
    }
}

#[test]
fn it_successfully_parses_call_with_empty_body() {
    parses_to! {
        parser: WdlParser,
        input: "call my_task{}",
        rule: Rule::workflow_call,
        tokens: [workflow_call(0, 14, [
            WHITESPACE(4, 5, [SPACE(4, 5)]),
            identifier(5, 12),
            workflow_call_body(12, 14)
        ])]
    }
}

#[test]
fn it_successfully_parses_call_with_implicitly_declared_input() {
    parses_to! {
        parser: WdlParser,
        input: "call my_task{input:a}",
        rule: Rule::workflow_call,
        tokens: [workflow_call(0, 21, [
            WHITESPACE(4, 5, [SPACE(4, 5)]),
            identifier(5, 12),
            workflow_call_body(12, 21, [
                workflow_call_input(19, 20, [identifier(19, 20)])
            ])
        ])]
    }
}

#[test]
fn it_successfully_parses_call_with_implicitly_declared_input_without_trailing_whitespace() {
    parses_to! {
        parser: WdlParser,
        input: "call my_task{input:a} ",
        rule: Rule::workflow_call,
        tokens: [workflow_call(0, 21, [
            WHITESPACE(4, 5, [SPACE(4, 5)]),
            identifier(5, 12),
            workflow_call_body(12, 21, [
                workflow_call_input(19, 20, [identifier(19, 20)])
            ])
        ])]
    }
}

#[test]
fn it_successfully_parses_call_with_explicitly_declared_input() {
    parses_to! {
        parser: WdlParser,
        input: "call my_task{input:a=b}",
        rule: Rule::workflow_call,
        tokens: [
            // `call my_task{input:a=b}`
            workflow_call(0, 23, [
                WHITESPACE(4, 5, [SPACE(4, 5)]),
                // `my_task`
                identifier(5, 12),
                // `{input:a=b}`
                workflow_call_body(12, 23, [
                    // `a=b`
                    workflow_call_input(19, 22, [
                        // `a`
                        identifier(19, 20),
                        // `b`
                        expression(21, 22, [
                            // `b`
                            identifier(21, 22),
                        ]),
                    ]),
                ]),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_call_with_explicitly_declared_input_without_trailing_whitespace() {
    parses_to! {
        parser: WdlParser,
        input: "call my_task{input:a=b} ",
        rule: Rule::workflow_call,
        tokens: [
            // `call my_task{input:a=b}`
            workflow_call(0, 23, [
                WHITESPACE(4, 5, [SPACE(4, 5)]),
                // `my_task`
                identifier(5, 12),
                // `{input:a=b}`
                workflow_call_body(12, 23, [
                    // `a=b`
                    workflow_call_input(19, 22, [
                        // `a`
                        identifier(19, 20),
                        // `b`
                        expression(21, 22, [
                            // `b`
                            identifier(21, 22),
                        ]),
                    ]),
                ]),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_call_with_multiple_inputs() {
    parses_to! {
        parser: WdlParser,
        input: "call my_task{input:a,b=b,c=z}",
        rule: Rule::workflow_call,
        tokens: [workflow_call(0, 29, [
            WHITESPACE(4, 5, [SPACE(4, 5)]),
            identifier(5, 12),
            workflow_call_body(12, 29, [
                workflow_call_input(19, 20, [identifier(19, 20)]),
                COMMA(20, 21),
                workflow_call_input(21, 24, [identifier(21, 22), expression(23, 24, [
                    identifier(23, 24)
                ])]),
                COMMA(24, 25),
                workflow_call_input(25, 28, [identifier(25, 26), expression(27, 28, [
                    identifier(27, 28)
                ])]),
            ])
        ])]
    }
}

#[test]
fn it_successfully_parses_call_with_multiple_inputs_without_trailing_whitespace() {
    parses_to! {
        parser: WdlParser,
        input: "call my_task{input:a,b=b,c=z} ",
        rule: Rule::workflow_call,
        tokens: [workflow_call(0, 29, [
            WHITESPACE(4, 5, [SPACE(4, 5)]),
            identifier(5, 12),
            workflow_call_body(12, 29, [
                workflow_call_input(19, 20, [identifier(19, 20)]),
                COMMA(20, 21),
                workflow_call_input(21, 24, [identifier(21, 22), expression(23, 24, [
                    identifier(23, 24)
                ])]),
                COMMA(24, 25),
                workflow_call_input(25, 28, [identifier(25, 26), expression(27, 28, [
                    identifier(27, 28)
                ])]),
            ])
        ])]
    }
}

#[test]
fn it_successfully_parses_call_with_as() {
    parses_to! {
        parser: WdlParser,
        input: "call my_task as different_task",
        rule: Rule::workflow_call,
        tokens: [workflow_call(0, 30, [
            WHITESPACE(4, 5, [SPACE(4, 5)]),
            identifier(5, 12),
            WHITESPACE(12, 13, [SPACE(12, 13)]),
            workflow_call_as(13, 30, [
                WHITESPACE(15, 16, [SPACE(15, 16)]),
                identifier(16, 30)
            ])
        ])]
    }
}

#[test]
fn it_successfully_parses_call_with_after() {
    parses_to! {
        parser: WdlParser,
        input: "call imported_doc.my_task after different_task",
        rule: Rule::workflow_call,
        tokens: [workflow_call(0, 46, [
            WHITESPACE(4, 5, [SPACE(4, 5)]),
            qualified_identifier(5, 25, [
                identifier(5, 17),
                identifier(18, 25)
            ]),
            WHITESPACE(25, 26, [SPACE(25, 26)]),
            workflow_call_after(26, 46, [
                WHITESPACE(31, 32, [SPACE(31, 32)]),
                identifier(32, 46)
            ])
        ])]
    }
}

#[test]
fn it_successfully_parses_call_with_after_without_trailing_whitespace() {
    parses_to! {
        parser: WdlParser,
        input: "call imported_doc.my_task after different_task ",
        rule: Rule::workflow_call,
        tokens: [workflow_call(0, 46, [
            WHITESPACE(4, 5, [SPACE(4, 5)]),
            qualified_identifier(5, 25, [
                identifier(5, 17),
                identifier(18, 25)
            ]),
            WHITESPACE(25, 26, [SPACE(25, 26)]),
            workflow_call_after(26, 46, [
                WHITESPACE(31, 32, [SPACE(31, 32)]),
                identifier(32, 46)
            ])
        ])]
    }
}

#[test]
fn it_successfully_parses_call_with_all_options() {
    parses_to! {
        parser: WdlParser,
        input: "call imported_doc.my_task as their_task after different_task { input: a, b = b, c=z, }",
        rule: Rule::workflow_call,
        tokens: [workflow_call(0, 86, [
            WHITESPACE(4, 5, [SPACE(4, 5)]),
            qualified_identifier(5, 25, [
                identifier(5, 17),
                identifier(18, 25)
            ]),
            WHITESPACE(25, 26, [SPACE(25, 26)]),
            workflow_call_as(26, 39, [
                WHITESPACE(28, 29, [SPACE(28, 29)]),
                identifier(29, 39)
            ]),
            WHITESPACE(39, 40, [SPACE(39, 40)]),
            workflow_call_after(40, 60, [
                WHITESPACE(45, 46, [SPACE(45, 46)]),
                identifier(46, 60)
            ]),
            WHITESPACE(60, 61, [SPACE(60, 61)]),
            workflow_call_body(61, 86, [
                WHITESPACE(62, 63, [SPACE(62, 63)]),
                WHITESPACE(69, 70, [SPACE(69, 70)]),
                workflow_call_input(70, 71, [identifier(70, 71)]),
                COMMA(71, 72),
                WHITESPACE(72, 73, [SPACE(72, 73)]),
                workflow_call_input(73, 78, [
                    identifier(73, 74),
                    WHITESPACE(74, 75, [SPACE(74, 75)]),
                    WHITESPACE(76, 77, [SPACE(76, 77)]),
                    expression(77, 78, [identifier(77, 78)])
                ]),
                COMMA(78, 79),
                WHITESPACE(79, 80, [SPACE(79, 80)]),
                workflow_call_input(80, 83, [
                    identifier(80, 81),
                    expression(82, 83, [identifier(82, 83)])
                ]),
                COMMA(83, 84),
                WHITESPACE(84, 85, [SPACE(84, 85)]),
            ]),
        ])]
    }
}

#[test]
fn it_successfully_parses_call_with_all_options_without_trailing_whitespace() {
    parses_to! {
        parser: WdlParser,
        input: "call imported_doc.my_task as their_task after different_task { input: a, b = b, c=z, } ",
        rule: Rule::workflow_call,
        tokens: [workflow_call(0, 86, [
            WHITESPACE(4, 5, [SPACE(4, 5)]),
            qualified_identifier(5, 25, [
                identifier(5, 17),
                identifier(18, 25)
            ]),
            WHITESPACE(25, 26, [SPACE(25, 26)]),
            workflow_call_as(26, 39, [
                WHITESPACE(28, 29, [SPACE(28, 29)]),
                identifier(29, 39)
            ]),
            WHITESPACE(39, 40, [SPACE(39, 40)]),
            workflow_call_after(40, 60, [
                WHITESPACE(45, 46, [SPACE(45, 46)]),
                identifier(46, 60)
            ]),
            WHITESPACE(60, 61, [SPACE(60, 61)]),
            workflow_call_body(61, 86, [
                WHITESPACE(62, 63, [SPACE(62, 63)]),
                WHITESPACE(69, 70, [SPACE(69, 70)]),
                workflow_call_input(70, 71, [identifier(70, 71)]),
                COMMA(71, 72),
                WHITESPACE(72, 73, [SPACE(72, 73)]),
                workflow_call_input(73, 78, [
                    identifier(73, 74),
                    WHITESPACE(74, 75, [SPACE(74, 75)]),
                    WHITESPACE(76, 77, [SPACE(76, 77)]),
                    expression(77, 78, [identifier(77, 78)])
                ]),
                COMMA(78, 79),
                WHITESPACE(79, 80, [SPACE(79, 80)]),
                workflow_call_input(80, 83, [
                    identifier(80, 81),
                    expression(82, 83, [identifier(82, 83)])
                ]),
                COMMA(83, 84),
                WHITESPACE(84, 85, [SPACE(84, 85)]),
            ]),
        ])]
    }
}
