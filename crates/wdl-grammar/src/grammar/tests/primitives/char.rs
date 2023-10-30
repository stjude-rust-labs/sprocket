use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::Parser as WdlParser;
use crate::Rule;

mod escaped;
mod hex;
mod octal;
mod unicode;

#[test]
fn it_fails_to_parse_an_empty_char_special() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::char_special,
        positives: vec![
            Rule::char_escaped,
            Rule::char_octal,
            Rule::char_hex,
            Rule::char_unicode
        ],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_char_escaped() {
    parses_to! {
        parser: WdlParser,
        input: "\\\\",
        rule: Rule::char_special,
        tokens: [char_escaped(0, 2)]
    }
}

#[test]
fn it_successfully_parses_char_hex() {
    parses_to! {
        parser: WdlParser,
        input: "\\xFF",
        rule: Rule::char_special,
        tokens: [char_hex(0, 4)]
    }
}

#[test]
fn it_successfully_parses_char_octal() {
    parses_to! {
        parser: WdlParser,
        input: "\\123",
        rule: Rule::char_special,
        tokens: [char_octal(0, 4)]
    }
}

#[test]
fn it_successfully_parses_char_unicode() {
    parses_to! {
        parser: WdlParser,
        input: "\\uFFFF",
        rule: Rule::char_special,
        tokens: [char_unicode(0, 6)]
    }

    parses_to! {
        parser: WdlParser,
        input: "\\uFFFF",
        rule: Rule::char_special,
        tokens: [char_unicode(0, 6)]
    }

    parses_to! {
        parser: WdlParser,
        input: "\\uFFFFFFFF",
        rule: Rule::char_special,
        tokens: [char_unicode(0, 10)]
    }

    parses_to! {
        parser: WdlParser,
        input: "\\uFFFFFFFF",
        rule: Rule::char_special,
        tokens: [char_unicode(0, 10)]
    }
}
