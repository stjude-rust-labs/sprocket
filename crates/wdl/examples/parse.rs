//! Generates a syntax tree, validates it, and then prints the resulting tree.

use std::fs::read_to_string;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use clap::Parser;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::emit;
use codespan_reporting::term::termcolor::ColorChoice;
use codespan_reporting::term::termcolor::StandardStream;
use codespan_reporting::term::Config;
use wdl::ast::Diagnostic;
use wdl::ast::Document;
use wdl::ast::Validator;

/// An example for parsing WDL source files.
#[derive(Parser)]
#[clap(bin_name = "parse")]
struct Args {
    /// The path to the source file to parse.
    path: PathBuf,
}

fn emit_diagnostics(path: &Path, source: &str, diagnostics: &[Diagnostic]) -> Result<()> {
    let file = SimpleFile::new(path.to_str().context("path should be UTF-8")?, source);
    let mut stream = StandardStream::stdout(ColorChoice::Auto);
    for diagnostic in diagnostics.iter() {
        emit(
            &mut stream,
            &Config::default(),
            &file,
            &diagnostic.to_codespan(),
        )
        .context("failed to emit diagnostic")?;
    }

    Ok(())
}

pub fn main() -> Result<()> {
    let args = Args::parse();
    let source = read_to_string(&args.path).with_context(|| {
        format!(
            "failed to read source file `{path}`",
            path = args.path.display()
        )
    })?;

    match Document::parse(&source).into_result() {
        Ok(document) => {
            let validator = Validator::default();
            if let Err(diagnostics) = validator.validate(&document) {
                emit_diagnostics(&args.path, &source, &diagnostics)?;
            } else {
                println!("{document:#?}");
            }
        }
        Err(diagnostics) => {
            emit_diagnostics(&args.path, &source, &diagnostics)?;
        }
    }

    Ok(())
}
