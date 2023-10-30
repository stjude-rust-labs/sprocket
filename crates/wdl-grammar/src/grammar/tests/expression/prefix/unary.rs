use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::Parser as WdlParser;
use crate::Rule;

#[test]
fn it_fails_to_parse_an_empty_unary_signed() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::unary_signed,
        positives: vec![Rule::unary_signed],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_value_that_is_not_unary_signed() {
    fails_with! {
        parser: WdlParser,
        input: "!",
        rule: Rule::unary_signed,
        positives: vec![Rule::unary_signed],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_positive_unary_signed() {
    parses_to! {
        parser: WdlParser,
        input: "+",
        rule: Rule::unary_signed,
        tokens: [unary_signed(0, 1)]
    }
}

#[test]
fn it_successfully_parses_negative_unary_signed() {
    parses_to! {
        parser: WdlParser,
        input: "-",
        rule: Rule::unary_signed,
        tokens: [unary_signed(0, 1)]
    }
}
