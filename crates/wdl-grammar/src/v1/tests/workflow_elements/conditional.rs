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
        tokens: [
          // `if(true){Int a=10}`
          workflow_conditional(0, 18, [
            // `true`
            workflow_conditional_condition(3, 7, [
              // `true`
              expression(3, 7, [
                // `true`
                boolean(3, 7),
              ]),
            ]),
            // `Int a=10`
            workflow_execution_statement(9, 17, [
              // `Int a=10`
              private_declarations(9, 17, [
                // `Int a=10`
                bound_declaration(9, 17, [
                  // `Int`
                  wdl_type(9, 12, [
                    // `Int`
                    int_type(9, 12),
                  ]),
                  WHITESPACE(12, 13, [
                    SPACE(12, 13),
                  ]),
                  // `a`
                  bound_declaration_name(13, 14, [
                    // `a`
                    singular_identifier(13, 14),
                  ]),
                  // `10`
                  expression(15, 17, [
                    // `10`
                    integer(15, 17, [
                      // `10`
                      integer_decimal(15, 17),
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
fn it_successfully_parses_conditional_with_empty_body() {
    parses_to! {
        parser: WdlParser,
        input: "if(true){}",
        rule: Rule::workflow_conditional,
        tokens: [
          // `if(true){}`
          workflow_conditional(0, 10, [
            // `true`
            workflow_conditional_condition(3, 7, [
              // `true`
              expression(3, 7, [
                // `true`
                boolean(3, 7),
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
        input: "if(true){Int a=10}   ",
        rule: Rule::workflow_conditional,
        tokens: [
          // `if(true){Int a=10}`
          workflow_conditional(0, 18, [
            // `true`
            workflow_conditional_condition(3, 7, [
              // `true`
              expression(3, 7, [
                // `true`
                boolean(3, 7),
              ]),
            ]),
            // `Int a=10`
            workflow_execution_statement(9, 17, [
              // `Int a=10`
              private_declarations(9, 17, [
                // `Int a=10`
                bound_declaration(9, 17, [
                  // `Int`
                  wdl_type(9, 12, [
                    // `Int`
                    int_type(9, 12),
                  ]),
                  WHITESPACE(12, 13, [
                    SPACE(12, 13),
                  ]),
                  // `a`
                  bound_declaration_name(13, 14, [
                    // `a`
                    singular_identifier(13, 14),
                  ]),
                  // `10`
                  expression(15, 17, [
                    // `10`
                    integer(15, 17, [
                      // `10`
                      integer_decimal(15, 17),
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
fn it_successfully_parses_conditional_with_space() {
    parses_to! {
        parser: WdlParser,
        input: "if ( true ) { Int a=10 }",
        rule: Rule::workflow_conditional,
        tokens: [
          // `if ( true ) { Int a=10 }`
          workflow_conditional(0, 24, [
            WHITESPACE(2, 3, [
              SPACE(2, 3),
            ]),
            WHITESPACE(4, 5, [
              SPACE(4, 5),
            ]),
            // `true`
            workflow_conditional_condition(5, 9, [
              // `true`
              expression(5, 9, [
                // `true`
                boolean(5, 9),
              ]),
            ]),
            WHITESPACE(9, 10, [
              SPACE(9, 10),
            ]),
            WHITESPACE(11, 12, [
              SPACE(11, 12),
            ]),
            WHITESPACE(13, 14, [
              SPACE(13, 14),
            ]),
            // `Int a=10`
            workflow_execution_statement(14, 23, [
              // `Int a=10`
              private_declarations(14, 23, [
                // `Int a=10`
                bound_declaration(14, 22, [
                  // `Int`
                  wdl_type(14, 17, [
                    // `Int`
                    int_type(14, 17),
                  ]),
                  WHITESPACE(17, 18, [
                    SPACE(17, 18),
                  ]),
                  // `a`
                  bound_declaration_name(18, 19, [
                    // `a`
                    singular_identifier(18, 19),
                  ]),
                  // `10`
                  expression(20, 22, [
                    // `10`
                    integer(20, 22, [
                      // `10`
                      integer_decimal(20, 22),
                    ]),
                  ]),
                ]),
                WHITESPACE(22, 23, [
                  SPACE(22, 23),
                ]),
              ]),
            ]),
          ])
        ]
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
        tokens: [
          // `if(true){ Int a=10 call my_task{input:foo=a} call no_inputs{} }`
          workflow_conditional(0, 75, [
            // `true`
            workflow_conditional_condition(3, 7, [
              // `true`
              expression(3, 7, [
                // `true`
                boolean(3, 7),
              ]),
            ]),
            WHITESPACE(9, 10, [
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
            // `Int a=10`
            workflow_execution_statement(14, 27, [
              // `Int a=10`
              private_declarations(14, 27, [
                // `Int a=10`
                bound_declaration(14, 22, [
                  // `Int`
                  wdl_type(14, 17, [
                    // `Int`
                    int_type(14, 17),
                  ]),
                  WHITESPACE(17, 18, [
                    SPACE(17, 18),
                  ]),
                  // `a`
                  bound_declaration_name(18, 19, [
                    // `a`
                    singular_identifier(18, 19),
                  ]),
                  // `10`
                  expression(20, 22, [
                    // `10`
                    integer(20, 22, [
                      // `10`
                      integer_decimal(20, 22),
                    ]),
                  ]),
                ]),
                WHITESPACE(22, 23, [
                  NEWLINE(22, 23),
                ]),
                WHITESPACE(23, 24, [
                  SPACE(23, 24),
                ]),
                WHITESPACE(24, 25, [
                  SPACE(24, 25),
                ]),
                WHITESPACE(25, 26, [
                  SPACE(25, 26),
                ]),
                WHITESPACE(26, 27, [
                  SPACE(26, 27),
                ]),
              ]),
            ]),
            // `call my_task{input:foo=a}`
            workflow_execution_statement(27, 52, [
              // `call my_task{input:foo=a}`
              workflow_call(27, 52, [
                WHITESPACE(31, 32, [
                  SPACE(31, 32),
                ]),
                // `my_task`
                workflow_call_name(32, 39, [
                  // `my_task`
                  singular_identifier(32, 39),
                ]),
                // `{input:foo=a}`
                workflow_call_body(39, 52, [
                  // `foo=a`
                  workflow_call_input(46, 51, [
                    // `foo`
                    singular_identifier(46, 49),
                    // `a`
                    expression(50, 51, [
                      // `a`
                      singular_identifier(50, 51),
                    ]),
                  ]),
                ]),
              ]),
            ]),
            WHITESPACE(52, 53, [
              NEWLINE(52, 53),
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
            // `call no_inputs{}`
            workflow_execution_statement(57, 73, [
              // `call no_inputs{}`
              workflow_call(57, 73, [
                WHITESPACE(61, 62, [
                  SPACE(61, 62),
                ]),
                // `no_inputs`
                workflow_call_name(62, 71, [
                  // `no_inputs`
                  singular_identifier(62, 71),
                ]),
                // `{}`
                workflow_call_body(71, 73),
              ]),
            ]),
            WHITESPACE(73, 74, [
              NEWLINE(73, 74),
            ]),
          ])
        ]
    }
}
