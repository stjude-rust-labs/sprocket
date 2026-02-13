//! Implementation of sprocket CLI commands.

use std::fmt;
use std::fmt::Debug;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::Command;
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
    /// A component was not found.
    MissingComponent {
        component: &'static str,
        component_dir: PathBuf,
    },
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
            Self::MissingComponent {
                component,
                component_dir,
            } => write!(
                f,
                "unable to locate component '{component}' (searched {}). Is it installed?",
                component_dir.display(),
            ),
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

/// Extension trait for debug printing [`Command`]s.
pub(crate) trait CommandDebugExt {
    /// Create a [`Debug`] adapter for this command.
    fn debug(&self) -> DebuggableCommand<'_>;
}

impl CommandDebugExt for Command {
    fn debug(&self) -> DebuggableCommand<'_> {
        DebuggableCommand(self)
    }
}

/// Wrapper for debug printing [`Command`]s.
pub struct DebuggableCommand<'a>(&'a Command);

impl Debug for DebuggableCommand<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut envs_str = String::new();
        for (key, value) in self.0.get_envs() {
            match value {
                Some(value) => envs_str.push_str(&format!(
                    "{}={} ",
                    key.to_string_lossy(),
                    value.to_string_lossy()
                )),
                None => envs_str.push_str(&format!("{} ", key.to_string_lossy())),
            }
        }

        let args_os = self.0.get_args();
        let mut args_str = String::new();
        if args_os.len() > 0 {
            args_str.push(' ');
            for arg in args_os {
                args_str.push_str(&format!("{} ", arg.to_string_lossy()));
            }
        }

        write!(
            f,
            "{envs_str}{}{args_spacer}{args_str}",
            self.0.get_program().to_string_lossy(),
            args_spacer = if args_str.is_empty() { "" } else { " " }
        )?;
        Ok(())
    }
}
