use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::Parser as WdlParser;
use crate::Rule;

#[test]
fn it_fails_to_parse_an_empty_string() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::number,
        positives: vec![Rule::integer, Rule::float],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_an_ascii_string() {
    fails_with! {
        parser: WdlParser,
        input: "helloworld",
        rule: Rule::number,
        positives: vec![Rule::integer, Rule::float],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_integer_decimal() {
    parses_to! {
        parser: WdlParser,
        input: "1000",
        rule: Rule::number,
        tokens: [
            integer(0, 4, [
                integer_decimal(0, 4)
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_integer_hex() {
    parses_to! {
        parser: WdlParser,
        input: "0xFF",
        rule: Rule::number,
        tokens: [integer(0, 4, [
            integer_hex(0, 4)
        ])]
    }
}

#[test]
fn it_successfully_parses_integer_octal() {
    parses_to! {
        parser: WdlParser,
        input: "077",
        rule: Rule::number,
        tokens: [integer(0, 3, [
            integer_octal(0, 3)
        ])]
    }
}

#[test]
fn it_successfully_parses_float_with_decimal() {
    parses_to! {
        parser: WdlParser,
        input: "1000.0e10",
        rule: Rule::number,
        tokens: [float(0, 9, [
            float_with_decimal(0, 9)
        ])]
    }
}

#[test]
fn it_successfully_parses_float_without_decimal() {
    parses_to! {
        parser: WdlParser,
        input: "1000.e10",
        rule: Rule::number,
        tokens: [float(0, 8, [
            float_without_decimal(0, 8)
        ])]
    }
}

#[test]
fn it_successfully_parses_float_simple() {
    parses_to! {
        parser: WdlParser,
        input: "10e+10",
        rule: Rule::number,
        tokens: [float(0, 6, [
            float_simple(0, 6)
        ])]
    }
}
