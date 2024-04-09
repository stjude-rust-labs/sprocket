//! Some basic exploration of various parts of a WDL document.
//!
//! This is intended to be a simple example that, generally speaking, is easy to
//! understand for newcomers to Rust—in a mature application, you definitely
//! want to handle things a bit differently (particularly with regard to error
//! handling!)

use ast::v1::document::Task;
use ast::v1::document::Workflow;
use wdl::ast;
use wdl::grammar;

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    let src = std::env::args().nth(1).expect("missing src");
    let contents = std::fs::read_to_string(src).expect("could not read file contents");

    // Generate the parse tree from the document source.
    let parse_tree = grammar::v1::parse(&contents)
        .expect("creating parse tree from WDL document failed")
        .into_tree()
        .expect("could not convert to parse tree");

    // Generate the abstract syntax tree from the parse tree.
    let syntax_tree = ast::v1::parse(parse_tree)
        .expect("creating abstract syntax tree from WDL document failed")
        .into_tree()
        .expect("could not convert to abstract syntax tree");

    // Print details for all of the tasks in the WDL document, if any exist.
    let tasks = syntax_tree.tasks();

    if !tasks.is_empty() {
        println!("# Tasks\n");

        for task in tasks {
            explore_task(task);
        }
    }

    // Print details for the workflow in the document, if it exists.
    if let Some(workflow) = syntax_tree.workflow() {
        if !tasks.is_empty() {
            println!();
        }

        println!("# Workflow\n");
        explore_workflow(workflow);
    }

    Ok(())
}

/// Prints the metadata, input, and output block from a WDL task.
fn explore_task(task: &Task) {
    println!("## `{}`", task.name());

    if let Some(metadata) = task.metadata() {
        println!();
        debug_block("Metadata", metadata);
    }

    if let Some(input) = task.input() {
        println!();
        debug_block("Input", input);
    }

    if let Some(output) = task.output() {
        println!();
        debug_block("Output", output);
    }
}

/// Prints the metadata, input, and output block from a WDL workflow.
fn explore_workflow(workflow: &Workflow) {
    println!("## `{}`", workflow.name());

    if let Some(metadata) = workflow.metadata() {
        println!();
        debug_block("Metadata", metadata);
    }

    if let Some(input) = workflow.input() {
        println!();
        debug_block("Input", input);
    }

    if let Some(output) = workflow.output() {
        println!();
        debug_block("Output", output);
    }
}

/// Prints a particular WDL block as Markdown.
fn debug_block(title: &'static str, block: impl std::fmt::Debug) {
    println!("### {}\n", title);
    println!("```rust");
    println!("{:#?}", block);
    println!("```");
}
