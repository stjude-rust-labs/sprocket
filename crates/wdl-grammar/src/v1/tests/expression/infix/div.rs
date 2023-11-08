use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::v1::Parser as WdlParser;
use crate::v1::Rule;

#[test]
fn it_fails_to_parse_an_empty_div() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::div,
        positives: vec![Rule::div],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_value_that_is_not_div() {
    fails_with! {
        parser: WdlParser,
        input: "*",
        rule: Rule::div,
        positives: vec![Rule::div],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_div() {
    parses_to! {
        parser: WdlParser,
        input: "/",
        rule: Rule::div,
        tokens: [div(0, 1)]
    }
}
