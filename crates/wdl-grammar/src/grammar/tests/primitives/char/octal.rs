use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::Parser as WdlParser;
use crate::Rule;

#[test]
fn it_fails_to_parse_an_empty_char_octal() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::char_octal,
        positives: vec![Rule::char_octal],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_char_with_only_the_octal_prefix() {
    fails_with! {
        parser: WdlParser,
        input: "\\",
        rule: Rule::char_octal,
        positives: vec![Rule::char_octal],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_char_escaped_to_char_octal() {
    fails_with! {
        parser: WdlParser,
        input: "\\\\",
        rule: Rule::char_octal,
        positives: vec![Rule::char_octal],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_char_hex_to_char_octal() {
    fails_with! {
        parser: WdlParser,
        input: "\\xFF",
        rule: Rule::char_octal,
        positives: vec![Rule::char_octal],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_char_unicode_to_char_octal() {
    fails_with! {
        parser: WdlParser,
        input: "\\uFFFF",
        rule: Rule::char_octal,
        positives: vec![Rule::char_octal],
        negatives: vec![],
        pos: 0
    }

    fails_with! {
        parser: WdlParser,
        input: "\\UFFFF",
        rule: Rule::char_octal,
        positives: vec![Rule::char_octal],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_char_octal_with_one_number() {
    parses_to! {
        parser: WdlParser,
        input: "\\1",
        rule: Rule::char_octal,
        tokens: [char_octal(0, 2)]
    }
}

#[test]
fn it_successfully_parses_char_octal_with_two_numbers() {
    parses_to! {
        parser: WdlParser,
        input: "\\12",
        rule: Rule::char_octal,
        tokens: [char_octal(0, 3)]
    }
}

#[test]
fn it_successfully_parses_char_octal_with_three_numbers() {
    parses_to! {
        parser: WdlParser,
        input: "\\123",
        rule: Rule::char_octal,
        tokens: [char_octal(0, 4)]
    }
}

#[test]
fn it_fails_to_parse_a_char_octal_with_four_numbers() {
    fails_with! {
        parser: WdlParser,
        input: "\\1234",
        rule: Rule::char_octal,
        positives: vec![Rule::char_octal],
        negatives: vec![],
        pos: 0
    }
}
