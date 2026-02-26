//! Implementation of sprocket CLI commands.

use std::fmt;
use std::io::IsTerminal;
use std::sync::Arc;

use clap::Subcommand;
use colored::Colorize;
use nonempty::NonEmpty;

pub mod analyzer;
pub mod check;
pub mod completions;
pub mod config;
pub mod doc;
pub mod explain;
pub mod format;
pub mod inputs;
pub mod lock;
pub mod run;
pub mod server;
pub mod test;
pub mod validate;

/// Represents an error that may result from a command.
///
/// The error may be from a single error source or multiple errors resulting
/// from WDL source file analysis.
#[derive(Debug)]
pub enum CommandError {
    /// The error is a single `anyhow::Error`.
    Single(anyhow::Error),
    /// The error is multiple shared `anyhow::Error`.
    Multiple(NonEmpty<Arc<anyhow::Error>>),
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn write(f: &mut fmt::Formatter<'_>, e: &anyhow::Error) -> fmt::Result {
            write!(
                f,
                "{error}: {e:?}",
                error = if std::io::stderr().is_terminal() {
                    "error".red().bold()
                } else {
                    "error".normal()
                }
            )
        }

        match self {
            Self::Single(e) => write(f, e),
            Self::Multiple(errors) => {
                for (i, e) in errors.iter().enumerate() {
                    if i > 0 {
                        writeln!(f)?;
                    }

                    write(f, e)?;
                }

                Ok(())
            }
        }
    }
}

impl From<anyhow::Error> for CommandError {
    fn from(e: anyhow::Error) -> Self {
        Self::Single(e)
    }
}

impl From<NonEmpty<Arc<anyhow::Error>>> for CommandError {
    fn from(errors: NonEmpty<Arc<anyhow::Error>>) -> Self {
        Self::Multiple(errors)
    }
}

/// Represents the result of a command.
pub type CommandResult<T> = std::result::Result<T, CommandError>;

/// Represents the available commands for the Sprocket CLI.
#[derive(Subcommand, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Commands {
    /// Runs the Language Server Protocol (LSP) server.
    Analyzer(analyzer::Args),

    /// Checks a document or a directory containing documents.
    Check(check::CheckArgs),

    /// Generates shell completions.
    Completions(completions::Args),

    /// Display the effective configuration.
    Config(config::Args),

    /// Explains linting and validation rules.
    Explain(explain::Args),

    /// Formats a document or a directory containing documents.
    #[clap(alias = "fmt")]
    Format(format::Args),

    /// Writes the inputs schema for a WDL document.
    Inputs(inputs::Args),

    /// Lints a document or a directory containing documents.
    Lint(check::LintArgs),

    /// Runs a task or workflow.
    Run(run::Args),

    /// Validate a set of inputs against a task or workflow.
    ///
    /// This ensures that every required input is supplied, every supplied input
    /// is correctly typed, that no extraneous inputs are provided, and that any
    /// provided `File` or `Directory` inputs exist.
    ///
    /// It will not catch potential runtime errors that may occur when running
    /// the task or workflow.
    Validate(validate::Args),

    /// Developmental and experimental commands.
    #[command(subcommand)]
    Dev(DevCommands),
}

/// Developmental and experimental commands.
#[derive(Subcommand, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum DevCommands {
    /// Document a workspace.
    Doc(doc::Args),
    /// Locks Docker images to a sha256 digest.
    Lock(lock::Args),
    /// Runs the HTTP API server for run execution.
    Server(server::Args),
    /// Runs unit tests for a WDL workspace.
    Test(test::Args),
}
