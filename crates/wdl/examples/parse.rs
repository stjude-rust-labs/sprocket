//! Generates a syntax tree and then prints the resulting tree.

use std::fs::read_to_string;
use std::io::IsTerminal;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use clap::Parser;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::Config;
use codespan_reporting::term::emit_to_write_style;
use codespan_reporting::term::termcolor::ColorChoice;
use codespan_reporting::term::termcolor::StandardStream;
use wdl::ast::Diagnostic;
use wdl::ast::Document;

/// An example for parsing WDL source files.
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
        emit_to_write_style(
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
        return Ok(());
    }

    println!("{document:#?}");

    Ok(())
}
