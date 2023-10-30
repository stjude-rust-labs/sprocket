use pest::consumes_to;
use pest::parses_to;

use crate::Parser as WdlParser;
use crate::Rule;

mod negation;
mod unary_signed;

#[test]
fn it_successfully_parses_negation() {
    parses_to! {
        parser: WdlParser,
        input: "!",
        rule: Rule::prefix,
        tokens: [negation(0, 1)]
    }
}

#[test]
fn it_successfully_parses_positive_unary() {
    parses_to! {
        parser: WdlParser,
        input: "+",
        rule: Rule::prefix,
        tokens: [unary_signed(0, 1)]
    }
}

#[test]
fn it_successfully_parses_negative_unary() {
    parses_to! {
        parser: WdlParser,
        input: "-",
        rule: Rule::prefix,
        tokens: [unary_signed(0, 1)]
    }
}
