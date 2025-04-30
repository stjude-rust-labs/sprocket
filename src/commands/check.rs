//! Implementation of the check and lint commands.

use std::collections::HashSet;
use std::fs;

use anyhow::Context;
use anyhow::bail;
use clap::Parser;
use codespan_reporting::files::SimpleFiles;
use codespan_reporting::term;
use url::Url;
use wdl::analysis;
use wdl::ast::AstNode;
use wdl::ast::Diagnostic;
use wdl::ast::Severity;
use wdl::cli::analyze;
use wdl::lint;

use crate::Mode;
use crate::emit_diagnostics;
use crate::get_display_config;

/// Common arguments for the `check` and `lint` subcommands.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Common {
    /// The file, URL, or directory to check.
    #[arg(required = true)]
    #[clap(value_name = "PATH or URL")]
    pub file: String,

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

    /// Supress diagnostics from imported documents.
    ///
    /// This will only display diagnostics for the document specified by `file`.
    /// If specified with a directory, an error will be raised.
    #[arg(long)]
    pub single_document: bool,

    /// Show diagnostics for remote documents.
    ///
    /// By default, when checking a local document remote diagnostics are
    /// suppressed. This flag will show diagnostics for remote documents.
    /// This flag has no effect when checking a remote document.
    #[arg(long)]
    pub show_remote_diagnostics: bool,

    /// Run the `shellcheck` program on command sections.
    ///
    /// Requires linting to be enabled. This feature is experimental.
    /// False positives may be reported.
    /// If `shellcheck` is not installed, an error will be raised.
    #[arg(long)]
    pub shellcheck: bool,

    /// Hide diagnostics with `note` severity.
    #[arg(long)]
    pub hide_notes: bool,

    /// Disables color output.
    #[arg(long)]
    pub no_color: bool,

    /// The report mode.
    #[arg(short = 'm', long, value_name = "MODE")]
    pub report_mode: Option<Mode>,
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
    let file = &args.common.file;
    let exceptions: Vec<String> = args.common.except;
    let lint = args.lint;
    let shellcheck = args.common.shellcheck;

    if shellcheck && !lint {
        bail!("`--shellcheck` requires `--lint` to be enabled");
    }

    if (args.common.single_document)
        && fs::metadata(file)
            .with_context(|| format!("failed to read metadata for file `{file}`"))
            .map(|m| m.is_dir())
            .unwrap_or_else(|_| false)
    {
        bail!(
            "`--single-document` was specified, but `{file}` is a directory",
            file = file
        );
    }

    let remote_file = Url::parse(file).is_ok();

    if !exceptions.is_empty() {
        // TODO: switch to case-insensitive rule handling when wdl crates are updated
        let analysis_rules: HashSet<String> = analysis::rules()
            .iter()
            .map(|r| r.id().to_string())
            .collect();
        let lint_rules: HashSet<String> =
            lint::rules().iter().map(|r| r.id().to_string()).collect();
        let all_rules: HashSet<_> = analysis_rules.union(&lint_rules).collect();

        let unknown_exceptions: Vec<&str> = exceptions
            .iter()
            .filter(|rule| !all_rules.contains(rule))
            .map(|rule| rule.as_str())
            .collect();

        if !unknown_exceptions.is_empty() {
            let (config, writer) = get_display_config(
                args.common.report_mode.unwrap_or_default(),
                args.common.no_color,
            );
            let mut w_lock = writer.lock();
            let files: SimpleFiles<String, String> = SimpleFiles::new();

            let message = format!(
                "ignoring unknown rule{s} provided via --except: '{rules}'",
                s = if unknown_exceptions.len() == 1 {
                    ""
                } else {
                    "s"
                },
                rules = unknown_exceptions
                    .iter()
                    .map(|r| format!("`{r}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );

            // TODO: revisit case-insensitive rule handling when wdl crates are updated
            let warning = codespan_reporting::diagnostic::Diagnostic::warning()
                .with_message(message)
                .with_notes(vec![
                    "run `sprocket explain --help` to see available rules".to_string(),
                ]);

            term::emit(&mut w_lock, config, &files, &warning)
                .expect("failed to emit unknown rule warning");
        }
    }

    let results = analyze(file, exceptions, lint, shellcheck).await?;

    let cwd = std::env::current_dir().ok();
    let mut error_count = 0;
    let mut warning_count = 0;
    let mut note_count = 0;
    for result in &results {
        let mut suppress = false;

        // Attempt to strip the CWD from the result path
        let uri = result.document().uri();
        if (args.common.single_document) && !uri.as_str().contains(file) {
            continue;
        }
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
            _ => {
                if !(remote_file || args.common.show_remote_diagnostics) {
                    suppress = true;
                }
                uri.to_string()
            }
        };

        let diagnostics = match result.error() {
            Some(e) => &[Diagnostic::error(format!("failed to read `{uri}`: {e:#}"))],
            None => result.document().diagnostics(),
        };

        if !diagnostics.is_empty() {
            let filtered_diagnostics: Vec<&Diagnostic> = diagnostics
                .iter()
                .filter(|d| {
                    let severity = d.severity();
                    match severity {
                        Severity::Error => true,
                        Severity::Note if (args.common.hide_notes) => false,
                        _ if suppress => false,
                        _ => true,
                    }
                })
                .collect();

            if !filtered_diagnostics.is_empty() {
                emit_diagnostics(
                    filtered_diagnostics.iter().copied(),
                    &uri,
                    &result.document().root().inner().text().to_string(),
                    args.common.report_mode.unwrap_or_default(),
                    args.common.no_color,
                );

                for diagnostic in diagnostics.iter() {
                    // we are not counting unknown exceptions
                    match diagnostic.severity() {
                        Severity::Error => error_count += 1,
                        Severity::Warning if !suppress => warning_count += 1,
                        Severity::Note if !suppress => note_count += 1,
                        _ => {}
                    }
                }
            }
        }
    }

    if error_count > 0 {
        bail!(
            "failing due to {error_count} error{s}",
            s = if error_count == 1 { "" } else { "s" }
        );
    } else if (args.common.deny_warnings) && warning_count > 0 {
        bail!(
            "failing due to {warning_count} warning{s} (`--deny-warnings` was specified)",
            s = if warning_count == 1 { "" } else { "s" }
        );
    } else if (args.common.deny_notes) && note_count > 0 {
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
