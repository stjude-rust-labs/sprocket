use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::Parser as WdlParser;
use crate::Rule;

#[test]
fn it_fails_to_parse_an_empty_scatter() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::workflow_scatter,
        positives: vec![Rule::workflow_scatter],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_scatter_without_a_body() {
    fails_with! {
        parser: WdlParser,
        input: "scatter(i in range(10))",
        rule: Rule::workflow_scatter,
        positives: vec![Rule::WHITESPACE, Rule::COMMENT],
        negatives: vec![],
        pos: 23
    }
}

#[test]
fn it_successfully_parses_scatter_without_spaces() {
    parses_to! {
        parser: WdlParser,
        input: "scatter(i in range(10)){call my_task}",
        rule: Rule::workflow_scatter,
        tokens: [workflow_scatter(0, 37, [
            workflow_scatter_iteration_statement(7, 23, [
                identifier(8, 9),
                WHITESPACE(9, 10, [SPACE(9, 10)]),
                WHITESPACE(12, 13, [SPACE(12, 13)]),
                expression(13, 22, [
                    identifier(13, 18),
                    apply(18, 22, [
                        expression(19, 21, [
                            integer(19, 21, [
                                integer_decimal(19, 21)
                            ])
                        ])
                    ])
                ]),
            ]),
            workflow_execution_statement(24, 36, [
                workflow_call(24, 36, [
                    WHITESPACE(28, 29, [SPACE(28, 29)]),
                    identifier(29, 36)
                ])
            ])
        ])]
    }
}

#[test]
fn it_successfully_excludes_trailing_whitespace() {
    parses_to! {
        parser: WdlParser,
        input: "scatter(i in range(10)){call my_task}    ",
        rule: Rule::workflow_scatter,
        tokens: [workflow_scatter(0, 37, [
            workflow_scatter_iteration_statement(7, 23, [
                identifier(8, 9),
                WHITESPACE(9, 10, [SPACE(9, 10)]),
                WHITESPACE(12, 13, [SPACE(12, 13)]),
                expression(13, 22, [
                    identifier(13, 18),
                    apply(18, 22, [
                        expression(19, 21, [
                            integer(19, 21, [
                                integer_decimal(19, 21)
                            ])
                        ])
                    ])
                ]),
            ]),
            workflow_execution_statement(24, 36, [
                workflow_call(24, 36, [
                    WHITESPACE(28, 29, [SPACE(28, 29)]),
                    identifier(29, 36)
                ])
            ])
        ])]
    }
}

#[test]
fn it_successfully_parses_scatter_with_empty_body() {
    parses_to! {
        parser: WdlParser,
        input: "scatter(i in range(10)){}",
        rule: Rule::workflow_scatter,
        tokens: [workflow_scatter(0, 25, [
            workflow_scatter_iteration_statement(7, 23, [
                identifier(8, 9),
                WHITESPACE(9, 10, [SPACE(9, 10)]),
                WHITESPACE(12, 13, [SPACE(12, 13)]),
                expression(13, 22, [
                    identifier(13, 18),
                    apply(18, 22, [
                        expression(19, 21, [
                            integer(19, 21, [
                                integer_decimal(19, 21)
                            ])
                        ])
                    ])
                ]),
            ]),
        ])]
    }
}

#[test]
fn it_successfully_parses_scatter_with_spaces() {
    parses_to! {
        parser: WdlParser,
        input: "scatter ( i in range( 10 ) ) { call my_task }",
        rule: Rule::workflow_scatter,
        tokens: [workflow_scatter(0, 45, [
            WHITESPACE(7, 8, [SPACE(7, 8)]),
            workflow_scatter_iteration_statement(8, 28, [
                WHITESPACE(9, 10, [SPACE(9, 10)]),
                identifier(10, 11),
                WHITESPACE(11, 12, [SPACE(11, 12)]),
                WHITESPACE(14, 15, [SPACE(14, 15)]),
                expression(15, 26, [
                    identifier(15, 20),
                    apply(20, 26, [
                        WHITESPACE(21, 22, [SPACE(21, 22)]),
                        expression(22, 24, [
                            integer(22, 24, [
                                integer_decimal(22, 24)
                            ])
                        ]),
                        WHITESPACE(24, 25, [SPACE(24, 25)]),
                    ])
                ]),
                WHITESPACE(26, 27, [SPACE(26, 27)]),
            ]),
            WHITESPACE(28, 29, [SPACE(28, 29)]),
            WHITESPACE(30, 31, [SPACE(30, 31)]),
            workflow_execution_statement(31, 43, [
                workflow_call(31, 43, [
                    WHITESPACE(35, 36, [SPACE(35, 36)]),
                    identifier(36, 43)
                ])
            ]),
            WHITESPACE(43, 44, [SPACE(43, 44)]),
        ])]
    }
}

#[test]
fn it_successfully_parses_scatter_with_multiple_calls() {
    parses_to! {
        parser: WdlParser,
        input: "scatter (i in range(10)){
            call my_task
            call other_task {}
            call another_task {input:foo=bar}
        }",
        rule: Rule::workflow_scatter,
        tokens: [workflow_scatter(0, 137, [
          WHITESPACE(7, 8, [
            SPACE(7, 8),
          ]),
          workflow_scatter_iteration_statement(8, 24, [
            identifier(9, 10),
            WHITESPACE(10, 11, [
              SPACE(10, 11),
            ]),
            WHITESPACE(13, 14, [
              SPACE(13, 14),
            ]),
            expression(14, 23, [
              identifier(14, 19),
              apply(19, 23, [
                expression(20, 22, [
                  integer(20, 22, [
                    integer_decimal(20, 22),
                  ]),
                ]),
              ]),
            ]),
          ]),
          // ``
          WHITESPACE(25, 26, [
            // ``
            NEWLINE(25, 26),
          ]),
          WHITESPACE(26, 27, [
            SPACE(26, 27),
          ]),
          WHITESPACE(27, 28, [
            SPACE(27, 28),
          ]),
          WHITESPACE(28, 29, [
            SPACE(28, 29),
          ]),
          WHITESPACE(29, 30, [
            SPACE(29, 30),
          ]),
          WHITESPACE(30, 31, [
            SPACE(30, 31),
          ]),
          WHITESPACE(31, 32, [
            SPACE(31, 32),
          ]),
          WHITESPACE(32, 33, [
            SPACE(32, 33),
          ]),
          WHITESPACE(33, 34, [
            SPACE(33, 34),
          ]),
          WHITESPACE(34, 35, [
            SPACE(34, 35),
          ]),
          WHITESPACE(35, 36, [
            SPACE(35, 36),
          ]),
          WHITESPACE(36, 37, [
            SPACE(36, 37),
          ]),
          WHITESPACE(37, 38, [
            SPACE(37, 38),
          ]),
          workflow_execution_statement(38, 50, [
            workflow_call(38, 50, [
              WHITESPACE(42, 43, [
                SPACE(42, 43),
              ]),
              identifier(43, 50),
            ]),
          ]),
          // ``
          WHITESPACE(50, 51, [
            // ``
            NEWLINE(50, 51),
          ]),
          WHITESPACE(51, 52, [
            SPACE(51, 52),
          ]),
          WHITESPACE(52, 53, [
            SPACE(52, 53),
          ]),
          WHITESPACE(53, 54, [
            SPACE(53, 54),
          ]),
          WHITESPACE(54, 55, [
            SPACE(54, 55),
          ]),
          WHITESPACE(55, 56, [
            SPACE(55, 56),
          ]),
          WHITESPACE(56, 57, [
            SPACE(56, 57),
          ]),
          WHITESPACE(57, 58, [
            SPACE(57, 58),
          ]),
          WHITESPACE(58, 59, [
            SPACE(58, 59),
          ]),
          WHITESPACE(59, 60, [
            SPACE(59, 60),
          ]),
          WHITESPACE(60, 61, [
            SPACE(60, 61),
          ]),
          WHITESPACE(61, 62, [
            SPACE(61, 62),
          ]),
          WHITESPACE(62, 63, [
            SPACE(62, 63),
          ]),
          workflow_execution_statement(63, 81, [
            workflow_call(63, 81, [
              WHITESPACE(67, 68, [
                SPACE(67, 68),
              ]),
              identifier(68, 78),
              WHITESPACE(78, 79, [
                SPACE(78, 79),
              ]),
              workflow_call_body(79, 81),
            ]),
          ]),
          // ``
          WHITESPACE(81, 82, [
            // ``
            NEWLINE(81, 82),
          ]),
          WHITESPACE(82, 83, [
            SPACE(82, 83),
          ]),
          WHITESPACE(83, 84, [
            SPACE(83, 84),
          ]),
          WHITESPACE(84, 85, [
            SPACE(84, 85),
          ]),
          WHITESPACE(85, 86, [
            SPACE(85, 86),
          ]),
          WHITESPACE(86, 87, [
            SPACE(86, 87),
          ]),
          WHITESPACE(87, 88, [
            SPACE(87, 88),
          ]),
          WHITESPACE(88, 89, [
            SPACE(88, 89),
          ]),
          WHITESPACE(89, 90, [
            SPACE(89, 90),
          ]),
          WHITESPACE(90, 91, [
            SPACE(90, 91),
          ]),
          WHITESPACE(91, 92, [
            SPACE(91, 92),
          ]),
          WHITESPACE(92, 93, [
            SPACE(92, 93),
          ]),
          WHITESPACE(93, 94, [
            SPACE(93, 94),
          ]),
          workflow_execution_statement(94, 127, [
            workflow_call(94, 127, [
              WHITESPACE(98, 99, [
                SPACE(98, 99),
              ]),
              identifier(99, 111),
              WHITESPACE(111, 112, [
                SPACE(111, 112),
              ]),
              workflow_call_body(112, 127, [
                workflow_call_input(119, 126, [
                  identifier(119, 122),
                  expression(123, 126, [
                    identifier(123, 126),
                  ]),
                ]),
              ]),
            ]),
          ]),
          // ``
          WHITESPACE(127, 128, [
            // ``
            NEWLINE(127, 128),
          ]),
          WHITESPACE(128, 129, [
            SPACE(128, 129),
          ]),
          WHITESPACE(129, 130, [
            SPACE(129, 130),
          ]),
          WHITESPACE(130, 131, [
            SPACE(130, 131),
          ]),
          WHITESPACE(131, 132, [
            SPACE(131, 132),
          ]),
          WHITESPACE(132, 133, [
            SPACE(132, 133),
          ]),
          WHITESPACE(133, 134, [
            SPACE(133, 134),
          ]),
          WHITESPACE(134, 135, [
            SPACE(134, 135),
          ]),
          WHITESPACE(135, 136, [
            SPACE(135, 136),
          ]),
        ])
        ]
    }
}
