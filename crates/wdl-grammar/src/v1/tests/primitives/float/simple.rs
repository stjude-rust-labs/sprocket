use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::v1::Parser as WdlParser;
use crate::v1::Rule;

#[test]
fn it_fails_to_parse_an_empty_float_simple() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::float_simple,
        positives: vec![Rule::float_simple],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_decimal_to_float_simple() {
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
fn it_fails_to_parse_an_integer_decimal_to_float_simple() {
    fails_with! {
        parser: WdlParser,
        input: "1000",
        rule: Rule::float_simple,
        positives: vec![Rule::float_simple],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_an_float_e_to_float_simple() {
    fails_with! {
        parser: WdlParser,
        input: "e10",
        rule: Rule::float_simple,
        positives: vec![Rule::float_simple],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_float_simple_with_e_and_plus() {
    parses_to! {
        parser: WdlParser,
        input: "10e+10",
        rule: Rule::float_simple,
        tokens: [float_simple(0, 6)]
    }
}

#[test]
fn it_successfully_parses_float_simple_with_capital_e_and_plus() {
    parses_to! {
        parser: WdlParser,
        input: "10E+10",
        rule: Rule::float_simple,
        tokens: [float_simple(0, 6)]
    }
}

#[test]
fn it_successfully_parses_float_simple_with_e_and_minus() {
    parses_to! {
        parser: WdlParser,
        input: "10e-10",
        rule: Rule::float_simple,
        tokens: [float_simple(0, 6)]
    }
}

#[test]
fn it_successfully_parses_float_simple_with_capital_e_and_minus() {
    parses_to! {
        parser: WdlParser,
        input: "10E-10",
        rule: Rule::float_simple,
        tokens: [float_simple(0, 6)]
    }
}

#[test]
fn it_successfully_parses_float_simple_with_e() {
    parses_to! {
        parser: WdlParser,
        input: "10e10",
        rule: Rule::float_simple,
        tokens: [float_simple(0, 5)]
    }
}

#[test]
fn it_successfully_parses_float_simple_with_capital_e() {
    parses_to! {
        parser: WdlParser,
        input: "10E10",
        rule: Rule::float_simple,
        tokens: [float_simple(0, 5)]
    }
}
