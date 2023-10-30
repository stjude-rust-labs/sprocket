use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::Parser as WdlParser;
use crate::Rule;

#[test]
fn it_fails_to_parse_an_empty_neq() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::neq,
        positives: vec![Rule::neq],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_value_that_is_not_neq() {
    fails_with! {
        parser: WdlParser,
        input: "==",
        rule: Rule::neq,
        positives: vec![Rule::neq],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_neq() {
    parses_to! {
        parser: WdlParser,
        input: "!=",
        rule: Rule::neq,
        tokens: [neq(0, 2)]
    }
}
