//! `wdl-grammar create-test`

use clap::Parser;
use log::warn;
use pest::iterators::Pair;
use pest::RuleType;

use wdl_grammar as grammar;

use crate::commands::get_contents_stdin;

/// An error related to the `wdl-grammar create-test` subcommand.
#[derive(Debug)]
pub enum Error {
    /// A common error.
    Common(super::Error),

    /// Multiple root nodes parsed.
    MultipleRootNodes,

    /// An error parsing the grammar.
    GrammarV1(grammar::Error<grammar::v1::Rule>),

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
            Error::Common(err) => write!(f, "{err}"),
            Error::GrammarV1(err) => write!(f, "grammar parse error: {err}"),
            Error::MultipleRootNodes => write!(f, "multiple root nodes found"),
            Error::UnknownRule { name, grammar } => {
                write!(f, "unknown rule '{name}' for grammar {grammar}")
            }
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
type Result<T> = std::result::Result<T, Error>;

/// Arguments for the `wdl-grammar create-test` subcommand.
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
}

/// Main function for this subcommand.
pub fn create_test(args: Args) -> Result<()> {
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
        grammar::Version::V1 => grammar::v1::parse(rule, &input).map_err(Error::GrammarV1)?,
    };

    if let Some(warnings) = parse_tree.warnings() {
        for warning in warnings {
            warn!("{}", warning);
        }
    }

    let root = match parse_tree.len() {
        // SAFETY: this should not be possible, as parsing just successfully
        // completed. As such, we should always have at least one parsed
        // element.
        0 => unreachable!(),
        1 => parse_tree.next().unwrap(),
        _ => return Err(Error::MultipleRootNodes),
    };

    write_test(root, 0);

    Ok(())
}

/// Writes a test by recursively traversing the [`Pair`].
fn write_test<R: RuleType>(pair: Pair<'_, R>, indent: usize) {
    let span = pair.as_span();
    let prefix = " ".repeat(indent);

    let comment = pair
        .as_str()
        .lines()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    if !comment.is_empty() {
        println!("{}// `{}`", prefix, comment);
    }

    print!(
        "{}{:?}({}, {}",
        prefix,
        pair.as_rule(),
        span.start(),
        span.end()
    );

    let inner = pair.into_inner();

    if inner.peek().is_some() {
        println!(", [");

        for pair in inner {
            write_test(pair, indent + 2);
            println!(",");
        }

        print!("{}]", prefix);
    }

    print!(")");
}
