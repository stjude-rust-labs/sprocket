//! Subcommands for the `wdl-grammar` command-line tool.

use log::debug;

pub mod create_test;
pub mod gauntlet;
pub mod parse;

/// An error common to any subcommand.
#[derive(Debug)]
pub enum Error {
    /// An input/output error.
    InputOutput(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::InputOutput(err) => write!(f, "i/o error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
type Result<T> = std::result::Result<T, Error>;

/// Gets lines of input from STDIN.
pub fn get_contents_stdin() -> Result<String> {
    debug!("Reading from STDIN...");

    Ok(std::io::stdin()
        .lines()
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Error::InputOutput)?
        .join("\n"))
}
