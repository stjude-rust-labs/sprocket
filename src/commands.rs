//! Implementation of sprocket CLI commands.

use clap::Subcommand;

pub mod analyzer;
pub mod check;
pub mod completions;
pub mod config;
pub mod explain;
pub mod format;
pub mod inputs;
pub mod lock;
pub mod run;
pub mod validate;

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

    /// Development commands.
    #[command(subcommand)]
    Dev(DevCommands),
}

/// Developmental and experimental commands.
#[derive(Subcommand, Debug)]
pub enum DevCommands {
    /// Locks Docker images to a sha256 digest.
    Lock(lock::Args),
}
