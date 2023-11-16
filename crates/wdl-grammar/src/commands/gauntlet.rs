//! `wdl-grammar gauntlet`

use std::collections::HashSet;
use std::path::PathBuf;
use std::process;

use clap::Parser;
use colored::Colorize as _;
use log::debug;
use log::trace;

pub mod config;
pub mod document;
mod report;
pub mod repository;

pub use config::Config;
pub use report::Report;
pub use repository::Repository;

use wdl_grammar as grammar;

use crate::commands::gauntlet::report::Status;
use crate::commands::gauntlet::repository::options;
use crate::commands::gauntlet::repository::Identifier;

/// The exit code to emit when any test unexpectedly fails.
const EXIT_CODE_FAILED: i32 = 1;

/// The exit code to emit when an error was expected but not encountered.
const EXIT_CODE_UNDETECTED_IGNORED_ERRORS: i32 = 2;

/// An error related to the `wdl-grammar gauntlet` subcommand.
#[derive(Debug)]
pub enum Error {
    /// A configuration file error.
    Config(config::Error),

    /// An input/output error.
    InputOutput(std::io::Error),

    /// An error related to a [`Repository`].
    Repository(repository::Error),

    /// An error related to a repository [`Builder`](repository::Builder).
    RepositoryBuilder(repository::builder::Error),

    /// An error related to a repository [`Identifier`].
    RepositoryIdentifier(repository::identifier::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Config(err) => write!(f, "configuration file error: {err}"),
            Error::InputOutput(err) => write!(f, "i/o error: {err}"),
            Error::Repository(err) => write!(f, "repository error: {err}"),
            Error::RepositoryBuilder(err) => write!(f, "repository builder error: {err}"),
            Error::RepositoryIdentifier(err) => write!(f, "repository identifier error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
type Result<T> = std::result::Result<T, Error>;

/// Arguments for the `wdl-grammar gauntlet` subcommand.
#[derive(Debug, Parser)]
pub struct Args {
    /// The GitHub repositories to evaluate (e.g., "stjudecloud/workflows").
    repositories: Option<Vec<String>>,

    /// The location of the cache directory.
    #[arg(long)]
    cache_dir: Option<PathBuf>,

    /// The location of the config file.
    #[arg(short, long)]
    config_file: Option<PathBuf>,

    /// Don't load any configuration from the cache.
    #[arg(short, long, global = true)]
    no_cache: bool,

    /// Only errors are printed to the stderr stream.
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Overwrites the configuration file.
    #[arg(long, global = true)]
    save_config: bool,

    /// Silences printing detailed error information.
    #[arg(long, global = true)]
    silence_error_details: bool,

    /// Skips the retreiving of remote objects.
    #[arg(long, global = true)]
    skip_remote: bool,

    /// Displays warnings as part of the report output.
    #[arg(long, global = true)]
    show_warnings: bool,

    /// The Workflow Description Language (WDL) specification version to use.
    #[arg(value_name = "VERSION", short = 's', long, default_value_t, value_enum)]
    specification_version: grammar::Version,

    /// All available information, including debug information, is logged.
    #[arg(short, long, global = true)]
    verbose: bool,
}

/// Main function for this subcommand.
pub async fn gauntlet(args: Args) -> Result<()> {
    let mut config = match args.no_cache {
        true => {
            debug!("Skipping loading from cache.");
            Config::default()
        }
        false => {
            let path = args.config_file.unwrap_or(Config::default_path());
            Config::load_or_new(path, args.specification_version).map_err(Error::Config)?
        }
    };

    if let Some(repositories) = args.repositories {
        config.repositories_mut().extend(
            repositories
                .into_iter()
                .map(|value| {
                    value
                        .parse::<Identifier>()
                        .map_err(Error::RepositoryIdentifier)
                })
                .collect::<Result<Vec<_>>>()?,
        );
    }

    let mut report = Report::new(std::io::stdout().lock());

    for (index, repository_identifier) in config.repositories().iter().enumerate() {
        let mut repository =
            repository::Builder::default().identifier(repository_identifier.clone());

        if let Some(ref root) = args.cache_dir {
            let mut repository_cache_root = root.clone();
            repository_cache_root.push(repository_identifier.organization());
            repository_cache_root.push(repository_identifier.name());
            repository = repository.root(repository_cache_root);
        }

        if args.skip_remote {
            let options = options::Builder::default().hydrate_remote(false).build();
            repository = repository.options(options)
        }

        let mut repository = repository.try_build().map_err(Error::RepositoryBuilder)?;
        let results = repository.hydrate().await.map_err(Error::Repository)?;

        report
            .title(repository_identifier)
            .map_err(Error::InputOutput)?;
        report.next_section().map_err(Error::InputOutput)?;

        for (path, content) in results {
            let document_identifier =
                document::Identifier::new(repository_identifier.clone(), path);

            match config.version() {
                grammar::Version::V1 => {
                    match grammar::v1::parse(grammar::v1::Rule::document, &content) {
                        Ok(tree) => match tree.warnings() {
                            Some(warnings) => {
                                trace!(
                                    "{}: successfully parsed with {} warnings.",
                                    document_identifier,
                                    warnings.len()
                                );
                                report
                                    .register(document_identifier, Status::Warning)
                                    .map_err(Error::InputOutput)?;

                                if args.show_warnings {
                                    for warning in warnings {
                                        report
                                            .report_warning(warning)
                                            .map_err(Error::InputOutput)?;
                                    }
                                }
                            }
                            None => {
                                trace!("{}: succesfully parsed.", document_identifier,);
                                report
                                    .register(document_identifier, Status::Success)
                                    .map_err(Error::InputOutput)?;
                            }
                        },
                        Err(err) => {
                            let actual_error = err.to_string();

                            if let Some(expected_error) =
                                config.ignored_errors().get(&document_identifier)
                            {
                                if expected_error == &actual_error {
                                    trace!(
                                        "{}: removing from expected errors.",
                                        document_identifier
                                    );
                                    report
                                        .register(
                                            document_identifier,
                                            Status::Ignored(actual_error),
                                        )
                                        .map_err(Error::InputOutput)?;
                                } else {
                                    trace!("{}: mismatched error message.", document_identifier);
                                    report
                                        .register(
                                            document_identifier,
                                            Status::Mismatch(actual_error),
                                        )
                                        .map_err(Error::InputOutput)?;
                                }
                            } else {
                                trace!("{}: not present in expected errors.", document_identifier);
                                report
                                    .register(document_identifier, Status::Error(actual_error))
                                    .map_err(Error::InputOutput)?;
                            }
                        }
                    }
                }
            }
        }

        report.next_section().map_err(Error::InputOutput)?;

        if !args.silence_error_details {
            report
                .report_unexpected_errors_for_repository(repository_identifier)
                .map_err(Error::InputOutput)?;
            report.next_section().map_err(Error::InputOutput)?;
        }

        report
            .footer(repository_identifier)
            .map_err(Error::InputOutput)?;
        report.next_section().map_err(Error::InputOutput)?;

        if index != config.repositories().len() - 1 {
            println!();
        }
    }

    let detected_errors = report
        .results()
        .clone()
        .into_iter()
        .filter(|(_, status)| !status.success())
        .map(|(id, status)| {
            (
                id,
                match status {
                    Status::Success => unreachable!(),
                    Status::Warning => unreachable!(),
                    Status::Mismatch(msg) => msg,
                    Status::Error(msg) => msg,
                    Status::Ignored(msg) => msg,
                },
            )
        })
        .collect::<HashSet<_>>();

    let ignored_errors = config
        .ignored_errors()
        .clone()
        .into_iter()
        .collect::<HashSet<_>>();

    let unignored_errors = &detected_errors - &ignored_errors;
    let undetected_ignored_errors = &ignored_errors - &detected_errors;

    if args.save_config {
        if !undetected_ignored_errors.is_empty() {
            debug!(
                "removing {} undetected but expected errors.",
                undetected_ignored_errors.len()
            );

            *config.ignored_errors_mut() = (&ignored_errors - &undetected_ignored_errors)
                .union(&unignored_errors)
                .cloned()
                .collect();
        }

        config.ignored_errors_mut().extend(
            unignored_errors
                .into_iter()
                .map(|(id, message)| (id.clone(), message.clone()))
                .collect::<Vec<_>>(),
        );
        config.save().map_err(Error::Config)?;
    } else if !undetected_ignored_errors.is_empty() {
        println!(
            "\n{}\n",
            "Undetected expected errors: you should remove these from your \
            Config.toml or run this command with the `--save-config` option!"
                .red()
                .bold()
        );

        for (document_identifier, error) in undetected_ignored_errors {
            println!(
                "{}\n\n{}\n",
                document_identifier.to_string().italic(),
                error
            );
        }

        process::exit(EXIT_CODE_UNDETECTED_IGNORED_ERRORS);
    } else if !unignored_errors.is_empty() {
        process::exit(EXIT_CODE_FAILED);
    }

    Ok(())
}
