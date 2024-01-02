//! `wdl-grammar gauntlet`

#![feature(let_chains)]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::missing_docs_in_private_items)]
#![warn(rustdoc::broken_intra_doc_links)]

use std::path::PathBuf;
use std::process;

use clap::Parser;
use colored::Colorize as _;
use indexmap::IndexSet;
use log::debug;
use log::info;
use log::trace;
use wdl_ast as ast;
use wdl_grammar as grammar;

pub mod config;
pub mod document;
pub mod report;
pub mod repository;

pub use config::Config;
pub use report::Report;
pub use repository::Repository;

use crate::config::ReportableConcern;
use crate::report::Status;
use crate::repository::options;
use crate::repository::Identifier;

/// The exit code to emit when any test unexpectedly fails.
const EXIT_CODE_FAILED: i32 = 1;

/// The exit code to emit when an error was expected but not encountered.
const EXIT_CODE_UNDETECTED_IGNORED_CONCERNS: i32 = 2;

/// An error related to the `wdl-grammar gauntlet` subcommand.
#[derive(Debug)]
pub enum Error {
    /// A WDL 1.x abstract syntax tree error.
    AstV1(ast::v1::Error),

    /// A configuration file error.
    Config(config::Error),

    /// An input/output error.
    Io(std::io::Error),

    /// A WDL 1.x parse tree error.
    GrammarV1(grammar::v1::Error),

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
            Error::AstV1(err) => write!(f, "ast error: {err}"),
            Error::Config(err) => write!(f, "configuration file error: {err}"),
            Error::Io(err) => write!(f, "i/o error: {err}"),
            Error::GrammarV1(err) => write!(f, "grammar error: {err}"),
            Error::Repository(err) => write!(f, "repository error: {err}"),
            Error::RepositoryBuilder(err) => {
                write!(f, "repository builder error: {err}")
            }
            Error::RepositoryIdentifier(err) => {
                write!(f, "repository identifier error: {err}")
            }
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
type Result<T> = std::result::Result<T, Error>;

/// A command-line utility for testing the compatibility of `wdl-grammar` and
/// `wdl-ast` against a wide variety of community WDL repositories.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about)]
pub struct Args {
    /// The GitHub repositories to evaluate (e.g., "stjudecloud/workflows").
    pub repositories: Option<Vec<String>>,

    /// The location of the cache directory.
    #[arg(long)]
    pub cache_dir: Option<PathBuf>,

    /// The location of the config file.
    #[arg(short, long)]
    pub config_file: Option<PathBuf>,

    /// Detailed information, including debug information, is logged in the
    /// console.
    #[arg(short, long)]
    pub debug: bool,

    /// If provided, only shows tests whose identifier contains the provided
    /// string(s).
    #[arg(short, long)]
    pub filter: Option<Vec<String>>,

    /// Enables logging for all modules (not just `wdl-grammar`).
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

    /// Overwrites the configuration file.
    #[arg(long)]
    pub save_config: bool,

    /// Skips the retreiving of remote objects.
    #[arg(long)]
    pub skip_remote: bool,

    /// Displays warnings as part of the report output.
    #[arg(long)]
    pub show_warnings: bool,

    /// The Workflow Description Language (WDL) specification version to use.
    #[arg(value_name = "VERSION", short = 's', long, default_value_t, value_enum)]
    pub specification_version: wdl_core::Version,

    /// All available information, including trace information, is logged in
    /// the console.
    #[arg(short, long)]
    pub trace: bool,

    /// Additional information is logged in the console.
    #[arg(short, long)]
    pub verbose: bool,
}

/// Main function for this subcommand.
pub async fn gauntlet(args: Args) -> Result<()> {
    let mut config = match args.no_config {
        true => {
            debug!("Skipping loading from config.");
            Config::default()
        }
        false => {
            let path = args.config_file.unwrap_or(Config::default_path());
            Config::load_or_new(path, args.specification_version).map_err(Error::Config)?
        }
    };

    if let Some(repositories) = args.repositories {
        config.inner_mut().extend_repositories(
            repositories
                .into_iter()
                .map(|value| {
                    value
                        .parse::<Identifier>()
                        .map_err(Error::RepositoryIdentifier)
                })
                .collect::<Result<Vec<_>>>()?,
        )
    }

    let mut report = Report::from(std::io::stdout().lock());

    for (index, repository_identifier) in config.inner().repositories().iter().enumerate() {
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

        report.title(repository_identifier).map_err(Error::Io)?;
        report.next_section().map_err(Error::Io)?;

        for (path, content) in results {
            if let Some(ref filters) = args.filter {
                let mut skip = true;

                for filter in filters {
                    if content.contains(filter) {
                        skip = false;
                        break;
                    }
                }

                if skip {
                    trace!("skipping: {path}");
                    continue;
                }
            }

            trace!("processing: {path}");

            let document_identifier =
                document::Identifier::new(repository_identifier.clone(), path);

            match config.inner().version() {
                wdl_core::Version::V1 => {
                    let mut detected_concerns = IndexSet::new();

                    let (pt, these_concerns) = grammar::v1::parse(&content)
                        .map_err(Error::GrammarV1)?
                        .into_parts();
                    if let Some(these_concerns) = these_concerns {
                        detected_concerns.extend(these_concerns.into_inner().map(|concern| {
                            ReportableConcern::from_concern(
                                document_identifier.to_string(),
                                concern,
                            )
                        }));
                    }

                    if let Some(pt) = pt {
                        let (_, these_concerns) =
                            ast::v1::parse(pt).map_err(Error::AstV1)?.into_parts();
                        if let Some(these_concerns) = these_concerns {
                            detected_concerns.extend(these_concerns.into_inner().map(|concern| {
                                ReportableConcern::from_concern(
                                    document_identifier.to_string(),
                                    concern,
                                )
                            }));
                        }
                    }

                    let expected_concerns = config
                        .inner()
                        .concerns()
                        .iter()
                        .filter_map(|concern| {
                            if concern.document() == document_identifier.to_string() {
                                Some(concern.clone())
                            } else {
                                None
                            }
                        })
                        .collect::<IndexSet<_>>();

                    let unexpected_concerns = &detected_concerns - &expected_concerns;
                    let missing_concerns = &expected_concerns - &detected_concerns;

                    if !unexpected_concerns.is_empty() {
                        report
                            .register(
                                document_identifier,
                                Status::UnexpectedConcerns(
                                    unexpected_concerns.into_iter().collect::<Vec<_>>(),
                                ),
                            )
                            .map_err(Error::Io)?;
                    } else if !missing_concerns.is_empty() {
                        report
                            .register(
                                document_identifier,
                                Status::MissingExpectedConcerns(
                                    missing_concerns.into_iter().collect::<Vec<_>>(),
                                ),
                            )
                            .map_err(Error::Io)?;
                    } else if !detected_concerns.is_empty() {
                        report
                            .register(document_identifier, Status::ConcernsMatched)
                            .map_err(Error::Io)?;
                    } else {
                        report
                            .register(document_identifier, Status::Success)
                            .map_err(Error::Io)?;
                    }
                }
            }
        }

        report.next_section().map_err(Error::Io)?;

        if !args.no_errors {
            report
                .report_unexpected_errors(repository_identifier)
                .map_err(Error::Io)?;
        }

        report.next_section().map_err(Error::Io)?;

        report.footer(repository_identifier).map_err(Error::Io)?;
        report.next_section().map_err(Error::Io)?;

        if index != config.inner().repositories().len() - 1 {
            println!();
        }
    }

    let missing_but_expected = report
        .results()
        .clone()
        .into_iter()
        .filter_map(|(_, status)| match status {
            Status::MissingExpectedConcerns(concerns) => Some(concerns),
            _ => None,
        })
        .flatten()
        .collect::<IndexSet<_>>();

    let unexpected = report
        .results()
        .clone()
        .into_iter()
        .filter_map(|(_, status)| match status {
            Status::UnexpectedConcerns(concerns) => Some(concerns),
            _ => None,
        })
        .flatten()
        .collect::<IndexSet<_>>();

    if args.save_config {
        info!("adding {} new expected concerns.", unexpected.len());
        info!(
            "removing {} outdated expected concerns.",
            missing_but_expected.len()
        );

        let existing = config.inner().concerns().clone();
        let new = existing
            .difference(&missing_but_expected)
            .chain(unexpected.iter())
            .cloned()
            .collect::<IndexSet<_>>();

        config.inner_mut().set_concerns(new);
        config.save().map_err(Error::Config)?;
    } else if !missing_but_expected.is_empty() {
        println!(
            "\n{}\n",
            "undetected but expected concerns remain: you should remove these from your \
             configuration file or run the command with the `--save-config` option!"
                .red()
                .bold()
        );

        for concern in missing_but_expected {
            println!("{}\n\n", concern);
        }

        process::exit(EXIT_CODE_UNDETECTED_IGNORED_CONCERNS);
    } else if !unexpected.is_empty() {
        process::exit(EXIT_CODE_FAILED);
    }

    Ok(())
}
