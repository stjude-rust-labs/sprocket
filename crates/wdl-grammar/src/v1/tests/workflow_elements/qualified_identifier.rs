use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::v1::Parser as WdlParser;
use crate::v1::Rule;

#[test]
fn it_fails_to_parse_an_empty_string() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::qualified_identifier,
        positives: vec![Rule::identifier],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_an_unqualified_identifier() {
    fails_with! {
        parser: WdlParser,
        input: "foo",
        rule: Rule::qualified_identifier,
        positives: vec![Rule::qualified_identifier],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_an_identifier_followed_by_a_period() {
    fails_with! {
        parser: WdlParser,
        input: "foo.",
        rule: Rule::qualified_identifier,
        positives: vec![Rule::identifier],
        negatives: vec![],
        pos: 4
    }
}

#[test]
fn it_fails_to_parse_an_identifier_proceeded_by_a_period() {
    fails_with! {
        parser: WdlParser,
        input: ".foo",
        rule: Rule::qualified_identifier,
        positives: vec![Rule::identifier],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_a_qualified_identifier() {
    parses_to! {
        parser: WdlParser,
        input: "foo.bar",
        rule: Rule::qualified_identifier,
        tokens: [qualified_identifier(0, 7, [
            identifier(0, 3),
            identifier(4, 7)
        ])]
    }
}

#[test]
fn it_successfully_excludes_trailing_whitespace() {
    parses_to! {
        parser: WdlParser,
        input: "foo.bar   ",
        rule: Rule::qualified_identifier,
        tokens: [qualified_identifier(0, 7, [
            identifier(0, 3),
            identifier(4, 7)
        ])]
    }
}

#[test]
fn it_successfully_parses_a_long_qualified_identifier() {
    parses_to! {
        parser: WdlParser,
        input: "foo.bar.baz.qux.corge.grault",
        rule: Rule::qualified_identifier,
        tokens: [qualified_identifier(0, 28, [
            identifier(0, 3),
            identifier(4, 7),
            identifier(8, 11),
            identifier(12, 15),
            identifier(16, 21),
            identifier(22, 28),
        ])]
    }
}
