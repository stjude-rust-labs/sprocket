use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::v1::Parser as WdlParser;
use crate::v1::Rule;

mod double_quoted_string;
mod single_quoted_string;

#[test]
fn it_fails_to_parse_an_empty_string() {
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
fn it_fails_to_parse_an_ascii_string() {
    fails_with! {
        parser: WdlParser,
        input: "helloworld",
        rule: Rule::string,
        positives: vec![Rule::string],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_single_double_quote() {
    fails_with! {
        parser: WdlParser,
        input: "\"",
        rule: Rule::string,
        positives: vec![Rule::char_special, Rule::string_expression_placeholder_start],
        negatives: vec![],
        pos: 1
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
fn it_successfully_parses_an_empty_double_quoted_string() {
    parses_to! {
        parser: WdlParser,
        input: "\"\"",
        rule: Rule::string,
        tokens: [
            // `""`
            string(0, 2, [
                // `"`
                double_quote(0, 1),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_an_empty_single_quoted_string() {
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
fn it_successfully_parses_a_double_quoted_string_with_a_unicode_character() {
    parses_to! {
        parser: WdlParser,
        input: "\"😀\"",
        rule: Rule::string,
        tokens: [
            // `"😀"`
            string(0, 6, [
                // `"`
                double_quote(0, 1),
                // `😀`
                string_literal_contents(1, 5),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_single_quoted_string_with_a_unicode_character() {
    parses_to! {
        parser: WdlParser,
        input: "'😀'",
        rule: Rule::string,
        tokens: [
            // `'😀'`
            string(0, 6, [
                // `'`
                single_quote(0, 1),
                // `😀`
                string_literal_contents(1, 5),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_double_quoted_string() {
    parses_to! {
        parser: WdlParser,
        input: "\"Hello, world!\"",
        rule: Rule::string,
        tokens: [
            // `"Hello, world!"`
            string(0, 15, [
                // `"`
                double_quote(0, 1),
                // `Hello, world!`
                string_literal_contents(1, 14),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_single_quoted_string() {
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
