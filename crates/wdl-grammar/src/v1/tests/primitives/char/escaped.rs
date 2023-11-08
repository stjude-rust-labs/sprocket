use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::v1::Parser as WdlParser;
use crate::v1::Rule;

#[test]
fn it_fails_to_parse_an_empty_char_escaped() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::char_escaped,
        positives: vec![Rule::char_escaped],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_char_hex_to_char_escaped() {
    fails_with! {
        parser: WdlParser,
        input: "\\xFF",
        rule: Rule::char_escaped,
        positives: vec![Rule::char_escaped],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_a_char_octal_to_char_escaped() {
    fails_with! {
        parser: WdlParser,
        input: "\\123",
        rule: Rule::char_escaped,
        positives: vec![Rule::char_escaped],
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
fn it_successfully_parses_an_escaped_backslash() {
    parses_to! {
        parser: WdlParser,
        input: "\\\\",
        rule: Rule::char_escaped,
        tokens: [char_escaped(0, 2)]
    }
}

#[test]
fn it_successfully_parses_an_escaped_double_quote() {
    parses_to! {
        parser: WdlParser,
        input: "\\\"",
        rule: Rule::char_escaped,
        tokens: [char_escaped(0, 2)]
    }
}

#[test]
fn it_successfully_parses_an_escaped_single_quote() {
    parses_to! {
        parser: WdlParser,
        input: "\\\'",
        rule: Rule::char_escaped,
        tokens: [char_escaped(0, 2)]
    }
}

#[test]
fn it_successfully_parses_an_escaped_newline() {
    parses_to! {
        parser: WdlParser,
        input: "\\n",
        rule: Rule::char_escaped,
        tokens: [char_escaped(0, 2)]
    }
}

#[test]
fn it_successfully_parses_an_escaped_carriage_return() {
    parses_to! {
        parser: WdlParser,
        input: "\\r",
        rule: Rule::char_escaped,
        tokens: [char_escaped(0, 2)]
    }
}

#[test]
fn it_successfully_parses_an_escaped_backspace() {
    parses_to! {
        parser: WdlParser,
        input: "\\b",
        rule: Rule::char_escaped,
        tokens: [char_escaped(0, 2)]
    }
}

#[test]
fn it_successfully_parses_an_escaped_tab() {
    parses_to! {
        parser: WdlParser,
        input: "\\t",
        rule: Rule::char_escaped,
        tokens: [char_escaped(0, 2)]
    }
}

#[test]
fn it_successfully_parses_an_escaped_form_feed() {
    parses_to! {
        parser: WdlParser,
        input: "\\f",
        rule: Rule::char_escaped,
        tokens: [char_escaped(0, 2)]
    }
}

#[test]
fn it_successfully_parses_an_escaped_backslash_a() {
    parses_to! {
        parser: WdlParser,
        input: "\\a",
        rule: Rule::char_escaped,
        tokens: [char_escaped(0, 2)]
    }
}

#[test]
fn it_successfully_parses_an_escaped_backslash_v() {
    parses_to! {
        parser: WdlParser,
        input: "\\v",
        rule: Rule::char_escaped,
        tokens: [char_escaped(0, 2)]
    }
}

#[test]
fn it_successfully_parses_an_escaped_backslash_question_mark() {
    parses_to! {
        parser: WdlParser,
        input: "\\?",
        rule: Rule::char_escaped,
        tokens: [char_escaped(0, 2)]
    }
}
