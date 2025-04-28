//! Some basic exploration of various parts of a WDL document.
//!
//! This is intended to be a simple example that, generally speaking, is easy to
//! understand for newcomers to Rust; in a mature application, you definitely
//! want to handle things a bit differently (particularly with regard to error
//! handling!)

use std::fs::read_to_string;
use std::io::IsTerminal;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use clap::Parser;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::Config;
use codespan_reporting::term::emit;
use codespan_reporting::term::termcolor::ColorChoice;
use codespan_reporting::term::termcolor::StandardStream;
use wdl::ast::Ast;
use wdl::ast::AstNode;
use wdl::ast::AstToken;
use wdl::ast::Diagnostic;
use wdl::ast::Document;
use wdl::ast::v1::InputSection;
use wdl::ast::v1::MetadataSection;
use wdl::ast::v1::OutputSection;
use wdl::ast::v1::TaskDefinition;
use wdl::ast::v1::WorkflowDefinition;

/// An example for exploring WDL source files.
#[derive(Parser)]
#[clap(bin_name = "parse")]
struct Args {
    /// The path to the source file to parse.
    path: PathBuf,
}

/// Emits diagnostics.
fn emit_diagnostics(path: &Path, source: &str, diagnostics: &[Diagnostic]) -> Result<()> {
    let file = SimpleFile::new(path.to_str().context("path should be UTF-8")?, source);
    let mut stream = StandardStream::stdout(if std::io::stdout().is_terminal() {
        ColorChoice::Auto
    } else {
        ColorChoice::Never
    });
    for diagnostic in diagnostics.iter() {
        emit(
            &mut stream,
            &Config::default(),
            &file,
            &diagnostic.to_codespan(()),
        )
        .context("failed to emit diagnostic")?;
    }

    Ok(())
}

/// The main function.
pub fn main() -> Result<()> {
    let args = Args::parse();
    let source = read_to_string(&args.path).with_context(|| {
        format!(
            "failed to read source file `{path}`",
            path = args.path.display()
        )
    })?;

    let (document, diagnostics) = Document::parse(&source);
    if !diagnostics.is_empty() {
        emit_diagnostics(&args.path, &source, &diagnostics)?;
    }

    match document.ast() {
        Ast::V1(ast) => {
            let mut tasks = false;
            for (i, task) in ast.tasks().enumerate() {
                tasks = true;

                if i == 0 {
                    println!("# Tasks\n");
                }

                explore_task(&task);
            }

            if tasks {
                println!();
            }

            for (i, workflow) in ast.workflows().enumerate() {
                if i == 0 {
                    println!("# Workflows\n");
                }

                explore_workflow(&workflow);
            }
        }
        Ast::Unsupported => bail!(
            "document `{path}` has an unsupported WDL version",
            path = args.path.display()
        ),
    }

    Ok(())
}

/// Explores metadata.
fn explore_metadata(metadata: &MetadataSection) {
    for item in metadata.items() {
        let value = item.value().text().to_string();
        println!(
            "`{name}`: `{value}`",
            name = item.name().text(),
            value = value.trim()
        );
    }
}

/// Explores an input.
fn explore_input(input: &InputSection) {
    for decl in input.declarations() {
        println!(
            "`{name}`: `{ty}`",
            name = decl.name().text(),
            ty = decl.ty()
        );
    }
}

/// Explores an output.
fn explore_output(output: &OutputSection) {
    for decl in output.declarations() {
        println!(
            "`{name}`: `{ty}`",
            name = decl.name().text(),
            ty = decl.ty()
        );
    }
}

/// Prints the metadata, input, and output sections from a WDL task.
fn explore_task(task: &TaskDefinition) {
    println!("## Task `{name}`", name = task.name().text());

    if let Some(metadata) = task.metadata() {
        println!("\n### Metadata");
        explore_metadata(&metadata);
    }

    if let Some(input) = task.input() {
        println!("\n### Inputs");
        explore_input(&input);
    }

    if let Some(output) = task.output() {
        println!("\n### Outputs");
        explore_output(&output);
    }
}

/// Prints the metadata, input, and output block from a WDL workflow.
fn explore_workflow(workflow: &WorkflowDefinition) {
    println!("## Workflow `{name}`", name = workflow.name().text());

    if let Some(metadata) = workflow.metadata() {
        println!("\n### Metadata");
        explore_metadata(&metadata);
    }

    if let Some(input) = workflow.input() {
        println!("\n### Inputs");
        explore_input(&input);
    }

    if let Some(output) = workflow.output() {
        println!("\n### Outputs");
        explore_output(&output);
    }
}
