use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::v1::Parser as WdlParser;
use crate::v1::Rule;

#[test]
fn it_fails_to_parse_an_empty_float_without_decimal() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::float_without_decimal,
        positives: vec![Rule::float_without_decimal],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_decimal_to_float_without_decimal() {
    fails_with! {
        parser: WdlParser,
        input: ".",
        rule: Rule::float_without_decimal,
        positives: vec![Rule::float_without_decimal],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_an_integer_decimal_to_float_without_decimal() {
    fails_with! {
        parser: WdlParser,
        input: "1000",
        rule: Rule::float_without_decimal,
        positives: vec![Rule::float_without_decimal],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_float_with_decimal_with_no_numbers_before_the_decimal() {
    fails_with! {
        parser: WdlParser,
        input: ".0",
        rule: Rule::float_without_decimal,
        positives: vec![Rule::float_without_decimal],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_float_with_decimal_with_numbers_before_the_decimal() {
    fails_with! {
        parser: WdlParser,
        input: "1000.0",
        rule: Rule::float_without_decimal,
        positives: vec![Rule::float_without_decimal],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_float_without_decimal() {
    parses_to! {
        parser: WdlParser,
        input: "1000.",
        rule: Rule::float_without_decimal,
        tokens: [float_without_decimal(0, 5)]
    }
}

#[test]
fn it_successfully_parses_float_with_decimal_with_e() {
    parses_to! {
        parser: WdlParser,
        input: "1000.e+10",
        rule: Rule::float_without_decimal,
        tokens: [float_without_decimal(0, 9)]
    }
}
