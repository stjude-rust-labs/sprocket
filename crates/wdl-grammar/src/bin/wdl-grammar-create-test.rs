//! A command-line tool to automatically generate tests for WDL syntax.
//!
//! This tool is only intended to be used in the development of the
//! `wdl-grammar` package. It was written quickly and relatively sloppily in
//! contrast to the rest of this package—please keep that in mind!

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![deny(rustdoc::broken_intra_doc_links)]

use std::fs;
use std::path::PathBuf;

use clap::Parser;
use log::LevelFilter;

use pest::Parser as _;

use pest::iterators::Pair;
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

/// A command-line tool to automatically generate tests for WDL syntax.
#[derive(Debug, Parser)]
pub struct Args {
    /// The path to the document.
    path: PathBuf,

    /// The WDL specification version to use.
    #[arg(short = 's', long, default_value_t, value_enum)]
    specification_version: Version,

    /// The rule to evaluate.
    #[arg(short = 'r', long, default_value = "document")]
    rule: String,
}

fn inner() -> Result<()> {
    let args = Args::parse();

    env_logger::builder()
        .filter_level(LevelFilter::Debug)
        .init();

    let rule = match args.specification_version {
        Version::V1 => wdl::v1::get_rule(&args.rule)
            .map(Ok)
            .unwrap_or_else(|| Err(Error::UnknownRule(args.rule.clone())))?,
    };

    let contents = fs::read_to_string(args.path).map_err(Error::IoError)?;

    match args.specification_version {
        Version::V1 => {
            let parse_tree: pest::iterators::Pairs<'_, wdl::v1::Rule> =
                wdl::v1::Parser::parse(rule, &contents)
                    .map_err(|err| Error::PestError(Box::new(err)))?;

            for pair in parse_tree {
                print_create_test_recursive(pair, 0);
            }

            Ok(())
        }
    }
}

fn print_create_test_recursive(pair: Pair<'_, wdl::v1::Rule>, indent: usize) {
    let span = pair.as_span();
    let comment = pair
        .as_str()
        .lines()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    if !comment.is_empty() {
        println!("{}// `{}`", " ".repeat(indent), comment);
    }
    print!(
        "{}{:?}({}, {}",
        " ".repeat(indent),
        pair.as_rule(),
        span.start(),
        span.end()
    );

    let inner = pair.into_inner();

    if inner.peek().is_some() {
        println!(", [");

        for pair in inner {
            print_create_test_recursive(pair, indent + 2);
            println!(",");
        }

        print!("{}]", " ".repeat(indent));
    }

    print!(")");
}

fn main() {
    match inner() {
        Ok(_) => {}
        Err(err) => eprintln!("{}", err),
    }
}
