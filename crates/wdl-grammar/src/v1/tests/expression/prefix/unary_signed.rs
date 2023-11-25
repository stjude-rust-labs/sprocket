use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::v1::Parser as WdlParser;
use crate::v1::Rule;

#[test]
fn it_fails_to_parse_an_empty_unary_signed_positive() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::unary_signed_positive,
        positives: vec![Rule::unary_signed_positive],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_an_empty_unary_signed_negative() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::unary_signed_negative,
        positives: vec![Rule::unary_signed_negative],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_value_that_is_not_unary_signed_positive() {
    fails_with! {
        parser: WdlParser,
        input: "!",
        rule: Rule::unary_signed_positive,
        positives: vec![Rule::unary_signed_positive],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_value_that_is_not_unary_signed_negative() {
    fails_with! {
        parser: WdlParser,
        input: "!",
        rule: Rule::unary_signed_negative,
        positives: vec![Rule::unary_signed_negative],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_a_unary_signed_positive() {
    parses_to! {
        parser: WdlParser,
        input: "+",
        rule: Rule::unary_signed_positive,
        tokens: [unary_signed_positive(0, 1)]
    }
}

#[test]
fn it_successfully_parses_a_unary_signed_negative() {
    parses_to! {
        parser: WdlParser,
        input: "-",
        rule: Rule::unary_signed_negative,
        tokens: [unary_signed_negative(0, 1)]
    }
}
