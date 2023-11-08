use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::v1::Parser as WdlParser;
use crate::v1::Rule;

#[test]
fn it_fails_to_parse_an_empty_char_unicode() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::char_unicode,
        positives: vec![Rule::char_unicode],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_char_with_only_the_unicode_prefix() {
    fails_with! {
        parser: WdlParser,
        input: "\\u",
        rule: Rule::char_unicode,
        positives: vec![Rule::char_unicode],
        negatives: vec![],
        pos: 0
    }

    fails_with! {
        parser: WdlParser,
        input: "\\U",
        rule: Rule::char_unicode,
        positives: vec![Rule::char_unicode],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_char_escaped_to_char_unicode() {
    fails_with! {
        parser: WdlParser,
        input: "\\\\",
        rule: Rule::char_unicode,
        positives: vec![Rule::char_unicode],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_char_hex_to_char_unicode() {
    fails_with! {
        parser: WdlParser,
        input: "\\xFF",
        rule: Rule::char_unicode,
        positives: vec![Rule::char_unicode],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_char_octal_to_char_unicode() {
    fails_with! {
        parser: WdlParser,
        input: "\\123",
        rule: Rule::char_unicode,
        positives: vec![Rule::char_unicode],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_char_unicode_with_four_hex_characters() {
    parses_to! {
        parser: WdlParser,
        input: "\\uFFFF",
        rule: Rule::char_unicode,
        tokens: [char_unicode(0, 6)]
    }

    parses_to! {
        parser: WdlParser,
        input: "\\UFFFF",
        rule: Rule::char_unicode,
        tokens: [char_unicode(0, 6)]
    }
}

#[test]
fn it_successfully_parses_char_unicode_with_eight_hex_characters() {
    parses_to! {
        parser: WdlParser,
        input: "\\uFFFFFFFF",
        rule: Rule::char_unicode,
        tokens: [char_unicode(0, 10)]
    }

    parses_to! {
        parser: WdlParser,
        input: "\\UFFFFFFFF",
        rule: Rule::char_unicode,
        tokens: [char_unicode(0, 10)]
    }
}

#[test]
fn it_fails_to_parse_a_char_unicode_with_two_hex_characters() {
    fails_with! {
        parser: WdlParser,
        input: "\\uFF",
        rule: Rule::char_unicode,
        positives: vec![Rule::char_unicode],
        negatives: vec![],
        pos: 0
    }

    fails_with! {
        parser: WdlParser,
        input: "\\UFF",
        rule: Rule::char_unicode,
        positives: vec![Rule::char_unicode],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_char_unicode_with_six_hex_characters() {
    fails_with! {
        parser: WdlParser,
        input: "\\uFFFFFF",
        rule: Rule::char_unicode,
        positives: vec![Rule::char_unicode],
        negatives: vec![],
        pos: 0
    }

    fails_with! {
        parser: WdlParser,
        input: "\\UFFFFFF",
        rule: Rule::char_unicode,
        positives: vec![Rule::char_unicode],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_char_unicode_with_ten_hex_characters() {
    fails_with! {
        parser: WdlParser,
        input: "\\uFFFFFFFFFF",
        rule: Rule::char_unicode,
        positives: vec![Rule::char_unicode],
        negatives: vec![],
        pos: 0
    }

    fails_with! {
        parser: WdlParser,
        input: "\\UFFFFFFFFFF",
        rule: Rule::char_unicode,
        positives: vec![Rule::char_unicode],
        negatives: vec![],
        pos: 0
    }
}
