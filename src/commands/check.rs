//! Implementation of the check and lint commands.

use std::fs;

use anyhow::Context;
use anyhow::bail;
use clap::Parser;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::emit;
use url::Url;
use wdl::ast::Diagnostic;
use wdl::ast::Severity;
use wdl::ast::SyntaxNode;
use wdl::cli::analyze;

use crate::Mode;
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

    /// Only display diagnostics for local files.
    ///
    /// This is useful when you want to ignore diagnostics for remote imports.
    /// If specified with a remote file, an error will be raised.
    #[arg(long)]
    pub local_only: bool,

    /// Supress diagnostics from imported documents.
    ///
    /// This will only display diagnostics for the document specified by `file`.
    /// If specified with a directory, an error will be raised.
    #[arg(long)]
    pub single_document: bool,

    /// Run the `shellcheck` program on command sections.
    ///
    /// Requires linting to be enabled. This feature is experimental.
    /// False positives may be reported.
    /// If `shellcheck` is not installed, an error will be raised.
    #[arg(long)]
    pub shellcheck: bool,

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
    if args.common.shellcheck && !args.lint {
        bail!("`--shellcheck` requires `--lint` to be enabled");
    }

    let (config, mut stream) = get_display_config(args.common.report_mode, args.common.no_color);
    let exceptions = args.common.except;
    let lint = args.lint;
    let shellcheck = args.common.shellcheck;

    let file = args.common.file;
    let results = if Url::parse(&file).is_ok() {
        if args.common.local_only {
            bail!(
                "`--local-only` was specified, but `{file}` is a remote URL",
                file = file
            );
        }
        analyze(&file, exceptions, lint, shellcheck)
    } else if fs::metadata(&file)
        .with_context(|| format!("failed to read metadata for file `{file}`"))?
        .is_dir()
    {
        if args.common.single_document {
            bail!(
                "`--single-document` was specified, but `{file}` is a directory",
                file = file
            );
        }
        analyze(&file, exceptions, lint, shellcheck)
    } else {
        analyze(&file, exceptions, lint, shellcheck)
    }
    .await?;

    let cwd = std::env::current_dir().ok();
    let mut error_count = 0;
    let mut warning_count = 0;
    let mut note_count = 0;
    for result in &results {
        // Attempt to strip the CWD from the result path
        let uri = result.document().uri();
        if args.common.single_document && !uri.as_str().contains(&file) {
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
                if args.common.local_only {
                    continue;
                }
                uri.to_string()
            }
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
