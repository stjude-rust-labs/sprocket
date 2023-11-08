use pest::consumes_to;
use pest::parses_to;

use crate::v1::Parser as WdlParser;
use crate::v1::Rule;

mod and;
mod eq;
mod neq;
mod or;

mod add;
mod div;
mod gt;
mod gte;
mod lt;
mod lte;
mod mul;
mod remainder;
mod sub;

#[test]
fn it_successfully_parses_add() {
    parses_to! {
        parser: WdlParser,
        input: "+",
        rule: Rule::infix,
        tokens: [add(0, 1)]
    }
}

#[test]
fn it_successfully_parses_and() {
    parses_to! {
        parser: WdlParser,
        input: "&&",
        rule: Rule::infix,
        tokens: [and(0, 2)]
    }
}

#[test]
fn it_successfully_parses_div() {
    parses_to! {
        parser: WdlParser,
        input: "/",
        rule: Rule::infix,
        tokens: [div(0, 1)]
    }
}

#[test]
fn it_successfully_parses_eq() {
    parses_to! {
        parser: WdlParser,
        input: "==",
        rule: Rule::infix,
        tokens: [eq(0, 2)]
    }
}

#[test]
fn it_successfully_parses_gt() {
    parses_to! {
        parser: WdlParser,
        input: ">",
        rule: Rule::infix,
        tokens: [gt(0, 1)]
    }
}

#[test]
fn it_successfully_parses_gte() {
    parses_to! {
        parser: WdlParser,
        input: ">=",
        rule: Rule::infix,
        tokens: [gte(0, 2)]
    }
}

#[test]
fn it_successfully_parses_lt() {
    parses_to! {
        parser: WdlParser,
        input: "<",
        rule: Rule::infix,
        tokens: [lt(0, 1)]
    }
}

#[test]
fn it_successfully_parses_lte() {
    parses_to! {
        parser: WdlParser,
        input: "<=",
        rule: Rule::infix,
        tokens: [lte(0, 2)]
    }
}

#[test]
fn it_successfully_parses_mul() {
    parses_to! {
        parser: WdlParser,
        input: "*",
        rule: Rule::infix,
        tokens: [mul(0, 1)]
    }
}

#[test]
fn it_successfully_parses_neq() {
    parses_to! {
        parser: WdlParser,
        input: "!=",
        rule: Rule::infix,
        tokens: [neq(0, 2)]
    }
}

#[test]
fn it_successfully_parses_or() {
    parses_to! {
        parser: WdlParser,
        input: "||",
        rule: Rule::infix,
        tokens: [or(0, 2)]
    }
}

#[test]
fn it_successfully_parses_remainder() {
    parses_to! {
        parser: WdlParser,
        input: "%",
        rule: Rule::infix,
        tokens: [remainder(0, 1)]
    }
}

#[test]
fn it_successfully_parses_sub() {
    parses_to! {
        parser: WdlParser,
        input: "-",
        rule: Rule::infix,
        tokens: [sub(0, 1)]
    }
}
