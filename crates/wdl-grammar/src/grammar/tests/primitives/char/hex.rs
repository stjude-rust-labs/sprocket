use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::Parser as WdlParser;
use crate::Rule;

#[test]
fn it_fails_to_parse_an_empty_char_hex() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::char_hex,
        positives: vec![Rule::char_hex],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_char_with_only_the_hex_prefix() {
    fails_with! {
        parser: WdlParser,
        input: "\\x",
        rule: Rule::char_hex,
        positives: vec![Rule::char_hex],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_char_escaped_to_char_hex() {
    fails_with! {
        parser: WdlParser,
        input: "\\\\",
        rule: Rule::char_hex,
        positives: vec![Rule::char_hex],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_char_octal_to_char_hex() {
    fails_with! {
        parser: WdlParser,
        input: "\\123",
        rule: Rule::char_hex,
        positives: vec![Rule::char_hex],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_char_unicode_to_char_hex() {
    fails_with! {
        parser: WdlParser,
        input: "\\uFFFF",
        rule: Rule::char_hex,
        positives: vec![Rule::char_hex],
        negatives: vec![],
        pos: 0
    }

    fails_with! {
        parser: WdlParser,
        input: "\\UFFFF",
        rule: Rule::char_hex,
        positives: vec![Rule::char_hex],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_char_hex() {
    parses_to! {
        parser: WdlParser,
        input: "\\xFF",
        rule: Rule::char_hex,
        tokens: [char_hex(0, 4)]
    }
}
