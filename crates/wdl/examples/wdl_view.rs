//! Generates a parse tree and abstract syntax tree and prints the warnings from
//! both trees.

use wdl::ast;
use wdl::grammar;

pub fn main() {
    let src = std::env::args().nth(1).expect("missing src");
    let contents = std::fs::read_to_string(src).expect("could not read file contents");

    let mut all_concerns = Vec::new();

    let pt = grammar::v1::parse(&contents).unwrap();
    if let Some(concerns) = pt.concerns().cloned() {
        all_concerns.extend(concerns.into_inner());
    }

    let ast = ast::v1::parse(pt.into_tree().unwrap()).unwrap();
    if let Some(concerns) = ast.concerns().cloned() {
        all_concerns.extend(concerns.into_inner());
    }

    for concern in all_concerns {
        eprintln!("{}", concern);
    }

    dbg!(ast);
}
