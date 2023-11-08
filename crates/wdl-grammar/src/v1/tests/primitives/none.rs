use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::v1::Parser as WdlParser;
use crate::v1::Rule;

#[test]
fn it_fails_to_parse_an_empty_none() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::none,
        positives: vec![Rule::none],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_an_emptyne() {
    parses_to! {
        parser: WdlParser,
        input: "None",
        rule: Rule::none,
        tokens: [none(0, 4)]
    }
}
