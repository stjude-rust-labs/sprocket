use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::v1::Parser as WdlParser;
use crate::v1::Rule;

#[test]
fn it_fails_to_parse_an_empty_literal() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::literal,
        positives: vec![
            Rule::none,
            Rule::boolean,
            Rule::integer,
            Rule::float,
            Rule::string,
            Rule::singular_identifier,
        ],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_false() {
    parses_to! {
        parser: WdlParser,
        input: "false",
        rule: Rule::literal,
        tokens: [boolean(0, 5)]
    }
}

#[test]
fn it_successfully_parses_true() {
    parses_to! {
        parser: WdlParser,
        input: "true",
        rule: Rule::literal,
        tokens: [boolean(0, 4)]
    }
}

#[test]
fn it_successfully_parses_integer_decimal() {
    parses_to! {
        parser: WdlParser,
        input: "1000",
        rule: Rule::literal,
        tokens: [
            integer(0, 4, [
                integer_decimal(0, 4)
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_integer_hex() {
    parses_to! {
        parser: WdlParser,
        input: "0xFF",
        rule: Rule::literal,
        tokens: [
            integer(0, 4, [
                integer_hex(0, 4)
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_integer_octal() {
    parses_to! {
        parser: WdlParser,
        input: "077",
        rule: Rule::literal,
        tokens: [
            integer(0, 3, [
                integer_octal(0, 3)
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_float_with_decimal() {
    parses_to! {
        parser: WdlParser,
        input: "1000.0e10",
        rule: Rule::literal,
        tokens: [
            float(0, 9, [
                float_with_decimal(0, 9)
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_float_without_decimal() {
    parses_to! {
        parser: WdlParser,
        input: "1000.e10",
        rule: Rule::literal,
        tokens: [
            float(0, 8, [
                float_without_decimal(0, 8)
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_float_simple() {
    parses_to! {
        parser: WdlParser,
        input: "10e+10",
        rule: Rule::literal,
        tokens: [
            float(0, 6, [
                float_simple(0, 6)
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_an_empty_double_quoted_string() {
    parses_to! {
        parser: WdlParser,
        input: "\"\"",
        rule: Rule::literal,
        tokens: [
          // `""`
          string(0, 2, [
            // `"`
            double_quote(0, 1),
            string_inner(1, 1),
          ])
        ]
    }
}

#[test]
fn it_successfully_parses_an_empty_single_quoted_string() {
    parses_to! {
        parser: WdlParser,
        input: "''",
        rule: Rule::literal,
        tokens: [
          // `''`
          string(0, 2, [
            // `'`
            single_quote(0, 1),
            string_inner(1, 1),
          ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_double_quoted_string_with_a_unicode_character() {
    parses_to! {
        parser: WdlParser,
        input: "\"😀\"",
        rule: Rule::literal,
        tokens: [
          // `"😀"`
          string(0, 6, [
            // `"`
            double_quote(0, 1),
            // `😀`
            string_inner(1, 5, [
              // `😀`
              string_literal_contents(1, 5),
            ]),
          ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_single_quoted_string_with_a_unicode_character() {
    parses_to! {
        parser: WdlParser,
        input: "'😀'",
        rule: Rule::literal,
        tokens: [
          // `'😀'`
          string(0, 6, [
            // `'`
            single_quote(0, 1),
            // `😀`
            string_inner(1, 5, [
              // `😀`
              string_literal_contents(1, 5),
            ]),
          ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_double_quoted_string() {
    parses_to! {
        parser: WdlParser,
        input: "\"Hello, world!\"",
        rule: Rule::literal,
        tokens: [
          // `"Hello, world!"`
          string(0, 15, [
            // `"`
            double_quote(0, 1),
            // `Hello, world!`
            string_inner(1, 14, [
              // `Hello, world!`
              string_literal_contents(1, 14),
            ]),
          ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_single_quoted_string() {
    parses_to! {
        parser: WdlParser,
        input: "'Hello, world!'",
        rule: Rule::literal,
        tokens: [
          // `'Hello, world!'`
          string(0, 15, [
            // `'`
            single_quote(0, 1),
            // `Hello, world!`
            string_inner(1, 14, [
              // `Hello, world!`
              string_literal_contents(1, 14),
            ]),
          ])
        ]
    }
}

#[test]
fn it_successfully_parses_none() {
    parses_to! {
        parser: WdlParser,
        input: "None",
        rule: Rule::literal,
        tokens: [none(0, 4)]
    }
}

#[test]
fn it_successfully_parses_an_identifier() {
    parses_to! {
        parser: WdlParser,
        input: "hello_world",
        rule: Rule::literal,
        tokens: [singular_identifier(0, 11)]
    }

    parses_to! {
        parser: WdlParser,
        input: "HelloWorld",
        rule: Rule::literal,
        tokens: [singular_identifier(0, 10)]
    }
}
