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
use std::path::absolute;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
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
use url::Url;
use wdl::ast::Diagnostic;
use wdl::ast::Document;
use wdl::ast::Validator;
use wdl::lint::LintVisitor;
use wdl_analysis::AnalysisResult;
use wdl_analysis::Analyzer;
use wdl_analysis::Rule;
use wdl_analysis::path_to_uri;
use wdl_analysis::rules;
use wdl_ast::Node;
use wdl_ast::Severity;
use wdl_doc::document_workspace;
use wdl_engine::Engine;
use wdl_engine::EvaluationError;
use wdl_engine::Inputs;
use wdl_engine::local::LocalTaskExecutionBackend;
use wdl_engine::v1::TaskEvaluator;
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
    file: &str,
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

    if let Ok(url) = Url::parse(file) {
        analyzer.add_document(url).await?;
    } else if fs::metadata(file)
        .with_context(|| format!("failed to read metadata for file `{file}`"))?
        .is_dir()
    {
        analyzer.add_directory(file.into()).await?;
    } else if let Some(url) = path_to_uri(file) {
        analyzer.add_document(url).await?;
    } else {
        bail!("failed to convert `{file}` to a URI", file = file)
    }

    let results = analyzer
        .analyze(bar.clone())
        .await
        .context("failed to analyze documents")?;

    drop(bar);

    let mut errors = 0;
    let cwd = std::env::current_dir().ok();
    for result in results.iter() {
        let path = result.document().uri().to_file_path().ok();

        // Attempt to strip the CWD from the result path
        let path = match (&cwd, &path) {
            // Use the id itself if there is no path
            (_, None) => result.document().uri().as_str().into(),
            // Use just the path if there's no CWD
            (None, Some(path)) => path.to_string_lossy(),
            // Strip the CWD from the path
            (Some(cwd), Some(path)) => path.strip_prefix(cwd).unwrap_or(path).to_string_lossy(),
        };

        let diagnostics: Cow<'_, [Diagnostic]> = match result.error() {
            Some(e) => vec![Diagnostic::error(format!("failed to read `{path}`: {e:#}"))].into(),
            None => result.document().diagnostics().into(),
        };

        if !diagnostics.is_empty() {
            errors += emit_diagnostics(
                &path,
                &result.document().node().syntax().text().to_string(),
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
    /// The path or URL to the source WDL file.
    #[clap(value_name = "PATH or URL")]
    pub file: String,

    /// The analysis options.
    #[clap(flatten)]
    pub options: AnalysisOptions,
}

impl CheckCommand {
    /// Executes the `check` subcommand.
    async fn exec(self) -> Result<()> {
        self.options.check_for_conflicts()?;
        analyze(self.options.into_rules(), &self.file, false).await?;
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
    /// The path or URL to the source WDL file.
    #[clap(value_name = "PATH or URL")]
    pub file: String,

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
        let results = analyze(self.options.into_rules(), &self.file, self.lint).await?;
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

/// Document a workspace.
#[derive(Args)]
#[clap(disable_version_flag = true)]
pub struct DocCommand {
    /// The path to the workspace.
    #[clap(value_name = "PATH")]
    pub path: PathBuf,
}

impl DocCommand {
    /// Executes the `document` subcommand.
    async fn exec(self) -> Result<()> {
        document_workspace(self.path).await
    }
}

/// Runs a WDL workflow or task using local execution.
#[derive(Args)]
#[clap(disable_version_flag = true)]
pub struct RunCommand {
    /// The path or URL to the source WDL file.
    #[clap(value_name = "PATH or URL")]
    pub file: String,

    /// The path to the inputs file; defaults to an empty set of inputs.
    #[clap(short, long, value_name = "INPUTS", conflicts_with = "name")]
    pub inputs: Option<PathBuf>,

    /// The name of the workflow or task to run; defaults to the name specified
    /// in the inputs file; required if the inputs file is not specified.
    #[clap(short, long, value_name = "NAME")]
    pub name: Option<String>,

    /// The task execution output directory; defaults to the task name.
    #[clap(short, long, value_name = "OUTPUT_DIR")]
    pub output: Option<PathBuf>,

    /// Overwrites the task execution output directory if it exists.
    #[clap(long)]
    pub overwrite: bool,

    /// The analysis options.
    #[clap(flatten)]
    pub options: AnalysisOptions,
}

impl RunCommand {
    /// Executes the `check` subcommand.
    async fn exec(self) -> Result<()> {
        self.options.check_for_conflicts()?;

        if Path::new(&self.file).is_dir() {
            bail!("specified path cannot be a directory");
        }

        let results = analyze(self.options.into_rules(), &self.file, false).await?;

        let uri = if let Ok(uri) = Url::parse(&self.file) {
            uri
        } else {
            path_to_uri(&self.file).expect("file should be a local path")
        };

        let result = results
            .iter()
            .find(|r| **r.document().uri() == uri)
            .context("failed to find document in analysis results")?;
        let document = result.document();

        // TODO: support other backends in the future
        let mut engine = Engine::new(LocalTaskExecutionBackend::new());
        let (path, name, inputs) = if let Some(path) = self.inputs {
            let abs_path = absolute(&path).with_context(|| {
                format!(
                    "failed to determine the absolute path of `{path}`",
                    path = path.display()
                )
            })?;
            match Inputs::parse(engine.types_mut(), document, &abs_path)? {
                Some((name, inputs)) => (Some(path), name, inputs),
                None => bail!(
                    "inputs file `{path}` is empty; use the `--name` option to specify the name \
                     of the task or workflow to run",
                    path = path.display()
                ),
            }
        } else if let Some(name) = self.name {
            if document.task_by_name(&name).is_some() {
                (None, name, Inputs::Task(Default::default()))
            } else if document.workflow().is_some() {
                (None, name, Inputs::Workflow(Default::default()))
            } else {
                bail!("document does not contain a task or workflow named `{name}`");
            }
        } else {
            let mut iter = document.tasks();
            let (name, inputs) = iter
                .next()
                .map(|t| (t.name().to_string(), Inputs::Task(Default::default())))
                .or_else(|| {
                    document
                        .workflow()
                        .map(|w| (w.name().to_string(), Inputs::Workflow(Default::default())))
                })
                .context(
                    "inputs file is empty and the WDL document contains no tasks or workflow",
                )?;

            if iter.next().is_some() {
                bail!("inputs file is empty and the WDL document contains more than one task");
            }

            (None, name, inputs)
        };

        let output_dir = self
            .output
            .unwrap_or_else(|| Path::new(&name).to_path_buf());

        // Check to see if the output directory already exists and if it should be
        // removed
        if output_dir.exists() {
            if !self.overwrite {
                bail!(
                    "output directory `{dir}` exists; use the `--overwrite` option to overwrite \
                     its contents",
                    dir = output_dir.display()
                );
            }

            fs::remove_dir_all(&output_dir).with_context(|| {
                format!(
                    "failed to remove output directory `{dir}`",
                    dir = output_dir.display()
                )
            })?;
        }

        match inputs {
            Inputs::Task(mut inputs) => {
                // Make any paths specified in the inputs absolute
                let task = document
                    .task_by_name(&name)
                    .ok_or_else(|| anyhow!("document does not contain a task named `{name}`"))?;

                // Ensure all the paths specified in the inputs file are relative to the file's
                // directory
                if let Some(path) = path.as_ref().and_then(|p| p.parent()) {
                    inputs.join_paths(engine.types_mut(), document, task, path);
                }

                let mut evaluator = TaskEvaluator::new(&mut engine);
                match evaluator
                    .evaluate(document, task, &inputs, &output_dir, &name)
                    .await
                {
                    Ok(evaluated) => {
                        match evaluated.into_result() {
                            Ok(outputs) => {
                                // Buffer the entire output before writing it out in case there are
                                // errors during serialization.
                                let mut buffer = Vec::new();
                                let mut serializer = serde_json::Serializer::pretty(&mut buffer);
                                outputs.serialize(engine.types(), &mut serializer)?;
                                println!(
                                    "{buffer}\n",
                                    buffer = std::str::from_utf8(&buffer)
                                        .expect("output should be UTF-8")
                                );
                            }
                            Err(e) => match e {
                                EvaluationError::Source(diagnostic) => {
                                    emit_diagnostics(
                                        &self.file,
                                        &document.node().syntax().text().to_string(),
                                        &[diagnostic],
                                    )?;

                                    bail!("aborting due to task evaluation failure");
                                }
                                EvaluationError::Other(e) => return Err(e),
                            },
                        }
                    }
                    Err(e) => match e {
                        EvaluationError::Source(diagnostic) => {
                            emit_diagnostics(
                                &self.file,
                                &document.node().syntax().text().to_string(),
                                &[diagnostic],
                            )?;

                            bail!("aborting due to task evaluation failure");
                        }
                        EvaluationError::Other(e) => return Err(e),
                    },
                }
            }
            Inputs::Workflow(mut inputs) => {
                let workflow = document
                    .workflow()
                    .ok_or_else(|| anyhow!("document does not contain a workflow"))?;
                if workflow.name() != name {
                    bail!("document does not contain a workflow named `{name}`");
                }

                // Ensure all the paths specified in the inputs file are relative to the file's
                // directory
                if let Some(path) = path.as_ref().and_then(|p| p.parent()) {
                    inputs.join_paths(engine.types_mut(), document, workflow, path);
                }

                bail!("running workflows is not yet supported")
            }
        }

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

    /// Documents a workspace.
    Doc(DocCommand),

    /// Runs a workflow or task.
    Run(RunCommand),
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
        Command::Doc(cmd) => cmd.exec().await,
        Command::Run(cmd) => cmd.exec().await,
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
