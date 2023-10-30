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
        rule: Rule::workflow_conditional,
        positives: vec![Rule::workflow_conditional],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_scatter_without_a_body() {
    fails_with! {
        parser: WdlParser,
        input: "if(true)",
        rule: Rule::workflow_conditional,
        positives: vec![Rule::WHITESPACE, Rule::COMMENT],
        negatives: vec![],
        pos: 8
    }
}

#[test]
fn it_successfully_parses_conditional_without_space() {
    parses_to! {
        parser: WdlParser,
        input: "if(true){Int a=10}",
        rule: Rule::workflow_conditional,
        tokens: [workflow_conditional(0, 18, [
            expression(3, 7, [
                boolean(3, 7)
            ]),
            workflow_execution_statement(9, 17, [
                workflow_private_declarations(9, 17, [
                    bound_declaration(9, 17, [
                        int_type(9, 12),
                        WHITESPACE(12, 13, [SPACE(12, 13)]),
                        identifier(13, 14),
                        expression(15, 17, [
                            integer(15, 17, [
                                integer_decimal(15, 17)
                            ])
                        ])
                    ])
                ]),
            ]),
        ])]
    }
}

#[test]
fn it_successfully_parses_conditional_with_empty_body() {
    parses_to! {
        parser: WdlParser,
        input: "if(true){}",
        rule: Rule::workflow_conditional,
        tokens: [workflow_conditional(0, 10, [
            expression(3, 7, [
                boolean(3, 7)
            ]),
        ])]
    }
}

#[test]
fn it_successfully_excludes_trailing_whitespace() {
    parses_to! {
        parser: WdlParser,
        input: "if(true){Int a=10}   ",
        rule: Rule::workflow_conditional,
        tokens: [workflow_conditional(0, 18, [
            expression(3, 7, [
                boolean(3, 7)
            ]),
            workflow_execution_statement(9, 17, [
                workflow_private_declarations(9, 17, [
                    bound_declaration(9, 17, [
                        int_type(9, 12),
                        WHITESPACE(12, 13, [SPACE(12, 13)]),
                        identifier(13, 14),
                        expression(15, 17, [
                            integer(15, 17, [
                                integer_decimal(15, 17)
                            ])
                        ])
                    ])
                ]),
            ]),
        ])]
    }
}

#[test]
fn it_successfully_parses_conditional_with_space() {
    parses_to! {
        parser: WdlParser,
        input: "if ( true ) { Int a=10 }",
        rule: Rule::workflow_conditional,
        tokens: [workflow_conditional(0, 24, [
            WHITESPACE(2, 3, [SPACE(2, 3)]),
            WHITESPACE(4, 5, [SPACE(4, 5)]),
            expression(5, 9, [
                boolean(5, 9)
            ]),
            WHITESPACE(9, 10, [SPACE(9, 10)]),
            WHITESPACE(11, 12, [SPACE(11, 12)]),
            WHITESPACE(13, 14, [SPACE(13, 14)]),
            workflow_execution_statement(14, 23, [
                workflow_private_declarations(14, 23, [
                    bound_declaration(14, 22, [
                        int_type(14, 17),
                        WHITESPACE(17, 18, [SPACE(17, 18)]),
                        identifier(18, 19),
                        expression(20, 22, [
                            integer(20, 22, [
                                integer_decimal(20, 22)
                            ])
                        ])
                    ]),
                    WHITESPACE(22, 23, [SPACE(22, 23)]),
                ]),
            ]),
        ])]
    }
}

#[test]
fn it_successfully_parses_conditional_with_multiple_statements() {
    parses_to! {
        parser: WdlParser,
        input: "if(true){
            Int a=10
            call my_task{input:foo=a}
            call no_inputs{}
        }",
        rule: Rule::workflow_conditional,
        tokens: [workflow_conditional(0, 107, [
            expression(3, 7, [
              boolean(3, 7),
            ]),
            // ``
            WHITESPACE(9, 10, [
              // ``
              NEWLINE(9, 10),
            ]),
            WHITESPACE(10, 11, [
              SPACE(10, 11),
            ]),
            WHITESPACE(11, 12, [
              SPACE(11, 12),
            ]),
            WHITESPACE(12, 13, [
              SPACE(12, 13),
            ]),
            WHITESPACE(13, 14, [
              SPACE(13, 14),
            ]),
            WHITESPACE(14, 15, [
              SPACE(14, 15),
            ]),
            WHITESPACE(15, 16, [
              SPACE(15, 16),
            ]),
            WHITESPACE(16, 17, [
              SPACE(16, 17),
            ]),
            WHITESPACE(17, 18, [
              SPACE(17, 18),
            ]),
            WHITESPACE(18, 19, [
              SPACE(18, 19),
            ]),
            WHITESPACE(19, 20, [
              SPACE(19, 20),
            ]),
            WHITESPACE(20, 21, [
              SPACE(20, 21),
            ]),
            WHITESPACE(21, 22, [
              SPACE(21, 22),
            ]),
            workflow_execution_statement(22, 43, [
              workflow_private_declarations(22, 43, [
                bound_declaration(22, 30, [
                  int_type(22, 25),
                  WHITESPACE(25, 26, [
                    SPACE(25, 26),
                  ]),
                  identifier(26, 27),
                  expression(28, 30, [
                    integer(28, 30, [
                      integer_decimal(28, 30),
                    ]),
                  ]),
                ]),
                // ``
                WHITESPACE(30, 31, [
                  // ``
                  NEWLINE(30, 31),
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
                WHITESPACE(38, 39, [
                  SPACE(38, 39),
                ]),
                WHITESPACE(39, 40, [
                  SPACE(39, 40),
                ]),
                WHITESPACE(40, 41, [
                  SPACE(40, 41),
                ]),
                WHITESPACE(41, 42, [
                  SPACE(41, 42),
                ]),
                WHITESPACE(42, 43, [
                  SPACE(42, 43),
                ]),
              ]),
            ]),
            workflow_execution_statement(43, 68, [
              workflow_call(43, 68, [
                WHITESPACE(47, 48, [
                  SPACE(47, 48),
                ]),
                identifier(48, 55),
                workflow_call_body(55, 68, [
                  workflow_call_input(62, 67, [
                    identifier(62, 65),
                    expression(66, 67, [
                      identifier(66, 67),
                    ]),
                  ]),
                ]),
              ]),
            ]),
            // ``
            WHITESPACE(68, 69, [
              // ``
              NEWLINE(68, 69),
            ]),
            WHITESPACE(69, 70, [
              SPACE(69, 70),
            ]),
            WHITESPACE(70, 71, [
              SPACE(70, 71),
            ]),
            WHITESPACE(71, 72, [
              SPACE(71, 72),
            ]),
            WHITESPACE(72, 73, [
              SPACE(72, 73),
            ]),
            WHITESPACE(73, 74, [
              SPACE(73, 74),
            ]),
            WHITESPACE(74, 75, [
              SPACE(74, 75),
            ]),
            WHITESPACE(75, 76, [
              SPACE(75, 76),
            ]),
            WHITESPACE(76, 77, [
              SPACE(76, 77),
            ]),
            WHITESPACE(77, 78, [
              SPACE(77, 78),
            ]),
            WHITESPACE(78, 79, [
              SPACE(78, 79),
            ]),
            WHITESPACE(79, 80, [
              SPACE(79, 80),
            ]),
            WHITESPACE(80, 81, [
              SPACE(80, 81),
            ]),
            workflow_execution_statement(81, 97, [
              workflow_call(81, 97, [
                WHITESPACE(85, 86, [
                  SPACE(85, 86),
                ]),
                identifier(86, 95),
                workflow_call_body(95, 97),
              ]),
            ]),
            // ``
            WHITESPACE(97, 98, [
              // ``
              NEWLINE(97, 98),
            ]),
            WHITESPACE(98, 99, [
              SPACE(98, 99),
            ]),
            WHITESPACE(99, 100, [
              SPACE(99, 100),
            ]),
            WHITESPACE(100, 101, [
              SPACE(100, 101),
            ]),
            WHITESPACE(101, 102, [
              SPACE(101, 102),
            ]),
            WHITESPACE(102, 103, [
              SPACE(102, 103),
            ]),
            WHITESPACE(103, 104, [
              SPACE(103, 104),
            ]),
            WHITESPACE(104, 105, [
              SPACE(104, 105),
            ]),
            WHITESPACE(105, 106, [
              SPACE(105, 106),
            ]),
          ])
        ]
    }
}
