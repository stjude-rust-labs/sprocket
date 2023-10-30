use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::Parser as WdlParser;
use crate::Rule;

#[test]
fn it_fails_to_parse_an_empty_one_or_more() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::ONE_OR_MORE,
        positives: vec![Rule::ONE_OR_MORE],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_one_or_more() {
    parses_to! {
        parser: WdlParser,
        input: "+",
        rule: Rule::ONE_OR_MORE,
        tokens: [ONE_OR_MORE(0, 1)]
    }
}
