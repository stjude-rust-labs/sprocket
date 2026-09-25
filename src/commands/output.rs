//! User-facing command output.

use std::fmt;
use std::io;
use std::io::IsTerminal as _;
use std::io::Write as _;

use anyhow::Context as _;
use colored::Colorize as _;
use dialoguer::Confirm;

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
    pub(crate) fn new(colorize: bool) -> Self {
        Self { colorize }
    }

    /// Prints a completed operation.
    pub(crate) fn completed(self, action: Action, subject: impl fmt::Display) {
        self.action(action.completed, subject, ActionColor::Green);
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

    /// Prints an indented label and value beneath an outcome.
    pub(crate) fn detail(self, label: &str, value: impl fmt::Display) {
        if self.colorize {
            println!("  {:<10} {value}", label.cyan().bold());
        } else {
            println!("  {label:<10} {value}");
        }
    }

    /// Prints command payload to stdout without decoration.
    pub(crate) fn payload(self, value: impl fmt::Display) {
        println!("{value}");
    }

    /// Prints interactive context to stderr without decoration.
    pub(crate) fn diagnostic(self, value: impl fmt::Display) {
        eprintln!("{value}");
    }

    /// Prints a blank interactive-context line to stderr.
    pub(crate) fn diagnostic_blank(self) {
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

    /// Prints an action line with only the verb colored.
    fn action(self, verb: &str, rest: impl fmt::Display, color: ActionColor) {
        if self.colorize {
            println!("{} {rest}", color.apply(verb));
        } else {
            println!("{verb} {rest}");
        }
    }
}
