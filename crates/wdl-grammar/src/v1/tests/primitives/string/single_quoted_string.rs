use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::v1::Parser as WdlParser;
use crate::v1::Rule;

#[test]
fn it_fails_to_parse_an_empty_single_quoted_string() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::string,
        positives: vec![Rule::string],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_single_single_quote() {
    fails_with! {
        parser: WdlParser,
        input: "\'",
        rule: Rule::string,
        positives: vec![Rule::char_special, Rule::string_expression_placeholder_start],
        negatives: vec![],
        pos: 1
    }
}

#[test]
fn it_fails_to_parse_a_string_with_a_newline() {
    fails_with! {
        parser: WdlParser,
        input: "'Hello,\nworld!'",
        rule: Rule::string,
        positives: vec![Rule::char_special, Rule::string_expression_placeholder_start],
        negatives: vec![],
        pos: 7
    }
}

#[test]
fn it_parses_an_empty_single_quoted_string() {
    parses_to! {
        parser: WdlParser,
        input: "''",
        rule: Rule::string,
        tokens: [
            // `''`
            string(0, 2, [
                // `'`
                single_quote(0, 1),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_the_first_two_double_quotes() {
    // This test will succeed, as `''` matches the pattern, but the last single
    // quote will not be included. This is fine for parsing, as the now
    // unmatched `'` will throw an error.

    parses_to! {
        parser: WdlParser,
        input: "'''",
        rule: Rule::string,
        tokens: [
            // `''`
            string(0, 2, [
                // `'`
                single_quote(0, 1),
            ])
        ]
    }
}

#[test]
fn it_parses_a_single_quoted_string() {
    parses_to! {
        parser: WdlParser,
        input: "'Hello, world!'",
        rule: Rule::string,
        tokens: [
            // `'Hello, world!'`
            string(0, 15, [
                // `'`
                single_quote(0, 1),
                // `Hello, world!`
                string_literal_contents(1, 14),
            ])
        ]
    }
}
