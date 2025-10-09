//! Gauntlet

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(clippy::missing_docs_in_private_items)]
#![warn(rustdoc::broken_intra_doc_links)]

use std::io::IsTerminal;
use std::path::Path;
use std::path::PathBuf;
use std::process;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;
use clap::Parser;
use codespan_reporting::files::Files;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term;
use codespan_reporting::term::DisplayStyle;
use codespan_reporting::term::termcolor::Buffer;
use codespan_reporting::term::termcolor::ColorChoice;
use codespan_reporting::term::termcolor::StandardStream;
use colored::Colorize;
use indexmap::IndexSet;
use tracing::debug;
use tracing::info;

pub mod config;
pub mod document;
pub mod report;
pub mod repository;

pub use config::Config;
pub use report::Report;
use report::Status;
use report::UnmatchedStatus;
pub use repository::Repository;
use wdl::analysis::Analyzer;
use wdl::analysis::Config as AnalysisConfig;
use wdl::analysis::DiagnosticsConfig;
use wdl::analysis::Validator;
use wdl::analysis::rules;
use wdl::ast::AstNode;
use wdl::ast::Diagnostic;
use wdl::lint::Linter;

use crate::repository::WorkDir;

/// The exit code to emit when any test unexpectedly fails.
const EXIT_CODE_UNEXPECTED: i32 = 1;

/// The exit code to emit when an error was expected but not encountered.
const EXIT_CODE_MISSING: i32 = 2;

/// A command-line utility for testing the compatibility of `wdl-analysis`
/// against a wide variety of community WDL repositories.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about)]
pub struct Args {
    /// The GitHub repositories to evaluate (e.g., "stjudecloud/workflows").
    /// This will create temporary shallow clones of every
    /// test repository specified on the CL. Normally, there is only one
    /// repository on disk at a time. The difference in disk space usage
    /// should be negligible.
    pub repositories: Vec<String>,

    /// Enable "arena mode", which switches the reported diagnostics from
    /// syntax errors to opinionated lint warnings.
    #[arg(short, long)]
    pub arena: bool,

    /// The location of the config file.
    #[arg(short, long)]
    pub config_file: Option<PathBuf>,

    /// Detailed information, including debug information, is logged in the
    /// console.
    #[arg(short, long)]
    pub debug: bool,

    /// If provided, only shows tests whose identifier contains the provided
    /// string(s).
    #[arg(short, long = "filter")]
    pub filters: Vec<String>,

    /// Enables logging for all modules (not just `gauntlet`).
    #[arg(short, long)]
    pub log_all_modules: bool,

    /// Don't load any configuration files.
    #[arg(short, long)]
    pub no_config: bool,

    /// Don't show any individual errors.
    #[arg(long)]
    pub no_errors: bool,

    /// Only errors are logged to the console.
    #[arg(short, long)]
    pub quiet: bool,

    /// Overwrites the configuration file with new expected diagnostics.
    ///
    /// Mutually exclusive with `--update`.
    #[arg(long, group = "action")]
    pub bless: bool,

    /// Updates the commit hashes for all repositories.
    ///
    /// Mutually exclusive with `--bless`.
    #[arg(long, group = "action")]
    pub update: bool,

    #[arg(long)]
    /// Whether to keep temporary directories containing the cloned repositories
    /// after execution.
    pub keep: bool,

    /// Displays warnings as part of the report output.
    #[arg(long)]
    pub show_warnings: bool,

    /// All available information, including trace information, is logged in
    /// the console.
    #[arg(short, long)]
    pub trace: bool,

    /// Additional information is logged in the console.
    #[arg(short, long)]
    pub verbose: bool,
}

/// Normalizes diagnostic message text to handle platform-specific differences.
/// This ensures the same diagnostic message on different platforms is treated
/// as identical.
pub fn normalize_diagnostic(s: &str) -> String {
    let s = s.replace('\\', "/").replace("\r\n", "\n");

    // Handle OS-specific error messages
    let s = s.replace(
        "The system cannot find the file specified. (os error 2)",
        "No such file or directory (os error 2)",
    );

    // Also handle Windows "path not found" error
    s.replace(
        "The system cannot find the path specified. (os error 3)",
        "No such file or directory (os error 2)",
    )
}

/// Main function for this subcommand.
pub async fn gauntlet(args: Args) -> Result<()> {
    let mut config = match args.no_config {
        true => {
            debug!("Skipping loading from config.");
            Config::default()
        }
        false => {
            let path = args.config_file.unwrap_or(Config::default_path(args.arena));
            Config::load_or_new(path)?
        }
    };

    let mut work_dir = WorkDir::new(args.keep);

    if args.update {
        info!("updating repository commit hashes.");
        config.inner_mut().update_repositories(work_dir.root());
    }

    for repo in args.repositories.into_iter() {
        let identifier = repo
            .parse()
            .with_context(|| format!("repository identifier `{repo}` is not valid"))?;
        work_dir.add_by_identifier(&identifier);
    }

    config
        .inner_mut()
        .extend_repositories(work_dir.repositories().clone());

    let mut report = Report::from(StandardStream::stdout(if std::io::stdout().is_terminal() {
        ColorChoice::Auto
    } else {
        ColorChoice::Never
    }));
    let mut total_time = Duration::ZERO;
    for (index, (repository_identifier, repo)) in config.inner().repositories().iter().enumerate() {
        let repo_root = repo.checkout(work_dir.root());
        report.title(repository_identifier).with_context(|| {
            format!("failed to write report title for repository `{repository_identifier}`")
        })?;
        report
            .next_section()
            .context("failed to write next section")?;

        let analyzer = Analyzer::new_with_validator(
            // Don't bother duplicating analysis warnings for arena mode
            AnalysisConfig::default().with_diagnostics_config(DiagnosticsConfig::new(
                if args.arena { Vec::new() } else { rules() },
            )),
            move |_: (), _, _, _| async move {},
            move || {
                let mut validator = if args.arena {
                    Validator::empty()
                } else {
                    Validator::default()
                };
                if args.arena {
                    validator.add_visitor(Linter::default());
                }

                validator
            },
        );

        let before = Instant::now();
        analyzer.add_directory(repo_root.clone()).await?;
        let results = analyzer.analyze(()).await?;
        let elapsed = before.elapsed();
        total_time += elapsed;

        for result in &results {
            let path = result.document().uri().to_file_path().ok();
            let path = match &path {
                Some(path) => path
                    .strip_prefix(&repo_root)
                    .unwrap_or(path)
                    .to_string_lossy(),
                // We're only concerned with local files from the repo for Gauntlet
                None => continue,
            };

            let document_identifier =
                document::Identifier::new(repository_identifier.clone(), &path);

            let diagnostics = match result.error() {
                Some(e) => {
                    vec![Diagnostic::error(format!("failed to read `{path}`: {e:#}"))]
                }
                None => result.document().diagnostics().cloned().collect(),
            };

            let mut actual = IndexSet::new();
            if !diagnostics.is_empty() {
                let source = result.document().root().text().to_string();

                let file: SimpleFile<_, _> = SimpleFile::new(
                    Path::new(document_identifier.path())
                        .file_name()
                        .expect("should have file name")
                        .to_str()
                        .expect("path should be UTF-8"),
                    source,
                );
                let config = codespan_reporting::term::Config {
                    display_style: DisplayStyle::Short,
                    ..Default::default()
                };

                for diagnostic in diagnostics.iter() {
                    if args.arena && diagnostic.severity() == wdl::ast::Severity::Error {
                        continue;
                    }
                    let mut buffer = Buffer::no_color();
                    term::emit(&mut buffer, &config, &file, &diagnostic.to_codespan(()))
                        .context("failed to write diagnostic")?;

                    let byte_start = diagnostic
                        .labels()
                        .next()
                        .map(|l| l.span().start())
                        .unwrap_or_default();
                    // The `+1` here is because line_index() is 0-based.
                    let line_no = file.line_index((), byte_start).unwrap_or_default() + 1;
                    let message = std::str::from_utf8(buffer.as_slice())
                        .context("diagnostic should be UTF-8")?
                        .trim()
                        .to_string();

                    // Normalize the diagnostic message to handle platform-specific differences
                    let message = normalize_diagnostic(&message);

                    if !actual.insert((message.clone(), line_no)) {
                        panic!(
                            "duplicate diagnostic: `{message}` at {path}:{line_no}",
                            path = document_identifier.path()
                        );
                    }
                }
            }

            // As the list of diagnostics has been sorted by document identifier, do
            // a binary search and collect the matching messages
            let diagnostics = config.inner().diagnostics();
            let expected: IndexSet<String> = diagnostics
                .binary_search_by_key(&document_identifier, |d| d.document().clone())
                .map(|mut start_index| {
                    // As binary search may return any matching index, back up until we find the
                    // start of the range
                    for i in (0..start_index).rev() {
                        if diagnostics[i].document() != &document_identifier {
                            break;
                        }

                        start_index -= 1;
                    }

                    diagnostics[start_index..]
                        .iter()
                        .map_while(|d| {
                            if d.document() == &document_identifier {
                                // Normalize the diagnostic message from config to handle
                                // platform-specific differences
                                Some(normalize_diagnostic(d.message()))
                            } else {
                                None
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();

            let actual_messages: IndexSet<_> = actual.iter().map(|(m, _)| m.clone()).collect();
            let unexpected = &actual_messages - &expected;
            let missing = &expected - &actual_messages;

            let status = if !unexpected.is_empty() || !missing.is_empty() {
                Status::DiagnosticsUnmatched(
                    UnmatchedStatus {
                        missing,
                        unexpected,
                        all: actual,
                    }
                    .into(),
                )
            } else if !actual.is_empty() {
                Status::DiagnosticsMatched(actual)
            } else {
                Status::Success
            };

            report
                .register(document_identifier, status)
                .context("failed to register report status")?;
        }

        report
            .next_section()
            .context("failed to transition to next report section")?;

        if !args.no_errors {
            report
                .report_unexpected_errors(repository_identifier)
                .context("failed to report unexpected errors")?;
        }

        report
            .next_section()
            .context("failed to transition to next report section")?;
        report
            .footer(repository_identifier, elapsed)
            .context("failed to write report footer")?;
        report
            .next_section()
            .context("failed to transition to next report section")?;

        if index != config.inner().repositories().len() - 1 {
            println!();
        }
    }

    let mut missing = 0;
    let mut unexpected = 0;
    let mut diagnostics = Vec::new();
    for (identifier, status) in report.into_results() {
        let messages = match status {
            Status::Success => continue,
            Status::DiagnosticsMatched(all) => all,
            Status::DiagnosticsUnmatched(unmatched) => {
                missing += unmatched.missing.len();
                unexpected += unmatched.unexpected.len();
                unmatched.all
            }
        };

        // Don't bother rebuilding the diagnostics
        if !args.bless && !args.update {
            continue;
        }

        let hash = config
            .inner()
            .repositories()
            .get(identifier.repository())
            .unwrap()
            .commit_hash()
            .as_ref()
            .unwrap();
        for (message, line_no) in messages {
            diagnostics.push(config::inner::Diagnostic::new(
                identifier.clone(),
                message,
                hash,
                Some(line_no),
            ));
        }
    }

    println!("\nTotal analysis time: {total_time:?}");

    if args.bless || args.update {
        info!("adding {unexpected} new expected diagnostics.");
        info!("removing {missing} outdated expected diagnostics.");

        config.inner_mut().set_diagnostics(diagnostics);
        config.inner_mut().sort();
        config.save().context("failed to save configuration file")?;
    } else if missing > 0 {
        println!(
            "\n{}\n",
            "missing but expected diagnostics remain: you should remove these from your \
             configuration file or run the command with the `--bless` option!"
                .red()
                .bold()
        );

        process::exit(EXIT_CODE_MISSING);
    } else if unexpected > 0 {
        process::exit(EXIT_CODE_UNEXPECTED);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_diagnostic() {
        // Test Windows-style path normalization
        let windows_path = "Failed to import 'C:\\Users\\test\\file.wdl'";
        let unix_path = "Failed to import 'C:/Users/test/file.wdl'";
        assert_eq!(normalize_diagnostic(windows_path), unix_path);

        // Test line ending normalization
        let windows_line_ending = "Error message\r\nSecond line";
        let unix_line_ending = "Error message\nSecond line";
        assert_eq!(normalize_diagnostic(windows_line_ending), unix_line_ending);

        // Test OS-specific error message normalization
        let windows_file_error = "The system cannot find the file specified. (os error 2)";
        let windows_path_error = "The system cannot find the path specified. (os error 3)";
        let unix_error = "No such file or directory (os error 2)";
        assert_eq!(normalize_diagnostic(windows_file_error), unix_error);
        assert_eq!(normalize_diagnostic(windows_path_error), unix_error);

        // Test that Unix style messages are left unchanged
        assert_eq!(normalize_diagnostic(unix_path), unix_path);
        assert_eq!(normalize_diagnostic(unix_line_ending), unix_line_ending);
        assert_eq!(normalize_diagnostic(unix_error), unix_error);
    }
}
