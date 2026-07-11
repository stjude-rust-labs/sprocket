//! Cargo-style status output shared by CLI subcommands.

use std::fmt;
use std::io;
use std::io::Write as _;

use anyhow::Context as _;
use colored::Colorize as _;

/// A console printer for cargo-style status lines.
///
/// Owns the colorization decision so subcommands don't thread a bare
/// `bool` through every call. Cheap to copy; construct it once from the
/// resolved color mode and pass it down by value.
#[derive(Clone, Copy, Debug)]
pub struct Printer {
    /// Whether to colorize the leading action verb.
    colorize: bool,
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

impl Printer {
    /// Creates a printer that colorizes output when `colorize` is true.
    pub fn new(colorize: bool) -> Self {
        Self { colorize }
    }

    /// Returns whether output should be colorized.
    ///
    /// For interop with APIs that take the raw flag (e.g. diagnostic
    /// emission).
    pub fn colorize(&self) -> bool {
        self.colorize
    }

    /// Prints a successful or constructive action line (green verb).
    pub fn status(&self, verb: &str, rest: impl fmt::Display) {
        self.action(verb, rest, ActionColor::Green);
    }

    /// Prints an update or dry-run change action line (yellow verb).
    pub fn change(&self, verb: &str, rest: impl fmt::Display) {
        self.action(verb, rest, ActionColor::Yellow);
    }

    /// Prints an informational action line (cyan verb).
    pub fn info(&self, verb: &str, rest: impl fmt::Display) {
        self.action(verb, rest, ActionColor::Cyan);
    }

    /// Prints a failed action line (red verb).
    pub fn failure(&self, verb: &str, rest: impl fmt::Display) {
        self.action(verb, rest, ActionColor::Red);
    }

    /// Prints a warning line to stderr.
    pub fn warn(&self, msg: impl fmt::Display) {
        if self.colorize {
            eprintln!("{}: {msg}", "warning".yellow().bold());
        } else {
            eprintln!("warning: {msg}");
        }
    }

    /// Prints an error line to stderr.
    pub fn error(&self, msg: impl fmt::Display) {
        if self.colorize {
            eprintln!("{}: {msg}", "error".red().bold());
        } else {
            eprintln!("error: {msg}");
        }
    }

    /// Prints a confirmation prompt to stderr and reads one line from stdin.
    ///
    /// Appends ` [y/N] ` to the prompt. Any case variant of `y`/`yes`
    /// accepts; anything else — including EOF and an empty line — declines.
    pub fn confirm(&self, prompt: impl fmt::Display) -> anyhow::Result<bool> {
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
    fn action(&self, verb: &str, rest: impl fmt::Display, color: ActionColor) {
        if self.colorize {
            println!("{} {rest}", color.apply(verb));
        } else {
            println!("{verb} {rest}");
        }
    }
}
