use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::v1::Parser as WdlParser;
use crate::v1::Rule;

#[test]
fn it_fails_to_parse_an_empty_boolean() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::boolean,
        positives: vec![Rule::boolean],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_false() {
    parses_to! {
        parser: WdlParser,
        input: "false",
        rule: Rule::boolean,
        tokens: [boolean(0, 5)]
    }
}

#[test]
fn it_successfully_parses_true() {
    parses_to! {
        parser: WdlParser,
        input: "true",
        rule: Rule::boolean,
        tokens: [boolean(0, 4)]
    }
}
