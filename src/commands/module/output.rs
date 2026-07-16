//! User-facing presentation for module porcelain commands.

use std::fmt;

use crate::commands::printer::Printer;

/// An operation reported by a module command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ModuleAction {
    /// Initialize a module.
    Initialize,
    /// Add a dependency.
    Add,
    /// Remove a dependency or cached module.
    Remove,
    /// Lock dependencies.
    Lock,
    /// Update locked dependencies.
    Update,
    /// Upgrade manifest constraints.
    Upgrade,
    /// Fetch dependencies.
    Fetch,
    /// Clean cached dependencies.
    Clean,
    /// Sign a module.
    Sign,
    /// Verify module content.
    Verify,
}

impl ModuleAction {
    /// Returns the past-tense outcome verb.
    fn completed(self) -> &'static str {
        match self {
            Self::Initialize => "Initialized",
            Self::Add => "Added",
            Self::Remove => "Removed",
            Self::Lock => "Locked",
            Self::Update => "Updated",
            Self::Upgrade => "Upgraded",
            Self::Fetch => "Fetched",
            Self::Clean => "Cleaned",
            Self::Sign => "Signed",
            Self::Verify => "Verified",
        }
    }

    /// Returns the infinitive verb used for planned work.
    fn planned(self) -> &'static str {
        match self {
            Self::Initialize => "initialize",
            Self::Add => "add",
            Self::Remove => "remove",
            Self::Lock => "lock",
            Self::Update => "update",
            Self::Upgrade => "upgrade",
            Self::Fetch => "fetch",
            Self::Clean => "clean",
            Self::Sign => "sign",
            Self::Verify => "verify",
        }
    }
}

/// Consistent outcome and detail rendering for module commands.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ModuleOutput {
    printer: Printer,
}

impl ModuleOutput {
    /// Creates module output backed by the shared console printer.
    pub(crate) fn new(printer: Printer) -> Self {
        Self { printer }
    }

    /// Prints a completed operation.
    pub(crate) fn completed(self, action: ModuleAction, subject: impl fmt::Display) {
        self.printer.status(action.completed(), subject);
    }

    /// Prints an operation that would occur without mutation.
    pub(crate) fn planned(self, action: ModuleAction, subject: impl fmt::Display) {
        self.printer
            .change(&format!("Would {}", action.planned()), subject);
    }

    /// Prints a successful no-op.
    pub(crate) fn current(self, subject: impl fmt::Display) {
        self.printer.info("Current", subject);
    }

    /// Prints supporting information beneath an outcome.
    pub(crate) fn detail(self, label: &str, value: impl fmt::Display) {
        self.printer.detail(label, value);
    }
}

/// Formats a count with a singular or plural noun.
pub(crate) fn count_noun(count: usize, singular: &str, plural: &str) -> String {
    let noun = if count == 1 { singular } else { plural };
    format!("{count} {noun}")
}
