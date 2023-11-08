//! A command-line tool for parsing and exploring Workflow Description Language
//! (WDL) documents.
//!
//! This tool is intended to be used as a utility to test and develop the
//! [`wdl`](https://crates.io/wdl) crate.

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![deny(rustdoc::broken_intra_doc_links)]

use std::fs;
use std::path::PathBuf;

use clap::Parser;
use clap::Subcommand;
use log::LevelFilter;

use pest::Parser as _;

use wdl_grammar as wdl;

use wdl::Version;

/// An error related to the `wdl` command-line tool.
#[derive(Debug)]
pub enum Error {
    /// An input/output error.
    IoError(std::io::Error),

    /// Attempted to access a file, but it was missing.
    FileDoesNotExist(PathBuf),

    /// Unknown rule name.
    UnknownRule(String),

    /// An error from Pest.
    PestError(Box<pest::error::Error<wdl::v1::Rule>>),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::IoError(err) => write!(f, "i/o error: {err}"),
            Error::FileDoesNotExist(path) => write!(f, "file does not exist: {}", path.display()),
            Error::UnknownRule(rule) => {
                write!(f, "unknown rule: {rule}")
            }
            Error::PestError(err) => write!(f, "pest error:\n{err}"),
        }
    }
}

impl std::error::Error for Error {}

type Result<T> = std::result::Result<T, Error>;

/// Arguments for the `parse` subcommand.
#[derive(Debug, Parser)]
pub struct ParseArgs {
    /// The path to the document.
    path: PathBuf,

    /// The WDL specification version to use.
    #[arg(short = 's', long, default_value_t, value_enum)]
    specification_version: Version,

    /// The rule to evaluate.
    #[arg(short = 'r', long, default_value = "document")]
    rule: String,
}

/// Subcommands for the `wdl` command-line tool.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Parses the Workflow Description Language document and prints the parse
    /// tree.
    Parse(ParseArgs),
}

/// Parse and describe Workflow Description Language documents.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about)]
struct Args {
    /// The subcommand to execute.
    #[command(subcommand)]
    command: Command,
}

fn inner() -> Result<()> {
    let args = Args::parse();

    env_logger::builder()
        .filter_level(LevelFilter::Debug)
        .init();

    match args.command {
        Command::Parse(args) => {
            let rule = match args.specification_version {
                Version::V1 => wdl::v1::get_rule(&args.rule)
                    .map(Ok)
                    .unwrap_or_else(|| Err(Error::UnknownRule(args.rule.clone())))?,
            };

            let contents = fs::read_to_string(args.path).map_err(Error::IoError)?;

            let mut parse_tree = match args.specification_version {
                Version::V1 => wdl::v1::Parser::parse(rule, &contents)
                    .map_err(|err| Error::PestError(Box::new(err)))?,
            };

            // For documents, we don't care about the parent element: it is much
            // more informative to see the children of the document split by
            // spaces. This is a stylistic choice.
            if args.rule == "document" {
                for element in parse_tree.next().unwrap().into_inner() {
                    dbg!(element);
                }
            } else {
                dbg!(parse_tree);
            };
        }
    }

    Ok(())
}

fn main() {
    match inner() {
        Ok(_) => {}
        Err(err) => eprintln!("{}", err),
    }
}
