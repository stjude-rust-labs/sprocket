use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::v1::Parser as WdlParser;
use crate::v1::Rule;

#[test]
fn it_fails_to_parse_an_empty_option() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::OPTION,
        positives: vec![Rule::OPTION],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_option() {
    parses_to! {
        parser: WdlParser,
        input: "?",
        rule: Rule::OPTION,
        tokens: [OPTION(0, 1)]
    }
}
