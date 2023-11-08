use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::v1::Parser as WdlParser;
use crate::v1::Rule;

#[test]
fn it_fails_to_parse_an_empty_integer_decimal() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::integer_decimal,
        positives: vec![Rule::integer_decimal],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_an_integer_hex_to_integer_decimal() {
    fails_with! {
        parser: WdlParser,
        input: "0xFF",
        rule: Rule::integer_decimal,
        positives: vec![Rule::integer_decimal],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_an_integer_octal_to_integer_decimal() {
    fails_with! {
        parser: WdlParser,
        input: "077",
        rule: Rule::integer_decimal,
        positives: vec![Rule::integer_decimal],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_doesnt_parse_the_decimal_when_parsing_a_float() {
    parses_to! {
        parser: WdlParser,
        input: "1000.0",
        rule: Rule::integer_decimal,
        tokens: [integer_decimal(0, 4)]
    }
}

#[test]
fn it_successfully_parses_zero_as_integer_decimal() {
    parses_to! {
        parser: WdlParser,
        input: "0",
        rule: Rule::integer_decimal,
        tokens: [integer_decimal(0, 1)]
    }
}

#[test]
fn it_successfully_parses_integer_decimal() {
    parses_to! {
        parser: WdlParser,
        input: "1000",
        rule: Rule::integer_decimal,
        tokens: [integer_decimal(0, 4)]
    }
}
