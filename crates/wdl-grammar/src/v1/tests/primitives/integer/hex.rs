use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::v1::Parser as WdlParser;
use crate::v1::Rule;

#[test]
fn it_fails_to_parse_an_empty_integer_hex() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::integer_hex,
        positives: vec![Rule::integer_hex],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_an_integer_decimal_to_integer_hex() {
    fails_with! {
        parser: WdlParser,
        input: "1000",
        rule: Rule::integer_hex,
        positives: vec![Rule::integer_hex],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_an_integer_octal_to_integer_hex() {
    fails_with! {
        parser: WdlParser,
        input: "077",
        rule: Rule::integer_hex,
        positives: vec![Rule::integer_hex],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_integer_hex() {
    parses_to! {
        parser: WdlParser,
        input: "0xFF",
        rule: Rule::integer_hex,
        tokens: [integer_hex(0, 4)]
    }
}
