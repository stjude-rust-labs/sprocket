//! The `wdl` command line tool.
//!
//! If you're here and not a developer of the `wdl` family of crates, you're
//! probably looking for
//! [Sprocket](https://github.com/stjude-rust-labs/sprocket) instead.
use std::borrow::Cow;
use std::collections::HashSet;
use std::fs;
use std::io::IsTerminal;
use std::io::Read;
use std::io::stderr;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use clap::Args;
use clap::Parser;
use clap::Subcommand;
use clap_verbosity_flag::Verbosity;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::Config;
use codespan_reporting::term::emit;
use codespan_reporting::term::termcolor::ColorChoice;
use codespan_reporting::term::termcolor::StandardStream;
use colored::Colorize;
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use tracing_log::AsTrace;
use wdl::ast::Diagnostic;
use wdl::ast::Document;
use wdl::ast::SyntaxNode;
use wdl::ast::Validator;
use wdl::lint::LintVisitor;
use wdl_analysis::AnalysisResult;
use wdl_analysis::Analyzer;
use wdl_analysis::Rule;
use wdl_analysis::rules;
use wdl_ast::Node;
use wdl_ast::Severity;
use wdl_format::Formatter;
use wdl_format::element::node::AstNodeFormatExt as _;

/// Emits the given diagnostics to the output stream.
///
/// The use of color is determined by the presence of a terminal.
///
/// In the future, we might want the color choice to be a CLI argument.
fn emit_diagnostics(path: &str, source: &str, diagnostics: &[Diagnostic]) -> Result<usize> {
    let file = SimpleFile::new(path, source);
    let mut stream = StandardStream::stdout(if std::io::stdout().is_terminal() {
        ColorChoice::Auto
    } else {
        ColorChoice::Never
    });

    let mut errors = 0;
    for diagnostic in diagnostics.iter() {
        if diagnostic.severity() == Severity::Error {
            errors += 1;
        }

        emit(
            &mut stream,
            &Config::default(),
            &file,
            &diagnostic.to_codespan(),
        )
        .context("failed to emit diagnostic")?;
    }

    Ok(errors)
}

/// Analyzes a path.
async fn analyze<T: AsRef<dyn Rule>>(
    rules: impl IntoIterator<Item = T>,
    path: PathBuf,
    lint: bool,
) -> Result<Vec<AnalysisResult>> {
    let bar = ProgressBar::new(0);
    bar.set_style(
        ProgressStyle::with_template("[{elapsed_precise}] {bar:40.cyan/blue} {msg} {pos}/{len}")
            .unwrap(),
    );

    let analyzer = Analyzer::new_with_validator(
        rules,
        move |bar: ProgressBar, kind, completed, total| async move {
            bar.set_position(completed.try_into().unwrap());
            if completed == 0 {
                bar.set_length(total.try_into().unwrap());
                bar.set_message(format!("{kind}"));
            }
        },
        move || {
            let mut validator = Validator::default();
            if lint {
                validator.add_visitor(LintVisitor::default());
            }
            validator
        },
    );

    analyzer.add_documents(vec![path]).await?;
    let results = analyzer
        .analyze(bar.clone())
        .await
        .context("failed to analyze documents")?;

    drop(bar);

    let mut errors = 0;
    let cwd = std::env::current_dir().ok();
    for result in &results {
        let path = result.uri().to_file_path().ok();

        // Attempt to strip the CWD from the result path
        let path = match (&cwd, &path) {
            // Use the id itself if there is no path
            (_, None) => result.uri().as_str().into(),
            // Use just the path if there's no CWD
            (None, Some(path)) => path.to_string_lossy(),
            // Strip the CWD from the path
            (Some(cwd), Some(path)) => path.strip_prefix(cwd).unwrap_or(path).to_string_lossy(),
        };

        let diagnostics: Cow<'_, [Diagnostic]> = match result.parse_result().error() {
            Some(e) => vec![Diagnostic::error(format!("failed to read `{path}`: {e:#}"))].into(),
            None => result.diagnostics().into(),
        };

        if !diagnostics.is_empty() {
            errors += emit_diagnostics(
                &path,
                &result
                    .parse_result()
                    .root()
                    .map(|n| SyntaxNode::new_root(n.clone()).text().to_string())
                    .unwrap_or(String::new()),
                &diagnostics,
            )?;
        }
    }

    if errors > 0 {
        bail!(
            "aborting due to previous {errors} error{s}",
            s = if errors == 1 { "" } else { "s" }
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
    /// Executes the `parse` subcommand.
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

/// Represents common analysis options.
#[derive(Args)]
pub struct AnalysisOptions {
    /// Denies all analysis rules by treating them as errors.
    #[clap(long, conflicts_with = "deny", conflicts_with = "except_all")]
    pub deny_all: bool,

    /// Except (ignores) all analysis rules.
    #[clap(long, conflicts_with = "except")]
    pub except_all: bool,

    /// Excepts (ignores) an analysis rule.
    #[clap(long)]
    pub except: Vec<String>,

    /// Denies an analysis rule by treating it as an error.
    #[clap(long)]
    pub deny: Vec<String>,
}

impl AnalysisOptions {
    /// Checks for conflicts in the analysis options.
    pub fn check_for_conflicts(&self) -> Result<()> {
        if let Some(id) = self.except.iter().find(|id| self.deny.contains(*id)) {
            bail!("rule `{id}` cannot be specified for both the `--except` and `--deny`",);
        }

        Ok(())
    }

    /// Converts the analysis options into an analysis rules set.
    pub fn into_rules(self) -> impl Iterator<Item = Box<dyn Rule>> {
        let Self {
            deny_all,
            except_all,
            except,
            deny,
        } = self;

        let except: HashSet<_> = except.into_iter().collect();
        let deny: HashSet<_> = deny.into_iter().collect();

        rules()
            .into_iter()
            .filter(move |r| !except_all && !except.contains(r.id()))
            .map(move |mut r| {
                if deny_all || deny.contains(r.id()) {
                    r.deny();
                }

                r
            })
    }
}

/// Checks a WDL source file for errors.
#[derive(Args)]
#[clap(disable_version_flag = true)]
pub struct CheckCommand {
    /// The path to the source WDL file.
    #[clap(value_name = "PATH")]
    pub path: PathBuf,

    /// The analysis options.
    #[clap(flatten)]
    pub options: AnalysisOptions,
}

impl CheckCommand {
    /// Executes the `check` subcommand.
    async fn exec(self) -> Result<()> {
        self.options.check_for_conflicts()?;
        analyze(self.options.into_rules(), self.path, false).await?;
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
    /// Executes the `lint` subcommand.
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

    /// The analysis options.
    #[clap(flatten)]
    pub options: AnalysisOptions,

    /// Whether or not to run lints as part of analysis.
    #[clap(long)]
    pub lint: bool,
}

impl AnalyzeCommand {
    /// Executes the `analyze` subcommand.
    async fn exec(self) -> Result<()> {
        self.options.check_for_conflicts()?;
        let results = analyze(self.options.into_rules(), self.path, self.lint).await?;
        println!("{:#?}", results);
        Ok(())
    }
}

/// Formats a WDL source file.
#[derive(Args)]
#[clap(disable_version_flag = true)]
pub struct FormatCommand {
    /// The path to the source WDL file.
    #[clap(value_name = "PATH")]
    pub path: PathBuf,
}

impl FormatCommand {
    /// Executes the `format` subcommand.
    async fn exec(self) -> Result<()> {
        let source = read_source(&self.path)?;

        let (document, diagnostics) = Document::parse(&source);
        assert!(diagnostics.is_empty());

        if !diagnostics.is_empty() {
            emit_diagnostics(&self.path.to_string_lossy(), &source, &diagnostics)?;

            bail!(
                "aborting due to previous {count} diagnostic{s}",
                count = diagnostics.len(),
                s = if diagnostics.len() == 1 { "" } else { "s" }
            );
        }

        let document = Node::Ast(document.ast().into_v1().unwrap()).into_format_element();
        let formatter = Formatter::default();

        match formatter.format(&document) {
            Ok(formatted) => print!("{formatted}"),
            Err(err) => bail!(err),
        };

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
struct App {
    /// The subcommand to use.
    #[command(subcommand)]
    command: Command,

    /// The verbosity flags.
    #[command(flatten)]
    verbose: Verbosity,
}

#[derive(Subcommand)]
enum Command {
    /// Parses a WDL file.
    Parse(ParseCommand),

    /// Checks a WDL file.
    Check(CheckCommand),

    /// Lints a WDL file.
    Lint(LintCommand),

    /// Analyzes a WDL workspace.
    Analyze(AnalyzeCommand),

    /// Formats a WDL file.
    Format(FormatCommand),
}

#[tokio::main]
async fn main() -> Result<()> {
    let app = App::parse();

    let subscriber = tracing_subscriber::fmt::Subscriber::builder()
        .with_max_level(app.verbose.log_level_filter().as_trace())
        .with_writer(std::io::stderr)
        .with_ansi(stderr().is_terminal())
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    if let Err(e) = match app.command {
        Command::Parse(cmd) => cmd.exec().await,
        Command::Check(cmd) => cmd.exec().await,
        Command::Lint(cmd) => cmd.exec().await,
        Command::Analyze(cmd) => cmd.exec().await,
        Command::Format(cmd) => cmd.exec().await,
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
