//! Implementation of the `check` and `lint` subcommands.

use std::collections::HashSet;

use anyhow::Context;
use anyhow::bail;
use clap::Parser;
use clap::builder::PossibleValuesParser;
use codespan_reporting::diagnostic::Diagnostic;
use codespan_reporting::files::SimpleFiles;
use tracing::info;
use wdl::ast::AstNode;
use wdl::ast::Severity;
use wdl::cli::Analysis;
use wdl::cli::analysis::Source;
use wdl::lint::find_nearest_rule;

use super::explain::ALL_RULE_IDS;
use crate::Mode;
use crate::emit_diagnostics;
use crate::get_display_config;

/// Common arguments for the `check` and `lint` subcommands.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Common {
    /// A set of source documents as files, directories, or URLs.
    #[clap(value_name = "PATH or URL")]
    pub sources: Vec<Source>,

    /// Excepts (ignores) an analysis or lint rule.
    ///
    /// Repeat the flag multiple times to except multiple rules.
    #[clap(short, long, value_name = "RULE",
        value_parser = PossibleValuesParser::new(ALL_RULE_IDS.iter()),
        action = clap::ArgAction::Append,
        num_args = 1,
    )]
    pub except: Vec<String>,

    /// Causes the command to fail if warnings were reported.
    #[clap(long)]
    pub deny_warnings: bool,

    /// Causes the command to fail if notes were reported.
    #[clap(long)]
    pub deny_notes: bool,

    /// Suppress diagnostics from documents that were not explicitly provided in
    /// the sources list (i.e., were imported from a provided source).
    ///
    /// If the sources list contains a directory, an error will be raised.
    #[arg(long)]
    pub suppress_imports: bool,

    /// Show diagnostics for remote documents.
    ///
    /// By default, when checking a local document remote diagnostics are
    /// suppressed. This flag will show diagnostics for remote documents.
    /// This flag has no effect when checking a remote document.
    #[arg(long)]
    pub show_remote_diagnostics: bool,

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

    /// Enable lint checks in addition to validation errors.
    #[arg(short, long)]
    pub lint: bool,
}

impl CheckArgs {
    /// Applies the configuration from the given config file to the command line
    /// arguments.
    pub fn apply(mut self, config: crate::config::Config) -> Self {
        self.common.except = self
            .common
            .except
            .clone()
            .into_iter()
            .chain(config.check.except.clone())
            .collect();
        self.common.deny_warnings = self.common.deny_warnings || config.check.deny_warnings;
        self.common.deny_notes = self.common.deny_notes || config.check.deny_notes;
        self.common.hide_notes = self.common.hide_notes || config.check.hide_notes;
        self.common.no_color = self.common.no_color || !config.common.color;
        self.common.report_mode = match self.common.report_mode {
            Some(mode) => Some(mode),
            None => Some(config.common.report_mode),
        };

        self
    }
}

/// Arguments for the `lint` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct LintArgs {
    /// The command command line arguments.
    #[command(flatten)]
    pub common: Common,
}

impl LintArgs {
    /// Applies the configuration from the given config file to the command line
    /// arguments.
    pub fn apply(mut self, config: crate::config::Config) -> Self {
        self.common.except = self
            .common
            .except
            .clone()
            .into_iter()
            .chain(config.check.except.clone())
            .collect();
        self.common.deny_warnings = self.common.deny_warnings || config.check.deny_warnings;
        self.common.deny_notes = self.common.deny_notes || config.check.deny_notes;
        self.common.hide_notes = self.common.hide_notes || config.check.hide_notes;
        self.common.no_color = self.common.no_color || !config.common.color;
        self.common.report_mode = match self.common.report_mode {
            Some(mode) => Some(mode),
            None => Some(config.common.report_mode),
        };

        self
    }
}

/// Performs the `check` subcommand.
pub async fn check(args: CheckArgs) -> anyhow::Result<()> {
    if args.common.sources.is_empty() {
        bail!("you must provide at least one source file, directory, or URL");
    }

    if args.common.suppress_imports {
        for source in args.common.sources.iter() {
            if let Source::Directory(dir) = source {
                bail!(
                    "`--suppress-imports` was specified but the provided inputs contain a \
                     directory: `{dir}`",
                    dir = dir.display()
                );
            }
        }
    }

    let show_remote_diagnostics = {
        let any_remote_sources = args
            .common
            .sources
            .iter()
            .any(|source| matches!(source, Source::Remote(_)));

        if any_remote_sources {
            info!("remote source detected, showing all remote diagnostics");
        }

        any_remote_sources || args.common.show_remote_diagnostics
    };

    report_unknown_rules(
        &args.common.except,
        args.common.report_mode.unwrap_or_default(),
        args.common.no_color,
    )?;

    let provided_source_uris = args
        .common
        .sources
        .iter()
        .flat_map(|s| s.as_url())
        .cloned()
        .collect::<HashSet<_>>();

    let results = match Analysis::default()
        .extend_sources(args.common.sources)
        .extend_exceptions(args.common.except)
        .lint(args.lint)
        .run()
        .await
    {
        Ok(results) => results,
        Err(errors) => {
            // SAFETY: this is a non-empty, so it must always have a first
            // element.
            bail!(errors.into_iter().next().unwrap())
        }
    };

    #[derive(Default)]
    struct Counts {
        /// The number of errors encountered.
        pub errors: usize,
        /// The number of warnings encountered.
        pub warnings: usize,
        /// The number of notes encountered.
        pub notes: usize,
    }

    let mut counts = Counts::default();

    for result in results {
        let uri = &result.document().uri();

        match uri.scheme() {
            "file" => {}
            "http" | "https" => {
                if !show_remote_diagnostics {
                    continue;
                }
            }
            v => todo!("unhandled uri scheme: {v}"),
        };

        let diagnostics = result.document().diagnostics();

        if !diagnostics.is_empty() {
            let path = result.document().path().to_string();
            let source = result.document().root().text().to_string();

            emit_diagnostics(
                &path,
                source,
                diagnostics.iter().filter(|d| {
                    let severity = d.severity();

                    match severity {
                        Severity::Error => {
                            counts.errors += 1;
                            true
                        }
                        Severity::Warning => {
                            if args.common.suppress_imports && !provided_source_uris.contains(uri) {
                                return false;
                            }

                            counts.warnings += 1;
                            true
                        }
                        Severity::Note => {
                            if args.common.suppress_imports && !provided_source_uris.contains(uri) {
                                return false;
                            }

                            if !args.common.hide_notes {
                                counts.notes += 1;
                                true
                            } else {
                                false
                            }
                        }
                    }
                }),
                &[],
                args.common.report_mode.unwrap_or_default(),
                args.common.no_color,
            )
            .context("failed to emit diagnostics")?;
        }
    }

    if counts.errors > 0 {
        bail!(
            "failing due to {errors} error{s}",
            errors = counts.errors,
            s = if counts.errors == 1 { "" } else { "s" }
        );
    } else if args.common.deny_warnings && counts.warnings > 0 {
        bail!(
            "failing due to {warnings} warning{s} (`--deny-warnings` was specified)",
            warnings = counts.warnings,
            s = if counts.warnings == 1 { "" } else { "s" }
        );
    } else if args.common.deny_notes && counts.notes > 0 {
        bail!(
            "failing due to {notes} note{s} (`--deny-notes` was specified)",
            notes = counts.notes,
            s = if counts.notes == 1 { "" } else { "s" }
        );
    }

    Ok(())
}

/// Performs the `lint` subcommand.
pub async fn lint(args: LintArgs) -> anyhow::Result<()> {
    check(CheckArgs {
        common: args.common,
        lint: true,
    })
    .await
}

/// Reports any unknown rules as diagnostics.
fn report_unknown_rules(
    excepted: &[String],
    report_mode: Mode,
    no_color: bool,
) -> anyhow::Result<()> {
    let mut rules = wdl::analysis::rules()
        .into_iter()
        .map(|rule| rule.id().to_owned())
        .collect::<Vec<_>>();
    rules.extend(
        wdl::lint::rules()
            .into_iter()
            .map(|rule| rule.id().to_owned()),
    );

    let mut unknown_rules = excepted
        .iter()
        .filter(|exception| {
            !rules
                .iter()
                .any(|rule| rule.eq_ignore_ascii_case(exception))
        })
        .map(|rule| (rule, find_nearest_rule(rule)))
        .collect::<Vec<_>>();

    if !unknown_rules.is_empty() {
        unknown_rules.sort();

        let (config, writer) = get_display_config(report_mode, no_color);
        let mut writer = writer.lock();
        let files = SimpleFiles::<String, String>::new();

        for (unknown_rule, nearest_rule) in unknown_rules {
            let mut notes = Vec::new();

            if let Some(nearest_rule) = nearest_rule {
                notes.push(format!("fix: did you mean the `{nearest_rule}` rule?"));
            }

            notes.push(String::from(
                "run `sprocket explain --help` to see available rules",
            ));

            let warning = Diagnostic::warning()
                .with_message(format!(
                    "ignoring unknown rule provided via --except: {unknown_rule}",
                ))
                .with_notes(notes);

            codespan_reporting::term::emit(&mut writer, config, &files, &warning)
                .expect("failed to emit unknown rule warning");
        }
    }

    Ok(())
}
