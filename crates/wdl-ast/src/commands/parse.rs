//! `wdl-ast parse`

use std::path::PathBuf;

use clap::Parser;
use log::warn;
use wdl_ast as ast;
use wdl_grammar as grammar;

/// An error related to the `wdl-ast parse` subcommand.
#[derive(Debug)]
pub enum Error {
    /// A WDL 1.x abstract syntax tree error.
    AstV1(ast::v1::Error),

    /// An input/output error.
    Io(std::io::Error),

    /// A WDL 1.x grammar error.
    GrammarV1(grammar::v1::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::AstV1(err) => write!(f, "ast error: {err}"),
            Error::Io(err) => write!(f, "i/o error: {err}"),
            Error::GrammarV1(err) => write!(f, "grammar error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
type Result<T> = std::result::Result<T, Error>;

/// Arguments for the `wdl-ast parse` subcommand.
#[derive(Debug, Parser)]
pub struct Args {
    /// Path to the WDL document.
    #[clap(value_name = "PATH")]
    path: PathBuf,

    /// The Workflow Description Language (WDL) specification version to use.
    #[arg(value_name = "VERSION", short = 's', long, default_value_t, value_enum)]
    specification_version: wdl_core::Version,
}

/// Main function for this subcommand.
pub fn parse(args: Args) -> Result<()> {
    let contents = std::fs::read_to_string(args.path).map_err(Error::Io)?;

    let document = match args.specification_version {
        wdl_core::Version::V1 => {
            let pt = grammar::v1::parse(&contents).map_err(Error::GrammarV1)?;
            ast::v1::parse(pt.into_tree().unwrap()).map_err(Error::AstV1)?
        }
    };

    if let Some(concerns) = document.concerns() {
        for warning in concerns.inner().iter() {
            warn!("{}", warning);
        }
    }

    dbg!(document);

    Ok(())
}
