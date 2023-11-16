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
        rule: Rule::array_literal,
        positives: vec![Rule::array_literal],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_an_array_literal_with_spaces_outside_the_input() {
    fails_with! {
        parser: WdlParser,
        input: " [if a then b else c, \"Hello, world!\"] ",
        rule: Rule::array_literal,
        positives: vec![Rule::array_literal],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_an_array_literal() {
    parses_to! {
        parser: WdlParser,
        input: "[if a then b else c,\"Hello, world!\"]",
        rule: Rule::array_literal,
        tokens: [
            // `[if a then b else c,"Hello, world!"]`
            array_literal(0, 36, [
              // `if a then b else c`
              expression(1, 19, [
                // `if a then b else c`
                r#if(1, 19, [
                  WHITESPACE(3, 4, [
                    SPACE(3, 4),
                  ]),
                  // `a`
                  expression(4, 5, [
                    // `a`
                    identifier(4, 5),
                  ]),
                  WHITESPACE(5, 6, [
                    SPACE(5, 6),
                  ]),
                  WHITESPACE(10, 11, [
                    SPACE(10, 11),
                  ]),
                  // `b`
                  expression(11, 12, [
                    // `b`
                    identifier(11, 12),
                  ]),
                  WHITESPACE(12, 13, [
                    SPACE(12, 13),
                  ]),
                  WHITESPACE(17, 18, [
                    SPACE(17, 18),
                  ]),
                  // `c`
                  expression(18, 19, [
                    // `c`
                    identifier(18, 19),
                  ]),
                ]),
              ]),
              // `,`
              COMMA(19, 20),
              // `"Hello, world!"`
              expression(20, 35, [
                // `"Hello, world!"`
                string(20, 35, [
                  // `"`
                  double_quote(20, 21),
                  // `Hello, world!`
                  string_literal_contents(21, 34),
                ]),
              ]),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_an_array_literal_without_the_trailing_space() {
    parses_to! {
        parser: WdlParser,
        input: "[if a then b else c, \"Hello, world!\"] ",
        rule: Rule::array_literal,
        tokens: [
          // `[if a then b else c, "Hello, world!"]`
          array_literal(0, 37, [
            // `if a then b else c`
            expression(1, 19, [
              // `if a then b else c`
              r#if(1, 19, [
                WHITESPACE(3, 4, [
                  SPACE(3, 4),
                ]),
                // `a`
                expression(4, 5, [
                  // `a`
                  identifier(4, 5),
                ]),
                WHITESPACE(5, 6, [
                  SPACE(5, 6),
                ]),
                WHITESPACE(10, 11, [
                  SPACE(10, 11),
                ]),
                // `b`
                expression(11, 12, [
                  // `b`
                  identifier(11, 12),
                ]),
                WHITESPACE(12, 13, [
                  SPACE(12, 13),
                ]),
                WHITESPACE(17, 18, [
                  SPACE(17, 18),
                ]),
                // `c`
                expression(18, 19, [
                  // `c`
                  identifier(18, 19),
                ]),
              ]),
            ]),
            // `,`
            COMMA(19, 20),
            WHITESPACE(20, 21, [
              SPACE(20, 21),
            ]),
            // `"Hello, world!"`
            expression(21, 36, [
              // `"Hello, world!"`
              string(21, 36, [
                // `"`
                double_quote(21, 22),
                // `Hello, world!`
                string_literal_contents(22, 35),
              ]),
            ]),
          ])
        ]
    }
}

#[test]
fn it_successfully_parses_an_array_literal_with_spaces_inside() {
    parses_to! {
        parser: WdlParser,
        input: "[if a then b else c, \"Hello, world!\"]",
        rule: Rule::array_literal,
        tokens: [
            // `[if a then b else c, "Hello, world!"]`
            array_literal(0, 37, [
              // `if a then b else c`
              expression(1, 19, [
                // `if a then b else c`
                r#if(1, 19, [
                  WHITESPACE(3, 4, [
                    SPACE(3, 4),
                  ]),
                  // `a`
                  expression(4, 5, [
                    // `a`
                    identifier(4, 5),
                  ]),
                  WHITESPACE(5, 6, [
                    SPACE(5, 6),
                  ]),
                  WHITESPACE(10, 11, [
                    SPACE(10, 11),
                  ]),
                  // `b`
                  expression(11, 12, [
                    // `b`
                    identifier(11, 12),
                  ]),
                  WHITESPACE(12, 13, [
                    SPACE(12, 13),
                  ]),
                  WHITESPACE(17, 18, [
                    SPACE(17, 18),
                  ]),
                  // `c`
                  expression(18, 19, [
                    // `c`
                    identifier(18, 19),
                  ]),
                ]),
              ]),
              // `,`
              COMMA(19, 20),
              WHITESPACE(20, 21, [
                SPACE(20, 21),
              ]),
              // `"Hello, world!"`
              expression(21, 36, [
                // `"Hello, world!"`
                string(21, 36, [
                  // `"`
                  double_quote(21, 22),
                  // `Hello, world!`
                  string_literal_contents(22, 35),
                ]),
              ]),
            ])
        ]
    }
}
