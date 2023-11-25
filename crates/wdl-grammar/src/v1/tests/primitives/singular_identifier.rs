use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::v1::Parser as WdlParser;
use crate::v1::Rule;

#[test]
fn it_fails_to_parse_an_empty_identifier() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::singular_identifier,
        positives: vec![Rule::singular_identifier],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_an_identifier_starting_with_a_number() {
    fails_with! {
        parser: WdlParser,
        input: "0hello",
        rule: Rule::singular_identifier,
        positives: vec![Rule::singular_identifier],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fail_to_parse_an_identifier_with_a_unicode_character() {
    fails_with! {
        parser: WdlParser,
        input: "😀",
        rule: Rule::singular_identifier,
        positives: vec![Rule::singular_identifier],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_part_of_an_identifier_with_a_dash() {
    // This test will succeed, as `hello` matches the pattern, but the `-world`
    // part will not be included. This is fine for parsing, as the now unmatched
    // `-world` will throw an error.

    parses_to! {
        parser: WdlParser,
        input: "hello-world",
        rule: Rule::singular_identifier,
        tokens: [singular_identifier(0, 5)]
    }
}

#[test]
fn it_successfully_parses_part_of_an_identifier_with_a_space() {
    // This test will succeed, as `hello` matches the pattern, but the ` world`
    // part will not be included. This is fine for parsing, as the now unmatched
    // ` world` will throw an error.

    parses_to! {
        parser: WdlParser,
        input: "hello world",
        rule: Rule::singular_identifier,
        tokens: [singular_identifier(0, 5)]
    }
}

#[test]
fn it_successfully_parses_an_identifer() {
    parses_to! {
        parser: WdlParser,
        input: "testing",
        rule: Rule::singular_identifier,
        tokens: [singular_identifier(0, 7)]
    }
}
