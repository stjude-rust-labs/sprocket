use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::v1::Parser as WdlParser;
use crate::v1::Rule;

mod e;
mod simple;
mod with_decimal;
mod without_decimal;

#[test]
fn it_fails_to_parse_an_empty_string() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::float,
        positives: vec![Rule::float],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_an_ascii_string() {
    fails_with! {
        parser: WdlParser,
        input: "helloworld",
        rule: Rule::float,
        positives: vec![Rule::float],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_float_with_decimal() {
    parses_to! {
        parser: WdlParser,
        input: "1000.0e10",
        rule: Rule::float,
        tokens: [float(0, 9, [
            float_with_decimal(0, 9)
        ])]
    }
}

#[test]
fn it_successfully_parses_float_without_decimal() {
    parses_to! {
        parser: WdlParser,
        input: "1000.e10",
        rule: Rule::float,
        tokens: [float(0, 8, [
            float_without_decimal(0, 8)
        ])]
    }
}

#[test]
fn it_successfully_parses_float_simple() {
    parses_to! {
        parser: WdlParser,
        input: "10e+10",
        rule: Rule::float,
        tokens: [float(0, 6, [
            float_simple(0, 6)
        ])]
    }
}
