//! Implementation of sprocket CLI commands.

use std::fmt;
use std::io;
use std::io::IsTerminal;
use std::io::Write as _;
use std::sync::Arc;

use anyhow::Context as _;
use clap::Subcommand;
use colored::ColoredString;
use colored::Colorize;
use dialoguer::Confirm;
use nonempty::NonEmpty;
use similar::DiffableStr;
use tracing::warn;
use wdl::engine::Config as EngineConfig;

pub mod analyzer;
pub mod check;
pub mod completions;
pub mod config;
pub mod doc;
pub mod explain;
pub mod format;
pub mod inputs;
pub mod lock;
pub mod module;
pub mod run;
pub mod server;
pub mod test;
pub mod validate;

/// Determines whether the engine is configured to run tasks with Docker.
///
/// A misnamed backend is reported as not using Docker; the configuration is
/// validated before evaluation starts, which is where that is diagnosed.
pub fn uses_docker_backend(engine: &EngineConfig) -> bool {
    engine
        .backend()
        .map(|config| config.as_docker().is_some())
        .unwrap_or(false)
}

/// Warns that terminating Sprocket leaves Docker containers, and the files
/// those containers created, behind.
///
/// Call this only when [`uses_docker_backend`] holds and Sprocket is about to
/// exit without waiting for executing tasks to cancel.
pub fn warn_docker_termination() {
    warn!(
        "terminating Sprocket does not remove Docker containers that are still running; files \
         that were created by containers may remain owned by another user (e.g. `root`) and \
         require elevated privileges to remove"
    );
}

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
    /// Create and manage WDL modules.
    #[command(subcommand)]
    Module(module::ModuleCommands),
    /// Run-management server commands (start, submit, status, inspect, cancel,
    /// retry).
    Server(server::Args),
    /// Runs unit tests for a WDL workspace.
    Test(test::Args),
}

/// A command operation with completed and planned forms.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Action {
    completed: &'static str,
    planned: &'static str,
}

impl Action {
    /// Creates an action from its completed and planned forms.
    pub(crate) const fn new(completed: &'static str, planned: &'static str) -> Self {
        Self { completed, planned }
    }
}

/// Color applied to the leading action verb of a status line.
#[derive(Clone, Copy, Debug)]
enum ActionColor {
    /// Successful or constructive action.
    Green,
    /// Update or dry-run change action.
    Yellow,
    /// Informational action.
    Cyan,
    /// Failed action.
    Red,
}

impl ActionColor {
    /// Applies this color to an action verb.
    fn apply(self, verb: &str) -> String {
        match self {
            Self::Green => verb.green().bold().to_string(),
            Self::Yellow => verb.yellow().bold().to_string(),
            Self::Cyan => verb.cyan().bold().to_string(),
            Self::Red => verb.red().bold().to_string(),
        }
    }
}

/// Presentation shared by interactive commands.
///
/// Owns the colorization decision so subcommands do not thread a bare `bool`
/// through every call. Cheap to copy; construct it once from the resolved color
/// mode and pass it down by value.
#[derive(Clone, Copy, Debug)]
pub struct CommandOutput {
    /// Whether to colorize the leading action verb.
    colorize: bool,
}

impl CommandOutput {
    /// Creates command output using the resolved color mode.
    pub fn new(colorize: bool) -> Self {
        Self { colorize }
    }

    /// Returns whether user-facing output should be colorized.
    pub(crate) fn colorize(self) -> bool {
        self.colorize
    }

    /// Returns the styled text when colorizing, or its plain text otherwise.
    pub(crate) fn style(self, value: ColoredString) -> String {
        if self.colorize {
            value.to_string()
        } else {
            value.input
        }
    }

    /// Prints a completed operation.
    pub(crate) fn completed(self, action: Action, subject: impl fmt::Display) {
        self.action(action.completed, subject, ActionColor::Green);
    }

    /// Prints a completed operation to stderr.
    pub(crate) fn completed_stderr(self, action: Action, subject: impl fmt::Display) {
        self.stderr_action(action.completed, subject, ActionColor::Green);
    }

    /// Prints an operation that would occur without mutation.
    pub(crate) fn planned(self, action: Action, subject: impl fmt::Display) {
        self.action(
            &format!("Would {}", action.planned),
            subject,
            ActionColor::Yellow,
        );
    }

    /// Prints a successful no-op.
    pub(crate) fn current(self, subject: impl fmt::Display) {
        self.action("Current", subject, ActionColor::Cyan);
    }

    /// Prints a skipped operation.
    pub(crate) fn skipped(self, subject: impl fmt::Display) {
        self.action("Skipped", subject, ActionColor::Cyan);
    }

    /// Prints a failed operation.
    pub(crate) fn failed(self, subject: impl fmt::Display) {
        self.action("Failed", subject, ActionColor::Red);
    }

    /// Writes a completed operation to a writer.
    ///
    /// Used when output must pass through a progress-aware writer.
    pub(crate) fn write_completed(
        self,
        writer: &mut impl io::Write,
        action: Action,
        subject: impl fmt::Display,
    ) -> io::Result<()> {
        writeln!(
            writer,
            "{}",
            self.format_action(action.completed, subject, ActionColor::Green)
        )
    }

    /// Writes a failed operation to a writer.
    ///
    /// Used when output must pass through a progress-aware writer.
    pub(crate) fn write_failed(
        self,
        writer: &mut impl io::Write,
        subject: impl fmt::Display,
    ) -> io::Result<()> {
        writeln!(
            writer,
            "{}",
            self.format_action("Failed", subject, ActionColor::Red)
        )
    }

    /// Prints an indented label and value beneath an outcome.
    pub(crate) fn detail(self, label: &str, value: impl fmt::Display) {
        if self.colorize {
            println!("  {:<10} {value}", label.cyan().bold());
        } else {
            println!("  {label:<10} {value}");
        }
    }

    /// Prints an undecorated payload to stdout.
    pub(crate) fn payload(self, value: impl fmt::Display) {
        if value.to_string().ends_with_newline() {
            print!("{value}");
        } else {
            println!("{value}");
        }
    }

    /// Prints an undecorated value to stderr.
    pub(crate) fn stderr(self, value: impl fmt::Display) {
        if value.to_string().ends_with_newline() {
            eprint!("{value}");
        } else {
            eprintln!("{value}");
        }
    }

    /// Prints a blank line to stdout.
    pub(crate) fn blank(self) {
        println!();
    }

    /// Prints a blank line to stderr.
    pub(crate) fn stderr_blank(self) {
        eprintln!();
    }

    /// Prints a confirmation prompt and reads one key from the terminal.
    ///
    /// The prompt defaults to `no`, so Enter and `n` decline while `y`
    /// accepts. When stdin or stderr is redirected, the line-based fallback
    /// keeps the prompt usable from scripts and tests.
    pub(crate) fn confirm(self, prompt: impl fmt::Display) -> anyhow::Result<bool> {
        let prompt = prompt.to_string();
        if io::stdin().is_terminal() && io::stderr().is_terminal() {
            return Confirm::new()
                .with_prompt(prompt)
                .default(false)
                .interact()
                .context("reading prompt response");
        }

        eprint!("{prompt} [y/N] ");
        io::stderr().flush().context("flushing prompt")?;
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("reading prompt response")?;
        Ok(matches!(
            input.trim().to_ascii_lowercase().as_str(),
            "y" | "yes"
        ))
    }

    /// Formats an action line with only the verb colored.
    fn format_action(self, verb: &str, rest: impl fmt::Display, color: ActionColor) -> String {
        if self.colorize {
            format!("{} {rest}", color.apply(verb))
        } else {
            format!("{verb} {rest}")
        }
    }

    /// Prints an action line with only the verb colored.
    fn action(self, verb: &str, rest: impl fmt::Display, color: ActionColor) {
        println!("{}", self.format_action(verb, rest, color));
    }

    /// Prints an action line to stderr with only the verb colored.
    fn stderr_action(self, verb: &str, rest: impl fmt::Display, color: ActionColor) {
        eprintln!("{}", self.format_action(verb, rest, color));
    }
}
