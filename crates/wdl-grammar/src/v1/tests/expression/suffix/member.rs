use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::v1::Parser as WdlParser;
use crate::v1::Rule;

#[test]
fn it_fails_to_parse_an_empty_member() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::member,
        positives: vec![Rule::member],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_value_that_is_not_member() {
    fails_with! {
        parser: WdlParser,
        input: "[1]",
        rule: Rule::member,
        positives: vec![Rule::member],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_member_from_an_expression() {
    fails_with! {
        parser: WdlParser,
        input: ".(if a then b else c)",
        rule: Rule::member,
        positives: vec![Rule::singular_identifier],
        negatives: vec![],
        pos: 1
    }
}

#[test]
fn it_successfully_parses_a_member() {
    parses_to! {
        parser: WdlParser,
        input: ".field",
        rule: Rule::member,
        tokens: [member(0, 6, [
            singular_identifier(1, 6)
        ])]
    }
}

#[test]
fn it_successfully_parses_a_member_without_including_the_trailing_space() {
    parses_to! {
        parser: WdlParser,
        input: ".field ",
        rule: Rule::member,
        tokens: [member(0, 6, [
            singular_identifier(1, 6)
        ])]
    }
}
