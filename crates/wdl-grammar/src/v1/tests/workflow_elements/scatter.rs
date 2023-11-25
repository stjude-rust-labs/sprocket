use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::v1::Parser as WdlParser;
use crate::v1::Rule;

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
        tokens: [
          // `scatter(i in range(10)){call my_task}`
          workflow_scatter(0, 37, [
            // `(i in range(10))`
            workflow_scatter_iteration_statement(7, 23, [
              // `i`
              workflow_scatter_iteration_statement_variable(8, 9, [
                // `i`
                singular_identifier(8, 9),
              ]),
              WHITESPACE(9, 10, [
                SPACE(9, 10),
              ]),
              WHITESPACE(12, 13, [
                SPACE(12, 13),
              ]),
              // `range(10)`
              workflow_scatter_iteration_statement_iterable(13, 22, [
                // `range(10)`
                expression(13, 22, [
                  // `range`
                  singular_identifier(13, 18),
                  // `(10)`
                  call(18, 22, [
                    // `10`
                    expression(19, 21, [
                      // `10`
                      integer(19, 21, [
                        // `10`
                        integer_decimal(19, 21),
                      ]),
                    ]),
                  ]),
                ]),
              ]),
            ]),
            // `call my_task`
            workflow_execution_statement(24, 36, [
              // `call my_task`
              workflow_call(24, 36, [
                WHITESPACE(28, 29, [
                  SPACE(28, 29),
                ]),
                // `my_task`
                workflow_call_name(29, 36, [
                  // `my_task`
                  singular_identifier(29, 36),
                ]),
              ]),
            ]),
          ])
        ]
    }
}

#[test]
fn it_successfully_excludes_trailing_whitespace() {
    parses_to! {
        parser: WdlParser,
        input: "scatter(i in range(10)){call my_task}    ",
        rule: Rule::workflow_scatter,
        tokens: [
          // `scatter(i in range(10)){call my_task}`
          workflow_scatter(0, 37, [
            // `(i in range(10))`
            workflow_scatter_iteration_statement(7, 23, [
              // `i`
              workflow_scatter_iteration_statement_variable(8, 9, [
                // `i`
                singular_identifier(8, 9),
              ]),
              WHITESPACE(9, 10, [
                SPACE(9, 10),
              ]),
              WHITESPACE(12, 13, [
                SPACE(12, 13),
              ]),
              // `range(10)`
              workflow_scatter_iteration_statement_iterable(13, 22, [
                // `range(10)`
                expression(13, 22, [
                  // `range`
                  singular_identifier(13, 18),
                  // `(10)`
                  call(18, 22, [
                    // `10`
                    expression(19, 21, [
                      // `10`
                      integer(19, 21, [
                        // `10`
                        integer_decimal(19, 21),
                      ]),
                    ]),
                  ]),
                ]),
              ]),
            ]),
            // `call my_task`
            workflow_execution_statement(24, 36, [
              // `call my_task`
              workflow_call(24, 36, [
                WHITESPACE(28, 29, [
                  SPACE(28, 29),
                ]),
                // `my_task`
                workflow_call_name(29, 36, [
                  // `my_task`
                  singular_identifier(29, 36),
                ]),
              ]),
            ]),
          ])
        ]
    }
}

#[test]
fn it_successfully_parses_scatter_with_empty_body() {
    parses_to! {
        parser: WdlParser,
        input: "scatter(i in range(10)){}",
        rule: Rule::workflow_scatter,
        tokens: [
          // `scatter(i in range(10)){}`
          workflow_scatter(0, 25, [
            // `(i in range(10))`
            workflow_scatter_iteration_statement(7, 23, [
              // `i`
              workflow_scatter_iteration_statement_variable(8, 9, [
                // `i`
                singular_identifier(8, 9),
              ]),
              WHITESPACE(9, 10, [
                SPACE(9, 10),
              ]),
              WHITESPACE(12, 13, [
                SPACE(12, 13),
              ]),
              // `range(10)`
              workflow_scatter_iteration_statement_iterable(13, 22, [
                // `range(10)`
                expression(13, 22, [
                  // `range`
                  singular_identifier(13, 18),
                  // `(10)`
                  call(18, 22, [
                    // `10`
                    expression(19, 21, [
                      // `10`
                      integer(19, 21, [
                        // `10`
                        integer_decimal(19, 21),
                      ]),
                    ]),
                  ]),
                ]),
              ]),
            ]),
          ])
        ]
    }
}

#[test]
fn it_successfully_parses_scatter_with_spaces() {
    parses_to! {
        parser: WdlParser,
        input: "scatter ( i in range( 10 ) ) { call my_task }",
        rule: Rule::workflow_scatter,
        tokens: [
          // `scatter ( i in range( 10 ) ) { call my_task }`
          workflow_scatter(0, 45, [
            WHITESPACE(7, 8, [
              SPACE(7, 8),
            ]),
            // `( i in range( 10 ) )`
            workflow_scatter_iteration_statement(8, 28, [
              WHITESPACE(9, 10, [
                SPACE(9, 10),
              ]),
              // `i`
              workflow_scatter_iteration_statement_variable(10, 11, [
                // `i`
                singular_identifier(10, 11),
              ]),
              WHITESPACE(11, 12, [
                SPACE(11, 12),
              ]),
              WHITESPACE(14, 15, [
                SPACE(14, 15),
              ]),
              // `range( 10 )`
              workflow_scatter_iteration_statement_iterable(15, 26, [
                // `range( 10 )`
                expression(15, 26, [
                  // `range`
                  singular_identifier(15, 20),
                  // `( 10 )`
                  call(20, 26, [
                    WHITESPACE(21, 22, [
                      SPACE(21, 22),
                    ]),
                    // `10`
                    expression(22, 24, [
                      // `10`
                      integer(22, 24, [
                        // `10`
                        integer_decimal(22, 24),
                      ]),
                    ]),
                    WHITESPACE(24, 25, [
                      SPACE(24, 25),
                    ]),
                  ]),
                ]),
              ]),
              WHITESPACE(26, 27, [
                SPACE(26, 27),
              ]),
            ]),
            WHITESPACE(28, 29, [
              SPACE(28, 29),
            ]),
            WHITESPACE(30, 31, [
              SPACE(30, 31),
            ]),
            // `call my_task`
            workflow_execution_statement(31, 43, [
              // `call my_task`
              workflow_call(31, 43, [
                WHITESPACE(35, 36, [
                  SPACE(35, 36),
                ]),
                // `my_task`
                workflow_call_name(36, 43, [
                  // `my_task`
                  singular_identifier(36, 43),
                ]),
              ]),
            ]),
            WHITESPACE(43, 44, [
              SPACE(43, 44),
            ]),
          ])
        ]
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
        tokens: [
          // `scatter (i in range(10)){ call my_task call other_task {} call another_task {input:foo=bar} }`
          workflow_scatter(0, 105, [
            WHITESPACE(7, 8, [
              SPACE(7, 8),
            ]),
            // `(i in range(10))`
            workflow_scatter_iteration_statement(8, 24, [
              // `i`
              workflow_scatter_iteration_statement_variable(9, 10, [
                // `i`
                singular_identifier(9, 10),
              ]),
              WHITESPACE(10, 11, [
                SPACE(10, 11),
              ]),
              WHITESPACE(13, 14, [
                SPACE(13, 14),
              ]),
              // `range(10)`
              workflow_scatter_iteration_statement_iterable(14, 23, [
                // `range(10)`
                expression(14, 23, [
                  // `range`
                  singular_identifier(14, 19),
                  // `(10)`
                  call(19, 23, [
                    // `10`
                    expression(20, 22, [
                      // `10`
                      integer(20, 22, [
                        // `10`
                        integer_decimal(20, 22),
                      ]),
                    ]),
                  ]),
                ]),
              ]),
            ]),
            WHITESPACE(25, 26, [
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
            // `call my_task`
            workflow_execution_statement(30, 42, [
              // `call my_task`
              workflow_call(30, 42, [
                WHITESPACE(34, 35, [
                  SPACE(34, 35),
                ]),
                // `my_task`
                workflow_call_name(35, 42, [
                  // `my_task`
                  singular_identifier(35, 42),
                ]),
              ]),
            ]),
            WHITESPACE(42, 43, [
              NEWLINE(42, 43),
            ]),
            WHITESPACE(43, 44, [
              SPACE(43, 44),
            ]),
            WHITESPACE(44, 45, [
              SPACE(44, 45),
            ]),
            WHITESPACE(45, 46, [
              SPACE(45, 46),
            ]),
            WHITESPACE(46, 47, [
              SPACE(46, 47),
            ]),
            // `call other_task {}`
            workflow_execution_statement(47, 65, [
              // `call other_task {}`
              workflow_call(47, 65, [
                WHITESPACE(51, 52, [
                  SPACE(51, 52),
                ]),
                // `other_task`
                workflow_call_name(52, 62, [
                  // `other_task`
                  singular_identifier(52, 62),
                ]),
                WHITESPACE(62, 63, [
                  SPACE(62, 63),
                ]),
                // `{}`
                workflow_call_body(63, 65),
              ]),
            ]),
            WHITESPACE(65, 66, [
              NEWLINE(65, 66),
            ]),
            WHITESPACE(66, 67, [
              SPACE(66, 67),
            ]),
            WHITESPACE(67, 68, [
              SPACE(67, 68),
            ]),
            WHITESPACE(68, 69, [
              SPACE(68, 69),
            ]),
            WHITESPACE(69, 70, [
              SPACE(69, 70),
            ]),
            // `call another_task {input:foo=bar}`
            workflow_execution_statement(70, 103, [
              // `call another_task {input:foo=bar}`
              workflow_call(70, 103, [
                WHITESPACE(74, 75, [
                  SPACE(74, 75),
                ]),
                // `another_task`
                workflow_call_name(75, 87, [
                  // `another_task`
                  singular_identifier(75, 87),
                ]),
                WHITESPACE(87, 88, [
                  SPACE(87, 88),
                ]),
                // `{input:foo=bar}`
                workflow_call_body(88, 103, [
                  // `foo=bar`
                  workflow_call_input(95, 102, [
                    // `foo`
                    singular_identifier(95, 98),
                    // `bar`
                    expression(99, 102, [
                      // `bar`
                      singular_identifier(99, 102),
                    ]),
                  ]),
                ]),
              ]),
            ]),
            WHITESPACE(103, 104, [
              NEWLINE(103, 104),
            ]),
          ])
                ]
    }
}
