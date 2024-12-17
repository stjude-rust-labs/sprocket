//! Implementation of the check and lint commands.

use std::fs;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use anyhow::bail;
use clap::Parser;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::emit;
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use url::Url;
use wdl::analysis::Analyzer;
use wdl::analysis::path_to_uri;
use wdl::analysis::rules;
use wdl::ast::Diagnostic;
use wdl::ast::Severity;
use wdl::ast::SyntaxNode;
use wdl::ast::Validator;
use wdl::lint::LintVisitor;

use super::Mode;
use super::get_display_config;

/// The delay in showing the progress bar.
const PROGRESS_BAR_DELAY: Duration = Duration::from_secs(2);
/// Common arguments for the `check` and `lint` subcommands.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Common {
    /// The files or directories to check.
    #[arg(required = true)]
    #[clap(value_name = "PATHs and/or URLs")]
    pub files: Vec<String>,

    /// A single rule ID to except from running.
    ///
    /// Can be specified multiple times.
    #[arg(short, long, value_name = "RULE")]
    pub except: Vec<String>,

    /// Causes the command to fail if warnings were reported.
    #[clap(long)]
    pub deny_warnings: bool,

    /// Causes the command to fail if notes were reported.
    #[clap(long)]
    pub deny_notes: bool,

    /// Disables color output.
    #[arg(long)]
    pub no_color: bool,

    /// The report mode.
    #[arg(short = 'm', long, default_value_t, value_name = "MODE")]
    pub report_mode: Mode,
}

/// Arguments for the `check` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct CheckArgs {
    /// The common command line arguments.
    #[command(flatten)]
    pub common: Common,

    /// Perform lint checks in addition to checking for errors.
    #[arg(short, long)]
    pub lint: bool,
}

/// Arguments for the `lint` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct LintArgs {
    /// The command command line arguments.
    #[command(flatten)]
    pub common: Common,
}

/// Checks WDL source files for diagnostics.
pub async fn check(args: CheckArgs) -> anyhow::Result<()> {
    if !args.lint && !args.common.except.is_empty() {
        bail!("cannot specify `--except` without `--lint`");
    }

    let (config, mut stream) = get_display_config(args.common.report_mode, args.common.no_color);
    let exceptions = Arc::new(args.common.except);
    let excepts = exceptions.clone();
    let rules = rules()
        .into_iter()
        .filter(|r| !excepts.iter().any(|e| e == r.id()));
    let lint = args.lint;
    let analyzer = Analyzer::new_with_validator(
        rules,
        move |bar: ProgressBar, kind, completed, total| async move {
            if bar.elapsed() < PROGRESS_BAR_DELAY {
                return;
            }

            if completed == 0 || bar.length() == Some(0) {
                bar.set_length(total.try_into().unwrap());
                bar.set_message(format!("{kind}"));
            }

            bar.set_position(completed.try_into().unwrap());
        },
        move || {
            let mut validator = Validator::empty();

            if lint {
                let visitor = LintVisitor::new(wdl::lint::rules().into_iter().filter_map(|rule| {
                    if exceptions.iter().any(|e| e == rule.id()) {
                        None
                    } else {
                        Some(rule)
                    }
                }));
                validator.add_visitor(visitor);
            }

            validator
        },
    );

    for file in &args.common.files {
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
    }

    let bar = ProgressBar::new(0);
    bar.set_style(
        ProgressStyle::with_template("[{elapsed_precise}] {bar:40.cyan/blue} {msg} {pos}/{len}")
            .unwrap(),
    );

    let results = analyzer
        .analyze(bar.clone())
        .await
        .context("failed to analyze documents")?;

    // Drop (hide) the progress bar before emitting any diagnostics
    drop(bar);

    let cwd = std::env::current_dir().ok();
    let mut error_count = 0;
    let mut warning_count = 0;
    let mut note_count = 0;
    for result in &results {
        // Attempt to strip the CWD from the result path
        let uri = result.document().uri();
        let scheme = uri.scheme();
        let uri = match (cwd.clone(), scheme) {
            (Some(cwd), "file") => uri
                .to_string()
                .strip_prefix(cwd.to_str().unwrap())
                .unwrap_or(
                    uri.to_file_path()
                        .expect("failed to convert file URI to file path")
                        .to_string_lossy()
                        .as_ref(),
                )
                .to_string(),
            (_, "file") => uri
                .to_file_path()
                .expect("failed to convert file URI to file path")
                .to_string_lossy()
                .to_string(),
            _ => uri.to_string(),
        };

        let diagnostics = match result.error() {
            Some(e) => &[Diagnostic::error(format!("failed to read `{uri}`: {e:#}"))],
            None => result.document().diagnostics(),
        };

        if !diagnostics.is_empty() {
            let file = SimpleFile::new(
                uri,
                SyntaxNode::new_root(result.document().node().syntax().green().into())
                    .text()
                    .to_string(),
            );

            for diagnostic in diagnostics.iter() {
                match diagnostic.severity() {
                    Severity::Error => error_count += 1,
                    Severity::Warning => warning_count += 1,
                    Severity::Note => note_count += 1,
                }

                emit(&mut stream, &config, &file, &diagnostic.to_codespan())
                    .context("failed to emit diagnostic")?;
            }
        }
    }

    if error_count > 0 {
        bail!(
            "failing due to {error_count} error{s}",
            s = if error_count == 1 { "" } else { "s" }
        );
    } else if args.common.deny_warnings && warning_count > 0 {
        bail!(
            "failing due to {warning_count} warning{s} (`--deny-warnings` was specified)",
            s = if warning_count == 1 { "" } else { "s" }
        );
    } else if args.common.deny_notes && note_count > 0 {
        bail!(
            "failing due to {note_count} note{s} (`--deny-notes` was specified)",
            s = if note_count == 1 { "" } else { "s" }
        );
    }

    Ok(())
}

/// Lints WDL source files.
pub async fn lint(args: LintArgs) -> anyhow::Result<()> {
    check(CheckArgs {
        common: args.common,
        lint: true,
    })
    .await
}
