//! `wdl-grammar parse`

use clap::Parser;
use pest::Parser as _;

use wdl_grammar as grammar;

use crate::commands::get_contents_stdin;

/// An error related to the `wdl-grammar parse` subcommand.
#[derive(Debug)]
pub enum Error {
    /// An empty parse tree was encountered.
    ChildrenOnlyWithEmptyParseTree,

    /// A common error.
    Common(super::Error),

    /// A parsing error from Pest.
    Parse(Box<dyn std::error::Error>),

    /// Unknown rule name.
    UnknownRule {
        /// The name of the rule.
        name: String,

        /// The grammar being used.
        grammar: grammar::Version,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::ChildrenOnlyWithEmptyParseTree => {
                write!(f, "cannot print children with empty parse tree")
            }
            Error::Common(err) => write!(f, "{err}"),
            Error::UnknownRule { name, grammar } => {
                write!(f, "unknown rule '{name}' for grammar {grammar}")
            }
            Error::Parse(err) => write!(f, "parse error: {err}"),
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
    specification_version: grammar::Version,

    /// The parser rule to evaluate.
    #[arg(value_name = "RULE", short = 'r', long, default_value = "document")]
    rule: String,

    /// Skips the parent element and prints each child.
    #[arg(short, long, global = true)]
    children_only: bool,
}

/// Main function for this subcommand.
pub fn parse(args: Args) -> Result<()> {
    let rule = match args.specification_version {
        grammar::Version::V1 => grammar::v1::get_rule(&args.rule)
            .map(Ok)
            .unwrap_or_else(|| {
                Err(Error::UnknownRule {
                    name: args.rule.clone(),
                    grammar: args.specification_version.clone(),
                })
            })?,
    };

    let input = args
        .input
        .map(Ok)
        .unwrap_or_else(|| get_contents_stdin().map_err(Error::Common))?;

    let mut parse_tree = match args.specification_version {
        grammar::Version::V1 => {
            grammar::v1::Parser::parse(rule, &input).map_err(|err| Error::Parse(Box::new(err)))?
        }
    };

    if args.children_only {
        let children = match parse_tree.next() {
            Some(root) => root.into_inner(),
            None => return Err(Error::ChildrenOnlyWithEmptyParseTree),
        };

        for child in children {
            dbg!(child);
        }
    } else {
        dbg!(parse_tree);
    };

    Ok(())
}
