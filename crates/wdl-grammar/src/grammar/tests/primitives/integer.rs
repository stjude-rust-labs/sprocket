use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::Parser as WdlParser;
use crate::Rule;

mod decimal;
mod hex;
mod octal;

#[test]
fn it_fails_to_parse_an_empty_string() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::integer,
        positives: vec![Rule::integer],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_fails_to_parse_an_ascii_string() {
    fails_with! {
        parser: WdlParser,
        input: "helloworld",
        rule: Rule::integer,
        positives: vec![Rule::integer],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_integer_decimal() {
    parses_to! {
        parser: WdlParser,
        input: "1000",
        rule: Rule::integer,
        tokens: [integer(0, 4, [
            integer_decimal(0, 4)
        ])]
    }
}

#[test]
fn it_successfully_parses_integer_hex() {
    parses_to! {
        parser: WdlParser,
        input: "0xFF",
        rule: Rule::integer,
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
        rule: Rule::integer,
        tokens: [integer(0, 3, [
            integer_octal(0, 3)
        ])]
    }
}

#[test]
fn it_only_parses_the_first_integer_when_two_are_given() {
    parses_to! {
        parser: WdlParser,
        input: "10 20",
        rule: Rule::integer,
        tokens: [integer(0, 2, [
            integer_decimal(0, 2)
        ])]
    }
}
