use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::Parser as WdlParser;
use crate::Rule;

#[test]
fn it_fails_to_parse_an_empty_eq() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::eq,
        positives: vec![Rule::eq],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_value_that_is_not_eq() {
    fails_with! {
        parser: WdlParser,
        input: "!=",
        rule: Rule::eq,
        positives: vec![Rule::eq],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_eq() {
    parses_to! {
        parser: WdlParser,
        input: "==",
        rule: Rule::eq,
        tokens: [eq(0, 2)]
    }
}
