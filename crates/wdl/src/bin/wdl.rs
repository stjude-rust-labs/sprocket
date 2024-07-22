use std::borrow::Cow;
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
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use wdl::ast::Diagnostic;
use wdl::ast::Document;
use wdl::ast::SyntaxNode;
use wdl::ast::Validator;
use wdl::lint::LintVisitor;
use wdl_analysis::AnalysisEngine;
use wdl_analysis::AnalysisResult;

/// Emits the given diagnostics to the output stream.
///
/// The use of color is determined by the presence of a terminal.
///
/// In the future, we might want the color choice to be a CLI argument.
fn emit_diagnostics(path: &str, source: &str, diagnostics: &[Diagnostic]) -> Result<()> {
    let file = SimpleFile::new(path, source);
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

async fn analyze(path: &Path, lint: bool) -> Result<Vec<AnalysisResult>> {
    let bar = ProgressBar::new(0);
    bar.set_style(
        ProgressStyle::with_template("[{elapsed_precise}] {bar:40.cyan/blue} {msg} {pos}/{len}")
            .unwrap(),
    );

    let engine = AnalysisEngine::new_with_validator(move || {
        let mut validator = Validator::default();
        if lint {
            validator.add_visitor(LintVisitor::default());
        }
        validator
    })?;

    let results = engine
        .analyze_with_progress(path, move |kind, completed, total| {
            if completed == 0 {
                bar.set_length(total.try_into().unwrap());
                bar.set_message(format!("{kind}"));
            }
            bar.set_position(completed.try_into().unwrap());
        })
        .await;

    let mut count = 0;
    let cwd = std::env::current_dir().ok();
    for result in &results {
        let path = result.id().path();

        // Attempt to strip the CWD from the result path
        let path = match (&cwd, &path) {
            // Use the id itself if there is no path
            (_, None) => result.id().to_str(),
            // Use just the path if there's no CWD
            (None, Some(path)) => path.to_string_lossy(),
            // Strip the CWD from the path
            (Some(cwd), Some(path)) => path.strip_prefix(cwd).unwrap_or(path).to_string_lossy(),
        };

        let diagnostics: Cow<'_, [Diagnostic]> = match result.error() {
            Some(e) => vec![Diagnostic::error(format!("failed to read `{path}`: {e:#}"))].into(),
            None => result.diagnostics().into(),
        };

        if !diagnostics.is_empty() {
            emit_diagnostics(
                &path,
                &result
                    .root()
                    .map(|n| SyntaxNode::new_root(n.clone()).text().to_string())
                    .unwrap_or(String::new()),
                &diagnostics,
            )?;
            count += diagnostics.len();
        }
    }

    engine.shutdown().await;

    if count > 0 {
        bail!(
            "aborting due to previous {count} diagnostic{s}",
            s = if count == 1 { "" } else { "s" }
        );
    }

    Ok(results)
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
    async fn exec(self) -> Result<()> {
        let source = read_source(&self.path)?;
        let (document, diagnostics) = Document::parse(&source);
        if !diagnostics.is_empty() {
            emit_diagnostics(&self.path.to_string_lossy(), &source, &diagnostics)?;
        }

        println!("{document:#?}");
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
    async fn exec(self) -> Result<()> {
        analyze(&self.path, false).await?;
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
    async fn exec(self) -> Result<()> {
        let source = read_source(&self.path)?;
        let (document, diagnostics) = Document::parse(&source);
        if !diagnostics.is_empty() {
            emit_diagnostics(&self.path.to_string_lossy(), &source, &diagnostics)?;

            bail!(
                "aborting due to previous {count} diagnostic{s}",
                count = diagnostics.len(),
                s = if diagnostics.len() == 1 { "" } else { "s" }
            );
        }

        let mut validator = Validator::default();
        validator.add_visitor(LintVisitor::default());
        if let Err(diagnostics) = validator.validate(&document) {
            emit_diagnostics(&self.path.to_string_lossy(), &source, &diagnostics)?;

            bail!(
                "aborting due to previous {count} diagnostic{s}",
                count = diagnostics.len(),
                s = if diagnostics.len() == 1 { "" } else { "s" }
            );
        }

        Ok(())
    }
}

/// Analyzes a WDL source file.
#[derive(Args)]
#[clap(disable_version_flag = true)]
pub struct AnalyzeCommand {
    /// The path to the source WDL file.
    #[clap(value_name = "PATH")]
    pub path: PathBuf,

    /// Whether or not to run lints as part of analysis.
    #[clap(long)]
    pub lint: bool,
}

impl AnalyzeCommand {
    async fn exec(self) -> Result<()> {
        let results = analyze(&self.path, self.lint).await?;
        println!("{:#?}", results);
        Ok(())
    }
}

/// A tool for parsing, validating, and linting WDL source code.
///
/// This command line tool is intended as an entrypoint to work with and develop
/// the `wdl` family of crates. It is not intended to be used by the broader
/// community. If you are interested in a command line tool designed to work
/// with WDL documents more generally, have a look at the `sprocket` command
/// line tool.
///
/// Link: https://github.com/stjude-rust-labs/sprocket
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
    Analyze(AnalyzeCommand),
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::builder()
        .format_module_path(false)
        .format_target(false)
        .init();

    if let Err(e) = match App::parse() {
        App::Parse(cmd) => cmd.exec().await,
        App::Check(cmd) => cmd.exec().await,
        App::Lint(cmd) => cmd.exec().await,
        App::Analyze(cmd) => cmd.exec().await,
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
