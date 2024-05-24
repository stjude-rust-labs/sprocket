//! `wdl-grammar gauntlet`

#![feature(let_chains)]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
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
use crate::repository::Identifier;
use crate::repository::WorkDir;

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
    /// This will create temporary shallow clones of every
    /// test repository specified on the CL. Normally, there is only one
    /// repository on disk at a time. The difference in disk space usage
    /// should be negligible.
    pub repositories: Option<Vec<String>>,

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

    /// Overwrites the configuration file with new expected concerns and the
    /// latest commit hashes. This will create temporary shallow clones of every
    /// test repository. Normally, there is only one repository on disk at a
    /// time. The difference in disk space usage should be negligible.
    #[arg(long)]
    pub refresh: bool,

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

    let mut work_dir = WorkDir::default();

    if args.refresh {
        info!("refreshing repository commit hashes.");
        config.inner_mut().update_repositories(work_dir.root());
    }

    if let Some(repositories) = args.repositories {
        repositories.into_iter().for_each(|value| {
            let identifier = value
                .parse::<Identifier>()
                .map_err(Error::RepositoryIdentifier)
                .unwrap();
            work_dir.add_by_identifier(&identifier);
        });
        config
            .inner_mut()
            .extend_repositories(work_dir.repositories().clone())
    }

    let mut report = Report::from(std::io::stdout().lock());

    for (index, (repository_identifier, repo)) in config.inner().repositories().iter().enumerate() {
        let results = repo.wdl_files(work_dir.root());

        report.title(repository_identifier).map_err(Error::Io)?;
        report.next_section().map_err(Error::Io)?;

        for (relative_path, content) in results {
            let abs_path = work_dir
                .root()
                .join(repository_identifier.organization())
                .join(repository_identifier.name())
                .join(&relative_path);
            if let Some(ref filters) = args.filter {
                let mut skip = true;

                for filter in filters {
                    if content.contains(filter) {
                        skip = false;
                        break;
                    }
                }

                if skip {
                    trace!("skipping: {:?}", abs_path);
                    continue;
                }
            }

            trace!("processing: {:?}", abs_path);

            let document_identifier =
                document::Identifier::new(repository_identifier.clone(), relative_path);

            match config.inner().version() {
                wdl_core::Version::V1 => {
                    let mut detected_concerns = IndexSet::new();

                    let (pt, these_concerns) = grammar::v1::parse(&content)
                        .map_err(Error::GrammarV1)?
                        .into_parts();
                    if let Some(these_concerns) = these_concerns {
                        if let Some(parse_errors) = these_concerns.parse_errors() {
                            detected_concerns.extend(parse_errors.into_iter().map(|error| {
                                // SAFETY: We ensure these concerns are not LintWarnings,
                                // so they will always unwrap.
                                ReportableConcern::from_concern(
                                    document_identifier.to_string(),
                                    wdl_core::Concern::ParseError(error.to_owned()),
                                )
                                .unwrap()
                            }));
                        }
                        if let Some(validation_failures) = these_concerns.validation_failures() {
                            detected_concerns.extend(validation_failures.into_iter().map(
                                |failure| {
                                    // SAFETY: We ensure these concerns are not LintWarnings,
                                    // so they will always unwrap.
                                    ReportableConcern::from_concern(
                                        document_identifier.to_string(),
                                        wdl_core::Concern::ValidationFailure(failure.to_owned()),
                                    )
                                    .unwrap()
                                },
                            ));
                        }
                    }

                    if let Some(pt) = pt {
                        let (_, these_concerns) =
                            ast::v1::parse(pt).map_err(Error::AstV1)?.into_parts();
                        if let Some(these_concerns) = these_concerns {
                            if let Some(parse_errors) = these_concerns.parse_errors() {
                                detected_concerns.extend(parse_errors.into_iter().map(|error| {
                                    // SAFETY: We ensure these concerns are not LintWarnings,
                                    // so they will always unwrap.
                                    ReportableConcern::from_concern(
                                        document_identifier.to_string(),
                                        wdl_core::Concern::ParseError(error.to_owned()),
                                    )
                                    .unwrap()
                                }));
                            }
                            if let Some(validation_failures) = these_concerns.validation_failures()
                            {
                                detected_concerns.extend(validation_failures.into_iter().map(
                                    |failure| {
                                        // SAFETY: We ensure these concerns are not LintWarnings,
                                        // so they will always unwrap.
                                        ReportableConcern::from_concern(
                                            document_identifier.to_string(),
                                            wdl_core::Concern::ValidationFailure(
                                                failure.to_owned(),
                                            ),
                                        )
                                        .unwrap()
                                    },
                                ));
                            }
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

    if args.refresh {
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
             configuration file or run the command with the `--refresh` option!"
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
