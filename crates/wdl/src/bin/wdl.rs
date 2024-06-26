use std::fs;
use std::io::IsTerminal;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;

use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use clap::Args;
use clap::Parser;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::emit;
use codespan_reporting::term::termcolor::ColorChoice;
use codespan_reporting::term::termcolor::StandardStream;
use codespan_reporting::term::Config;
use colored::Colorize;
use wdl::ast::Diagnostic;
use wdl::ast::Document;
use wdl::ast::Validator;
use wdl::lint::rules;
use wdl::lint::ExceptVisitor;

/// Emits the given diagnostics to the output stream.
///
/// The use of color is determined by the presence of a terminal.
///
/// In the future, we might want the color choice to be a CLI argument.
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

/// Reads source from the given path.
///
/// If the path is simply `-`, the source is read from STDIN.
fn read_source(path: &Path) -> Result<String> {
    if path.as_os_str() == "-" {
        let mut source = String::new();
        std::io::stdin()
            .read_to_string(&mut source)
            .context("failed to read source from stdin")?;
        Ok(source)
    } else {
        Ok(fs::read_to_string(path).with_context(|| {
            format!("failed to read source file `{path}`", path = path.display())
        })?)
    }
}

/// Parses a WDL source file and prints the syntax tree.
#[derive(Args)]
#[clap(disable_version_flag = true)]
pub struct ParseCommand {
    /// The path to the source WDL file.
    #[clap(value_name = "PATH")]
    pub path: PathBuf,
}

impl ParseCommand {
    fn exec(self) -> Result<()> {
        let source = read_source(&self.path)?;
        let parse = Document::parse(&source);
        if !parse.diagnostics().is_empty() {
            emit_diagnostics(&self.path, &source, parse.diagnostics())?;
        }

        println!("{document:#?}", document = parse.document());
        Ok(())
    }
}

/// Checks a WDL source file for errors.
#[derive(Args)]
#[clap(disable_version_flag = true)]
pub struct CheckCommand {
    /// The path to the source WDL file.
    #[clap(value_name = "PATH")]
    pub path: PathBuf,
}

impl CheckCommand {
    fn exec(self) -> Result<()> {
        let source = read_source(&self.path)?;
        match Document::parse(&source).into_result() {
            Ok(document) => {
                let validator = Validator::default();
                if let Err(diagnostics) = validator.validate(&document) {
                    emit_diagnostics(&self.path, &source, &diagnostics)?;

                    bail!(
                        "aborting due to previous {count} diagnostic{s}",
                        count = diagnostics.len(),
                        s = if diagnostics.len() == 1 { "" } else { "s" }
                    );
                }
            }
            Err(diagnostics) => {
                emit_diagnostics(&self.path, &source, &diagnostics)?;

                bail!(
                    "aborting due to previous {count} diagnostic{s}",
                    count = diagnostics.len(),
                    s = if diagnostics.len() == 1 { "" } else { "s" }
                );
            }
        }

        Ok(())
    }
}

/// Runs lint rules against a WDL source file.
#[derive(Args)]
#[clap(disable_version_flag = true)]
pub struct LintCommand {
    /// The path to the source WDL file.
    #[clap(value_name = "PATH")]
    pub path: PathBuf,
}

impl LintCommand {
    fn exec(self) -> Result<()> {
        let source = read_source(&self.path)?;
        match Document::parse(&source).into_result() {
            Ok(document) => {
                let mut validator = Validator::default();
                validator.add_visitor(ExceptVisitor::new(rules().iter().map(AsRef::as_ref)));
                if let Err(diagnostics) = validator.validate(&document) {
                    emit_diagnostics(&self.path, &source, &diagnostics)?;

                    bail!(
                        "aborting due to previous {count} diagnostic{s}",
                        count = diagnostics.len(),
                        s = if diagnostics.len() == 1 { "" } else { "s" }
                    );
                }
            }
            Err(diagnostics) => {
                emit_diagnostics(&self.path, &source, &diagnostics)?;

                bail!(
                    "aborting due to previous {count} diagnostic{s}",
                    count = diagnostics.len(),
                    s = if diagnostics.len() == 1 { "" } else { "s" }
                );
            }
        }

        Ok(())
    }
}

/// A tool for parsing, validating, and linting WDL source code.
#[derive(Parser)]
#[clap(
    bin_name = "wdl",
    version,
    propagate_version = true,
    arg_required_else_help = true
)]
enum App {
    Parse(ParseCommand),
    Check(CheckCommand),
    Lint(LintCommand),
}

fn main() -> Result<()> {
    if let Err(e) = match App::parse() {
        App::Parse(cmd) => cmd.exec(),
        App::Check(cmd) => cmd.exec(),
        App::Lint(cmd) => cmd.exec(),
    } {
        eprintln!(
            "{error}: {e:?}",
            error = if std::io::stderr().is_terminal() {
                "error".red().bold()
            } else {
                "error".normal()
            }
        );
        std::process::exit(1);
    }

    Ok(())
}
