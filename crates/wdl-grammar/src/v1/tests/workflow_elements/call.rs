use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::v1::Parser as WdlParser;
use crate::v1::Rule;

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
            Rule::workflow_call_name,
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
        tokens: [
          // `call my_task`
          workflow_call(0, 12, [
              WHITESPACE(4, 5, [
                SPACE(4, 5),
              ]),
              // `my_task`
              workflow_call_name(5, 12, [
                // `my_task`
                singular_identifier(5, 12),
              ]),
            ])
        ]

    }
}

#[test]
fn it_successfully_excludes_trailing_whitespace() {
    parses_to! {
        parser: WdlParser,
        input: "call my_task   ",
        rule: Rule::workflow_call,
        tokens: [
          // `call my_task`
          workflow_call(0, 12, [
            WHITESPACE(4, 5, [
              SPACE(4, 5),
            ]),
            // `my_task`
            workflow_call_name(5, 12, [
              // `my_task`
              singular_identifier(5, 12),
            ]),
          ])
        ]
    }
}

#[test]
fn it_successfully_parses_call_with_empty_body() {
    parses_to! {
        parser: WdlParser,
        input: "call my_task{}",
        rule: Rule::workflow_call,
        tokens: [
          // `call my_task{}`
          workflow_call(0, 14, [
            WHITESPACE(4, 5, [
              SPACE(4, 5),
            ]),
            // `my_task`
            workflow_call_name(5, 12, [
              // `my_task`
              singular_identifier(5, 12),
            ]),
            // `{}`
            workflow_call_body(12, 14),
          ])
        ]
    }
}

#[test]
fn it_successfully_parses_call_with_implicitly_declared_input() {
    parses_to! {
        parser: WdlParser,
        input: "call my_task{input:a}",
        rule: Rule::workflow_call,
        tokens: [
          // `call my_task{input:a}`
          workflow_call(0, 21, [
            WHITESPACE(4, 5, [
              SPACE(4, 5),
            ]),
            // `my_task`
            workflow_call_name(5, 12, [
              // `my_task`
              singular_identifier(5, 12),
            ]),
            // `{input:a}`
            workflow_call_body(12, 21, [
              // `a`
              workflow_call_input(19, 20, [
                // `a`
                singular_identifier(19, 20),
              ]),
            ]),
          ])
        ]
    }
}

#[test]
fn it_successfully_parses_call_with_implicitly_declared_input_without_trailing_whitespace() {
    parses_to! {
        parser: WdlParser,
        input: "call my_task{input:a} ",
        rule: Rule::workflow_call,
        tokens: [
          // `call my_task{input:a}`
          workflow_call(0, 21, [
            WHITESPACE(4, 5, [
              SPACE(4, 5),
            ]),
            // `my_task`
            workflow_call_name(5, 12, [
              // `my_task`
              singular_identifier(5, 12),
            ]),
            // `{input:a}`
            workflow_call_body(12, 21, [
              // `a`
              workflow_call_input(19, 20, [
                // `a`
                singular_identifier(19, 20),
              ]),
            ]),
          ])
        ]
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
            WHITESPACE(4, 5, [
              SPACE(4, 5),
            ]),
            // `my_task`
            workflow_call_name(5, 12, [
              // `my_task`
              singular_identifier(5, 12),
            ]),
            // `{input:a=b}`
            workflow_call_body(12, 23, [
              // `a=b`
              workflow_call_input(19, 22, [
                // `a`
                singular_identifier(19, 20),
                // `b`
                expression(21, 22, [
                  // `b`
                  singular_identifier(21, 22),
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
            WHITESPACE(4, 5, [
              SPACE(4, 5),
            ]),
            // `my_task`
            workflow_call_name(5, 12, [
              // `my_task`
              singular_identifier(5, 12),
            ]),
            // `{input:a=b}`
            workflow_call_body(12, 23, [
              // `a=b`
              workflow_call_input(19, 22, [
                // `a`
                singular_identifier(19, 20),
                // `b`
                expression(21, 22, [
                  // `b`
                  singular_identifier(21, 22),
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
        tokens: [
          // `call my_task{input:a,b=b,c=z}`
          workflow_call(0, 29, [
            WHITESPACE(4, 5, [
              SPACE(4, 5),
            ]),
            // `my_task`
            workflow_call_name(5, 12, [
              // `my_task`
              singular_identifier(5, 12),
            ]),
            // `{input:a,b=b,c=z}`
            workflow_call_body(12, 29, [
              // `a`
              workflow_call_input(19, 20, [
                // `a`
                singular_identifier(19, 20),
              ]),
              // `,`
              COMMA(20, 21),
              // `b=b`
              workflow_call_input(21, 24, [
                // `b`
                singular_identifier(21, 22),
                // `b`
                expression(23, 24, [
                  // `b`
                  singular_identifier(23, 24),
                ]),
              ]),
              // `,`
              COMMA(24, 25),
              // `c=z`
              workflow_call_input(25, 28, [
                // `c`
                singular_identifier(25, 26),
                // `z`
                expression(27, 28, [
                  // `z`
                  singular_identifier(27, 28),
                ]),
              ]),
            ]),
          ])
        ]
    }
}

#[test]
fn it_successfully_parses_call_with_multiple_inputs_without_trailing_whitespace() {
    parses_to! {
        parser: WdlParser,
        input: "call my_task{input:a,b=b,c=z} ",
        rule: Rule::workflow_call,
        tokens: [
          // `call my_task{input:a,b=b,c=z}`
          workflow_call(0, 29, [
            WHITESPACE(4, 5, [
              SPACE(4, 5),
            ]),
            // `my_task`
            workflow_call_name(5, 12, [
              // `my_task`
              singular_identifier(5, 12),
            ]),
            // `{input:a,b=b,c=z}`
            workflow_call_body(12, 29, [
              // `a`
              workflow_call_input(19, 20, [
                // `a`
                singular_identifier(19, 20),
              ]),
              // `,`
              COMMA(20, 21),
              // `b=b`
              workflow_call_input(21, 24, [
                // `b`
                singular_identifier(21, 22),
                // `b`
                expression(23, 24, [
                  // `b`
                  singular_identifier(23, 24),
                ]),
              ]),
              // `,`
              COMMA(24, 25),
              // `c=z`
              workflow_call_input(25, 28, [
                // `c`
                singular_identifier(25, 26),
                // `z`
                expression(27, 28, [
                  // `z`
                  singular_identifier(27, 28),
                ]),
              ]),
            ]),
          ])
        ]
    }
}

#[test]
fn it_successfully_parses_call_with_as() {
    parses_to! {
        parser: WdlParser,
        input: "call my_task as different_task",
        rule: Rule::workflow_call,
        tokens: [
          // `call my_task as different_task`
          workflow_call(0, 30, [
            WHITESPACE(4, 5, [
              SPACE(4, 5),
            ]),
            // `my_task`
            workflow_call_name(5, 12, [
              // `my_task`
              singular_identifier(5, 12),
            ]),
            WHITESPACE(12, 13, [
              SPACE(12, 13),
            ]),
            // `as different_task`
            workflow_call_as(13, 30, [
              WHITESPACE(15, 16, [
                SPACE(15, 16),
              ]),
              // `different_task`
              singular_identifier(16, 30),
            ]),
          ])
        ]
    }
}

#[test]
fn it_successfully_parses_call_with_after() {
    parses_to! {
        parser: WdlParser,
        input: "call imported_doc.my_task after different_task",
        rule: Rule::workflow_call,
        tokens: [
          // `call imported_doc.my_task after different_task`
          workflow_call(0, 46, [
            WHITESPACE(4, 5, [
              SPACE(4, 5),
            ]),
            // `imported_doc.my_task`
            workflow_call_name(5, 25, [
              // `imported_doc.my_task`
              qualified_identifier(5, 25, [
                // `imported_doc`
                singular_identifier(5, 17),
                // `my_task`
                singular_identifier(18, 25),
              ]),
            ]),
            WHITESPACE(25, 26, [
              SPACE(25, 26),
            ]),
            // `after different_task`
            workflow_call_after(26, 46, [
              WHITESPACE(31, 32, [
                SPACE(31, 32),
              ]),
              // `different_task`
              singular_identifier(32, 46),
            ]),
          ])
        ]
    }
}

#[test]
fn it_successfully_parses_call_with_after_without_trailing_whitespace() {
    parses_to! {
        parser: WdlParser,
        input: "call imported_doc.my_task after different_task ",
        rule: Rule::workflow_call,
        tokens: [
          // `call imported_doc.my_task after different_task`
          workflow_call(0, 46, [
            WHITESPACE(4, 5, [
              SPACE(4, 5),
            ]),
            // `imported_doc.my_task`
            workflow_call_name(5, 25, [
              // `imported_doc.my_task`
              qualified_identifier(5, 25, [
                // `imported_doc`
                singular_identifier(5, 17),
                // `my_task`
                singular_identifier(18, 25),
              ]),
            ]),
            WHITESPACE(25, 26, [
              SPACE(25, 26),
            ]),
            // `after different_task`
            workflow_call_after(26, 46, [
              WHITESPACE(31, 32, [
                SPACE(31, 32),
              ]),
              // `different_task`
              singular_identifier(32, 46),
            ]),
          ])
        ]
    }
}

#[test]
fn it_successfully_parses_call_with_all_options() {
    parses_to! {
        parser: WdlParser,
        input: "call imported_doc.my_task as their_task after different_task { input: a, b = b, c=z, }",
        rule: Rule::workflow_call,
        tokens: [
          // `call imported_doc.my_task as their_task after different_task { input: a, b = b, c=z, }`
          workflow_call(0, 86, [
            WHITESPACE(4, 5, [
              SPACE(4, 5),
            ]),
            // `imported_doc.my_task`
            workflow_call_name(5, 25, [
              // `imported_doc.my_task`
              qualified_identifier(5, 25, [
                // `imported_doc`
                singular_identifier(5, 17),
                // `my_task`
                singular_identifier(18, 25),
              ]),
            ]),
            WHITESPACE(25, 26, [
              SPACE(25, 26),
            ]),
            // `as their_task`
            workflow_call_as(26, 39, [
              WHITESPACE(28, 29, [
                SPACE(28, 29),
              ]),
              // `their_task`
              singular_identifier(29, 39),
            ]),
            WHITESPACE(39, 40, [
              SPACE(39, 40),
            ]),
            // `after different_task`
            workflow_call_after(40, 60, [
              WHITESPACE(45, 46, [
                SPACE(45, 46),
              ]),
              // `different_task`
              singular_identifier(46, 60),
            ]),
            WHITESPACE(60, 61, [
              SPACE(60, 61),
            ]),
            // `{ input: a, b = b, c=z, }`
            workflow_call_body(61, 86, [
              WHITESPACE(62, 63, [
                SPACE(62, 63),
              ]),
              WHITESPACE(69, 70, [
                SPACE(69, 70),
              ]),
              // `a`
              workflow_call_input(70, 71, [
                // `a`
                singular_identifier(70, 71),
              ]),
              // `,`
              COMMA(71, 72),
              WHITESPACE(72, 73, [
                SPACE(72, 73),
              ]),
              // `b = b`
              workflow_call_input(73, 78, [
                // `b`
                singular_identifier(73, 74),
                WHITESPACE(74, 75, [
                  SPACE(74, 75),
                ]),
                WHITESPACE(76, 77, [
                  SPACE(76, 77),
                ]),
                // `b`
                expression(77, 78, [
                  // `b`
                  singular_identifier(77, 78),
                ]),
              ]),
              // `,`
              COMMA(78, 79),
              WHITESPACE(79, 80, [
                SPACE(79, 80),
              ]),
              // `c=z`
              workflow_call_input(80, 83, [
                // `c`
                singular_identifier(80, 81),
                // `z`
                expression(82, 83, [
                  // `z`
                  singular_identifier(82, 83),
                ]),
              ]),
              // `,`
              COMMA(83, 84),
              WHITESPACE(84, 85, [
                SPACE(84, 85),
              ]),
            ]),
          ])
        ]
    }
}

#[test]
fn it_successfully_parses_call_with_all_options_without_trailing_whitespace() {
    parses_to! {
        parser: WdlParser,
        input: "call imported_doc.my_task as their_task after different_task { input: a, b = b, c=z, } ",
        rule: Rule::workflow_call,
        tokens: [
          // `call imported_doc.my_task as their_task after different_task { input: a, b = b, c=z, }`
          workflow_call(0, 86, [
            WHITESPACE(4, 5, [
              SPACE(4, 5),
            ]),
            // `imported_doc.my_task`
            workflow_call_name(5, 25, [
              // `imported_doc.my_task`
              qualified_identifier(5, 25, [
                // `imported_doc`
                singular_identifier(5, 17),
                // `my_task`
                singular_identifier(18, 25),
              ]),
            ]),
            WHITESPACE(25, 26, [
              SPACE(25, 26),
            ]),
            // `as their_task`
            workflow_call_as(26, 39, [
              WHITESPACE(28, 29, [
                SPACE(28, 29),
              ]),
              // `their_task`
              singular_identifier(29, 39),
            ]),
            WHITESPACE(39, 40, [
              SPACE(39, 40),
            ]),
            // `after different_task`
            workflow_call_after(40, 60, [
              WHITESPACE(45, 46, [
                SPACE(45, 46),
              ]),
              // `different_task`
              singular_identifier(46, 60),
            ]),
            WHITESPACE(60, 61, [
              SPACE(60, 61),
            ]),
            // `{ input: a, b = b, c=z, }`
            workflow_call_body(61, 86, [
              WHITESPACE(62, 63, [
                SPACE(62, 63),
              ]),
              WHITESPACE(69, 70, [
                SPACE(69, 70),
              ]),
              // `a`
              workflow_call_input(70, 71, [
                // `a`
                singular_identifier(70, 71),
              ]),
              // `,`
              COMMA(71, 72),
              WHITESPACE(72, 73, [
                SPACE(72, 73),
              ]),
              // `b = b`
              workflow_call_input(73, 78, [
                // `b`
                singular_identifier(73, 74),
                WHITESPACE(74, 75, [
                  SPACE(74, 75),
                ]),
                WHITESPACE(76, 77, [
                  SPACE(76, 77),
                ]),
                // `b`
                expression(77, 78, [
                  // `b`
                  singular_identifier(77, 78),
                ]),
              ]),
              // `,`
              COMMA(78, 79),
              WHITESPACE(79, 80, [
                SPACE(79, 80),
              ]),
              // `c=z`
              workflow_call_input(80, 83, [
                // `c`
                singular_identifier(80, 81),
                // `z`
                expression(82, 83, [
                  // `z`
                  singular_identifier(82, 83),
                ]),
              ]),
              // `,`
              COMMA(83, 84),
              WHITESPACE(84, 85, [
                SPACE(84, 85),
              ]),
            ]),
          ])
        ]
    }
}
