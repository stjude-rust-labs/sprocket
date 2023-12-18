//! `wdl-grammar parse`

use clap::Parser;
use log::warn;
use wdl_grammar as grammar;

use crate::commands::get_contents_stdin;

/// An error related to the `wdl-grammar parse` subcommand.
#[derive(Debug)]
pub enum Error {
    /// A common error.
    Common(super::Error),

    /// An error parsing the WDL 1.x grammar.
    GrammarV1(grammar::v1::Error),

    /// Unknown rule name.
    UnknownRule {
        /// The name of the rule.
        name: String,

        /// The grammar being used.
        grammar: wdl_core::Version,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Common(err) => write!(f, "{err}"),
            Error::GrammarV1(err) => write!(f, "grammar parse error: {err}"),
            Error::UnknownRule { name, grammar } => {
                write!(f, "unknown rule '{name}' for grammar {grammar}")
            }
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
type Result<T> = std::result::Result<T, Error>;

/// Arguments for the `wdl-grammar parse` subcommand.
#[derive(Debug, Parser)]
pub struct Args {
    /// The input to parse.
    #[clap(value_name = "INPUT")]
    input: Option<String>,

    /// The Workflow Description Language (WDL) specification version to use.
    #[arg(value_name = "VERSION", short = 's', long, default_value_t, value_enum)]
    specification_version: wdl_core::Version,

    /// The parser rule to evaluate.
    #[arg(value_name = "RULE", short = 'r', long, default_value = "document")]
    rule: String,

    /// Skips the parent element and prints each child.
    #[arg(short, long)]
    children_only: bool,
}

/// Main function for this subcommand.
pub fn parse(args: Args) -> Result<()> {
    let rule = match args.specification_version {
        wdl_core::Version::V1 => {
            grammar::v1::get_rule(&args.rule)
                .map(Ok)
                .unwrap_or_else(|| {
                    Err(Error::UnknownRule {
                        name: args.rule.clone(),
                        grammar: args.specification_version.clone(),
                    })
                })?
        }
    };

    let input = args
        .input
        .map(Ok)
        .unwrap_or_else(|| get_contents_stdin().map_err(Error::Common))?;

    let parse_tree = match args.specification_version {
        wdl_core::Version::V1 => grammar::v1::parse_rule(rule, &input).map_err(Error::GrammarV1)?,
    };

    if let Some(concerns) = parse_tree.concerns() {
        for concern in concerns.inner().iter() {
            warn!("{}", concern);
        }
    }

    if let Some(pt) = parse_tree.into_tree() {
        let nodes = if args.children_only {
            pt.into_inner().collect::<Vec<_>>()
        } else {
            vec![pt]
        };

        for node in nodes {
            println!("{:#?}", node);
        }
    }

    Ok(())
}
