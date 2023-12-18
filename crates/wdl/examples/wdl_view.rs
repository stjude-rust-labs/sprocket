//! Generates a parse tree and abstract syntax tree and prints the warnings from
//! both trees.

use wdl::ast;
use wdl::grammar;

pub fn main() -> Result<(), Box<dyn std::error::Error>>{
    let src = std::env::args().nth(1).expect("missing src");
    let contents = std::fs::read_to_string(src).expect("could not read file contents");

    // Concerns are parse errors, validation failures, and lint warnings
    // encountered during grammar and abstract syntax tree parsing.
    let mut concerns = Vec::new();

    // Generate the parse tree.
    let result = grammar::v1::parse(&contents)?;

    // Collect the concerns from grammar parsing.
    if let Some(pt_concerns) = result.concerns() {
        concerns.extend(pt_concerns.clone().into_inner());
    }

    // If grammar parsing was successful, generate the abstract syntax tree
    // (AST) and record concerns from AST parsing.
    if let Some(pt) = result.into_tree() {
        let result = ast::v1::parse(pt)?;

        if let Some(ast_concerns) = result.concerns() {
            concerns.extend(ast_concerns.clone().into_inner());
        }

        // If the AST was successfully parsed, print it.
        if let Some(ast) = result.into_tree() {
            println!("{:#?}", ast);
        }
    }

    // Print any concerns that were recorded during the parsing of the grammar
    // or the abstract syntax tree.
    for concern in concerns {
        eprintln!("{}", concern);
    }

    Ok(())
}
