use pest::consumes_to;
use pest::parses_to;

use crate::Parser as WdlParser;
use crate::Rule;

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
        tokens: [infix(0, 1, [add(0, 1)])]
    }
}

#[test]
fn it_successfully_parses_and() {
    parses_to! {
        parser: WdlParser,
        input: "&&",
        rule: Rule::infix,
        tokens: [infix(0, 2, [and(0, 2)])]
    }
}

#[test]
fn it_successfully_parses_div() {
    parses_to! {
        parser: WdlParser,
        input: "/",
        rule: Rule::infix,
        tokens: [infix(0, 1, [div(0, 1)])]
    }
}

#[test]
fn it_successfully_parses_eq() {
    parses_to! {
        parser: WdlParser,
        input: "==",
        rule: Rule::infix,
        tokens: [infix(0, 2, [eq(0, 2)])]
    }
}

#[test]
fn it_successfully_parses_gt() {
    parses_to! {
        parser: WdlParser,
        input: ">",
        rule: Rule::infix,
        tokens: [infix(0, 1, [gt(0, 1)])]
    }
}

#[test]
fn it_successfully_parses_gte() {
    parses_to! {
        parser: WdlParser,
        input: ">=",
        rule: Rule::infix,
        tokens: [infix(0, 2, [gte(0, 2)])]
    }
}

#[test]
fn it_successfully_parses_lt() {
    parses_to! {
        parser: WdlParser,
        input: "<",
        rule: Rule::infix,
        tokens: [infix(0, 1, [lt(0, 1)])]
    }
}

#[test]
fn it_successfully_parses_lte() {
    parses_to! {
        parser: WdlParser,
        input: "<=",
        rule: Rule::infix,
        tokens: [infix(0, 2, [lte(0, 2)])]
    }
}

#[test]
fn it_successfully_parses_mul() {
    parses_to! {
        parser: WdlParser,
        input: "*",
        rule: Rule::infix,
        tokens: [infix(0, 1, [mul(0, 1)])]
    }
}

#[test]
fn it_successfully_parses_neq() {
    parses_to! {
        parser: WdlParser,
        input: "!=",
        rule: Rule::infix,
        tokens: [infix(0, 2, [neq(0, 2)])]
    }
}

#[test]
fn it_successfully_parses_or() {
    parses_to! {
        parser: WdlParser,
        input: "||",
        rule: Rule::infix,
        tokens: [infix(0, 2, [or(0, 2)])]
    }
}

#[test]
fn it_successfully_parses_remainder() {
    parses_to! {
        parser: WdlParser,
        input: "%",
        rule: Rule::infix,
        tokens: [infix(0, 1, [remainder(0, 1)])]
    }
}

#[test]
fn it_successfully_parses_sub() {
    parses_to! {
        parser: WdlParser,
        input: "-",
        rule: Rule::infix,
        tokens: [infix(0, 1, [sub(0, 1)])]
    }
}
