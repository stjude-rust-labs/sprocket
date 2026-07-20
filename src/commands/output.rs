//! User-facing command output.

use std::fmt;

use crate::commands::printer::Printer;

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

/// Presentation shared by interactive commands.
#[derive(Clone, Copy, Debug)]
pub struct CommandOutput {
    printer: Printer,
}

impl CommandOutput {
    /// Creates command output using the resolved color mode.
    pub(crate) fn new(colorize: bool) -> Self {
        Self {
            printer: Printer::new(colorize),
        }
    }

    /// Prints a completed operation.
    pub(crate) fn completed(self, action: Action, subject: impl fmt::Display) {
        self.printer.status(action.completed, subject);
    }

    /// Prints an operation that would occur without mutation.
    pub(crate) fn planned(self, action: Action, subject: impl fmt::Display) {
        self.printer
            .change(&format!("Would {}", action.planned), subject);
    }

    /// Prints a successful no-op.
    pub(crate) fn current(self, subject: impl fmt::Display) {
        self.printer.info("Current", subject);
    }

    /// Prints a skipped operation.
    pub(crate) fn skipped(self, subject: impl fmt::Display) {
        self.printer.info("Skipped", subject);
    }

    /// Prints a failed operation.
    pub(crate) fn failed(self, subject: impl fmt::Display) {
        self.printer.failure("Failed", subject);
    }

    /// Prints supporting information beneath an outcome.
    pub(crate) fn detail(self, label: &str, value: impl fmt::Display) {
        self.printer.detail(label, value);
    }

    /// Prints command payload to stdout.
    pub(crate) fn payload(self, value: impl fmt::Display) {
        self.printer.payload(value);
    }

    /// Prints interactive context to stderr.
    pub(crate) fn diagnostic(self, value: impl fmt::Display) {
        self.printer.diagnostic(value);
    }

    /// Prints a blank interactive-context line to stderr.
    pub(crate) fn diagnostic_blank(self) {
        self.printer.diagnostic_blank();
    }

    /// Reads confirmation from stdin after prompting on stderr.
    pub(crate) fn confirm(self, prompt: impl fmt::Display) -> anyhow::Result<bool> {
        self.printer.confirm(prompt)
    }
}

/// Formats a count with a singular or plural noun.
pub(crate) fn count_noun(count: usize, singular: &str, plural: &str) -> String {
    let noun = if count == 1 { singular } else { plural };
    format!("{count} {noun}")
}
