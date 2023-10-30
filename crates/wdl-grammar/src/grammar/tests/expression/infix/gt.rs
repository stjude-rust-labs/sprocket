use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::Parser as WdlParser;
use crate::Rule;

#[test]
fn it_fails_to_parse_an_empty_gt() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::gt,
        positives: vec![Rule::gt],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_value_that_is_not_gt() {
    fails_with! {
        parser: WdlParser,
        input: "<",
        rule: Rule::gt,
        positives: vec![Rule::gt],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_gt() {
    parses_to! {
        parser: WdlParser,
        input: ">",
        rule: Rule::gt,
        tokens: [gt(0, 1)]
    }
}
