use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::v1::Parser as WdlParser;
use crate::v1::Rule;

#[test]
fn it_fails_to_parse_an_empty_float_e() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::float_e,
        positives: vec![Rule::float_e],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_an_e_and_plus_to_float_e() {
    fails_with! {
        parser: WdlParser,
        input: "e+",
        rule: Rule::float_e,
        positives: vec![Rule::float_e],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_capital_e_and_plus_to_float_e() {
    fails_with! {
        parser: WdlParser,
        input: "E+",
        rule: Rule::float_e,
        positives: vec![Rule::float_e],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_an_e_and_minus_to_float_e() {
    fails_with! {
        parser: WdlParser,
        input: "e-",
        rule: Rule::float_e,
        positives: vec![Rule::float_e],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_capital_e_and_minus_to_float_e() {
    fails_with! {
        parser: WdlParser,
        input: "E-",
        rule: Rule::float_e,
        positives: vec![Rule::float_e],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_an_ascii_string_to_float_e() {
    fails_with! {
        parser: WdlParser,
        input: "ello",
        rule: Rule::float_e,
        positives: vec![Rule::float_e],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_float_e_with_e_and_plus() {
    parses_to! {
        parser: WdlParser,
        input: "e+10",
        rule: Rule::float_e,
        tokens: [float_e(0, 4)]
    }
}

#[test]
fn it_successfully_parses_float_e_with_capital_e_and_plus() {
    parses_to! {
        parser: WdlParser,
        input: "E+10",
        rule: Rule::float_e,
        tokens: [float_e(0, 4)]
    }
}

#[test]
fn it_successfully_parses_float_e_with_e_and_minus() {
    parses_to! {
        parser: WdlParser,
        input: "e-10",
        rule: Rule::float_e,
        tokens: [float_e(0, 4)]
    }
}

#[test]
fn it_successfully_parses_float_e_with_capital_e_and_minus() {
    parses_to! {
        parser: WdlParser,
        input: "E-10",
        rule: Rule::float_e,
        tokens: [float_e(0, 4)]
    }
}

#[test]
fn it_successfully_parses_float_e_with_e() {
    parses_to! {
        parser: WdlParser,
        input: "E10",
        rule: Rule::float_e,
        tokens: [float_e(0, 3)]
    }
}

#[test]
fn it_successfully_parses_float_e_with_capital_e() {
    parses_to! {
        parser: WdlParser,
        input: "e10",
        rule: Rule::float_e,
        tokens: [float_e(0, 3)]
    }
}
