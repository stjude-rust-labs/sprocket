//! Implementation of the `check` and `lint` subcommands.

use std::collections::HashSet;
use std::str::FromStr;

use anyhow::Context;
use anyhow::anyhow;
use clap::Parser;
use clap::builder::PossibleValuesParser;
use codespan_reporting::diagnostic::Diagnostic;
use codespan_reporting::files::SimpleFiles;
use strum::VariantArray;
use tracing::info;
use wdl::ast::AstNode;
use wdl::ast::Severity;
use wdl::lint::ALL_TAG_NAMES;
use wdl::lint::Tag;
use wdl::lint::TagSet;
use wdl::lint::find_nearest_rule;

use super::explain::ALL_RULE_IDS;
use crate::Config;
use crate::analysis::Analysis;
use crate::analysis::Source;
use crate::commands::CommandError;
use crate::commands::CommandResult;
use crate::diagnostics::DiagnosticCounts;
use crate::diagnostics::Mode;
use crate::diagnostics::emit_diagnostics;
use crate::diagnostics::get_diagnostics_display_config;

/// The [`Tag`]s which will run with the default `lint` configuration.
const DEFAULT_TAG_SET: TagSet = TagSet::new(&[
    Tag::Completeness,
    Tag::Naming,
    Tag::Clarity,
    Tag::Portability,
    Tag::Correctness,
    Tag::Deprecated,
    Tag::Documentation,
]);

/// Common arguments for the `check` and `lint` subcommands.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Common {
    /// A set of source documents as files, directories, or URLs.
    #[clap(value_name = "SOURCE")]
    pub sources: Vec<Source>,

    /// Excepts (ignores) an analysis or lint rule.
    ///
    /// Repeat the flag multiple times to except multiple rules. This is
    /// additive with exceptions found in config files.
    #[clap(short, long, value_name = "RULE",
        value_parser = PossibleValuesParser::new(ALL_RULE_IDS.iter()),
        ignore_case = true,
        action = clap::ArgAction::Append,
        num_args = 1,
        hide_possible_values = true,
    )]
    pub except: Vec<String>,

    /// Enable all lint rules. This includes additional rules outside the
    /// default set.
    ///
    /// `--except <RULE>` and `--filter-lint-tag <TAG>` can be used in
    /// conjunction with this argument.
    #[clap(short, long, conflicts_with_all = ["only_lint_tag"])]
    pub all_lint_rules: bool,

    /// Excludes a lint tag from running if it would have been included
    /// otherwise.
    ///
    /// Repeat the flag multiple times to filter multiple tags. This is additive
    /// with filtered tags found in config files.
    #[clap(long, value_name = "TAG",
        value_parser = PossibleValuesParser::new(ALL_TAG_NAMES.iter()),
        ignore_case = true,
        action = clap::ArgAction::Append,
        num_args = 1,
    )]
    pub filter_lint_tag: Vec<String>,

    /// Includes a lint tag for running.
    ///
    /// Repeat the flag multiple times to include multiple tags. `--except
    /// <RULE>` and `--filter-lint-tag <TAG>` can be used in conjunction with
    /// this argument. This is additive with tags selected via config files.
    #[clap(long, value_name = "TAG",
        value_parser = PossibleValuesParser::new(ALL_TAG_NAMES.iter()),
        ignore_case = true,
        action = clap::ArgAction::Append,
        num_args = 1,
    )]
    pub only_lint_tag: Vec<String>,

    /// Causes the command to fail if warnings were reported.
    #[clap(long)]
    pub deny_warnings: bool,

    /// Causes the command to fail if notes were reported.
    ///
    /// Implies `--deny-warnings`.
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
    /// Applies the configuration to the CLI options.
    fn apply(&mut self, config: &crate::config::Config) {
        self.common
            .except
            .extend(config.check.except.iter().cloned());

        self.common.deny_notes |= config.check.deny_notes;

        self.common.deny_warnings |= config.check.deny_warnings || self.common.deny_notes;

        self.common.hide_notes |= config.check.hide_notes;

        if self.common.report_mode.is_none() {
            self.common.report_mode = Some(config.common.report_mode);
        }

        // Linting is implied by any of these args when they are used on the CL
        if !self.common.filter_lint_tag.is_empty()
            || !self.common.only_lint_tag.is_empty()
            || self.common.all_lint_rules
        {
            self.lint = true;
        }

        self.common.all_lint_rules |= config.check.all_lint_rules;
        self.common
            .filter_lint_tag
            .extend(config.check.filter_lint_tags.iter().cloned());

        if !self.common.all_lint_rules {
            self.common
                .only_lint_tag
                .extend(config.check.only_lint_tags.iter().cloned());
        }
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

/// Performs the `check` subcommand.
pub async fn check(mut args: CheckArgs, config: Config, colorize: bool) -> CommandResult<()> {
    args.apply(&config);

    let mut sources = args.common.sources;
    if sources.is_empty() {
        sources.push(Source::default());
    }

    // Validate provided args
    if args.common.suppress_imports {
        for source in sources.iter() {
            if let Source::Directory(dir) = source {
                return Err(anyhow!(
                    "`--suppress-imports` was specified but the provided inputs contain a \
                     directory: `{dir}`",
                    dir = dir.display()
                )
                .into());
            }
        }
    }

    // Process args
    let show_remote_diagnostics = {
        let any_remote_sources = sources
            .iter()
            .any(|source| matches!(source, Source::File(url) if url.scheme() != "file"));

        if any_remote_sources {
            info!("remote source detected, showing all remote diagnostics");
        }

        any_remote_sources || args.common.show_remote_diagnostics
    };

    report_unknown_rules(
        &args.common.except,
        args.common.report_mode.unwrap_or_default(),
        colorize,
    )?;

    let provided_source_uris = sources
        .iter()
        .flat_map(|s| s.as_url())
        .cloned()
        .collect::<HashSet<_>>();

    let enabled_tags = if args.lint {
        if args.common.all_lint_rules {
            TagSet::new(Tag::VARIANTS)
        } else if !args.common.only_lint_tag.is_empty() {
            TagSet::new(
                args.common
                    .only_lint_tag
                    .iter()
                    .filter_map(|t| Tag::from_str(t).ok())
                    .collect::<Vec<_>>()
                    .as_slice(),
            )
        } else {
            DEFAULT_TAG_SET
        }
    } else {
        TagSet::new(&[])
    };

    let disabled_tags = if args.lint && !args.common.filter_lint_tag.is_empty() {
        TagSet::new(
            args.common
                .filter_lint_tag
                .iter()
                .filter_map(|t| Tag::from_str(t).ok())
                .collect::<Vec<_>>()
                .as_slice(),
        )
    } else {
        TagSet::new(&[])
    };

    // Run analysis
    let results = Analysis::default()
        .extend_sources(sources)
        .extend_exceptions(args.common.except)
        .enabled_lint_tags(enabled_tags)
        .disabled_lint_tags(disabled_tags)
        .run()
        .await
        .map_err(CommandError::from)?;

    let mut counts = DiagnosticCounts::default();

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

        let mut diagnostics = result.document().diagnostics().peekable();

        if diagnostics.peek().is_some() {
            let path = result.document().path().to_string();
            let source = result.document().root().text().to_string();

            emit_diagnostics(
                &path,
                source,
                diagnostics.filter(|d| {
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
                colorize,
            )
            .context("failed to emit diagnostics")?;
        }
    }

    if let Some(e) = counts.verify_no_errors() {
        return Err(e.into());
    } else if args.common.deny_warnings
        && let Some(e) = counts.verify_no_warnings(true)
    {
        return Err(e.into());
    } else if args.common.deny_notes
        && let Some(e) = counts.verify_no_notes(true)
    {
        return Err(e.into());
    }

    Ok(())
}

/// Performs the `lint` subcommand.
pub async fn lint(args: LintArgs, config: Config, colorize: bool) -> CommandResult<()> {
    check(
        CheckArgs {
            common: args.common,
            lint: true,
        },
        config,
        colorize,
    )
    .await
}

/// Reports any unknown rules as diagnostics.
fn report_unknown_rules(
    excepted: &[String],
    report_mode: Mode,
    colorize: bool,
) -> anyhow::Result<()> {
    let rules = ALL_RULE_IDS.clone();

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

        let (config, writer) = get_diagnostics_display_config(report_mode, colorize);
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

            codespan_reporting::term::emit_to_write_style(&mut writer, config, &files, &warning)
                .expect("failed to emit unknown rule warning");
        }
    }

    Ok(())
}
