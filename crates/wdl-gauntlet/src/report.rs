//! Reporting.

use std::collections::HashMap;
use std::time::Duration;

use colored::Colorize as _;
use indexmap::IndexSet;

use crate::document;
use crate::repository;

/// Represents an unmatched status.
#[derive(Clone, Debug)]
pub struct UnmatchedStatus {
    /// The missing set of diagnostics.
    ///
    /// These are the diagnostics that were in the config but were not
    /// emitted by the parser/validator.
    pub missing: IndexSet<String>,
    /// The unexpected set of diagnostics.
    ///
    /// These are the diagnostics that were not in the configuration but
    /// were emitted by the parser/validator.
    pub unexpected: IndexSet<String>,
    /// The set of all diagnostics that were emitted.
    pub all: IndexSet<String>,
}

/// The status of a single parsing test.
#[derive(Clone, Debug)]
pub enum Status {
    /// The document passed parsing, validation, and linting successfully with
    /// no diagnostics to report.
    Success,

    /// The document had diagnostics, but the diagnostics exactly matched what
    /// was already expected in the configuration, meaning that this
    /// [`Status`] is considered successful.
    DiagnosticsMatched(IndexSet<String>),

    /// The document had diagnostics, but they did not match what was expected
    /// in the configuration.
    DiagnosticsUnmatched(Box<UnmatchedStatus>),
}

/// A printable section within a report for a repository.
#[derive(Debug, Eq, PartialEq)]
pub enum Section {
    /// Title of the section.
    Title,

    /// Summarized status of each parsing test.
    Summary,

    /// Detailed information on unexpected diagnostics that were encountered.
    Diagnostics,

    /// Summarized information about all results reported.
    Footer,
}

impl Status {
    /// Gets whether the status is considered a successful parsing test.
    ///
    /// This is used to determine the numerator when calculating the percentage
    /// of tests that passed for a repository.
    pub fn success(&self) -> bool {
        match self {
            Self::Success | Self::DiagnosticsMatched(_) => true,
            Self::DiagnosticsUnmatched { .. } => false,
        }
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success => write!(f, "✅"),
            Self::DiagnosticsMatched(_) => write!(f, "📘"),
            Self::DiagnosticsUnmatched(status) => {
                if !status.unexpected.is_empty() {
                    write!(f, "❌")?;
                }

                if !status.missing.is_empty() {
                    write!(f, "🔄️")?;
                }

                Ok(())
            }
        }
    }
}

/// A mapping between [document identifiers](document::Identifier) and the
/// [status](Status) of their parsing test.
pub type Results = HashMap<document::Identifier, Status>;

/// A terminal-based report.
#[derive(Debug)]
pub struct Report<T: std::io::Write> {
    /// The handle to which to write the report.
    inner: T,

    /// The reporting section we are currently on.
    section: Section,

    /// The [results of the testing](Results) of the parsing tests.
    ///
    /// **Note:** these are appended as we parse them, so they need to be
    /// filtered by the repository you are currently considering.
    results: Results,

    /// Whether or not _anything_ was printed for the current section.
    ///
    /// This informs us of whether we need to print a space between sections
    /// when we transition to the next section. In other words, if we didn't
    /// print anything in a section, we don't want to separate the first and
    /// third sections by _two_ spaces.
    printed: bool,
}

impl<T: std::io::Write> Report<T> {
    /// Consumes the report and returns the [`Results`].
    // **Note:** see the note at [Self::results] for a caveat on interpretation
    // of these results.
    pub fn into_results(self) -> Results {
        self.results
    }

    /// Transitions to the next section of the report.
    ///
    /// **Note:** when a report rolls over to the next repository, the
    /// [`Section::Footer`] simply transitions to a [`Section::Title`].
    pub fn next_section(&mut self) -> std::io::Result<()> {
        self.section = match self.section {
            Section::Title => Section::Summary,
            Section::Summary => {
                if self.results.is_empty() {
                    write!(self.inner, "⚠️ No items reported for this repository!")?;
                }

                Section::Diagnostics
            }
            Section::Diagnostics => Section::Footer,
            Section::Footer => Section::Title,
        };

        if self.printed && self.section != Section::Title {
            writeln!(self.inner)?;
        }

        self.printed = false;

        Ok(())
    }

    /// Prints the title for a repository report.
    pub fn title(&mut self, name: impl std::fmt::Display) -> std::io::Result<()> {
        if self.section != Section::Title {
            panic!(
                "cannot print a new title when report phase is {:?}",
                self.section
            );
        }

        let name = name.to_string();
        writeln!(self.inner, "{}", name.bold().underline())?;
        self.printed = true;

        Ok(())
    }

    /// Registers and prints a single parse test result for a repository report.
    pub fn register(
        &mut self,
        identifier: document::Identifier,
        status: Status,
        elapsed: Duration,
    ) -> std::io::Result<()> {
        if self.section != Section::Summary {
            panic!(
                "cannot register a status when the report phase is {:?}",
                self.section
            );
        }

        writeln!(self.inner, "{status} {identifier} ({elapsed:?})")?;
        self.results.insert(identifier, status);
        self.printed = true;

        Ok(())
    }

    /// Reports all unexpected errors within a repository report.
    pub fn report_unexpected_errors(
        &mut self,
        repository_identifier: &repository::Identifier,
    ) -> std::io::Result<()> {
        if self.section != Section::Diagnostics {
            panic!(
                "cannot report unexpected diagnostics when the report phase is {:?}",
                self.section
            );
        }

        for (i, (id, status)) in self
            .results
            .iter()
            .filter(|(id, _)| id.repository() == repository_identifier)
            .filter_map(|(id, status)| match status {
                Status::DiagnosticsUnmatched(status) => Some((id, status)),
                _ => None,
            })
            .enumerate()
        {
            self.printed = true;

            if i > 0 {
                writeln!(self.inner)?;
            }

            writeln!(self.inner, "{id}", id = id.path().italic())?;

            for diagnostic in &status.unexpected {
                writeln!(self.inner, "❌ {diagnostic}")?;
            }

            for diagnostic in &status.missing {
                writeln!(self.inner, "🔄️ {diagnostic}")?;
            }
        }

        Ok(())
    }

    /// Prints the summary footer for a repository report.
    pub fn footer(
        &mut self,
        repository_identifier: &repository::Identifier,
    ) -> std::io::Result<()> {
        if self.section != Section::Footer {
            panic!(
                "cannot report footer when the report phase is {:?}",
                self.section
            );
        }

        if self.results.is_empty() {
            return Ok(());
        }

        let (passed, considered, missing, unexpected) = self
            .results
            .iter()
            .filter(|(id, _)| id.repository() == repository_identifier)
            .map(|(_, s)| s)
            .fold((0, 0, 0, 0), |mut counts, status| {
                if status.success() {
                    counts.0 += 1
                }

                counts.1 += 1;

                if let Status::DiagnosticsUnmatched(unmatched) = status {
                    counts.2 += unmatched.missing.len();
                    counts.3 += unmatched.unexpected.len();
                }

                counts
            });

        write!(self.inner, "Passed {}/{} tests", passed, considered)?;

        let mut with = Vec::new();

        match missing {
            0 => {}
            1 => with.push(String::from("1 missing diagnostic")),
            v => with.push(format!("{} missing diagnostics", v)),
        }

        match unexpected {
            0 => {}
            1 => with.push(String::from("1 unexpected diagnostic")),
            v => with.push(format!("{} unexpected diagnostics", v)),
        }

        if !with.is_empty() {
            write!(self.inner, " (with {})", with.join(" and "))?;
        }

        writeln!(
            self.inner,
            " ({:.1}%)",
            (passed as f64 / considered as f64) * 100.0
        )?;
        self.printed = true;

        Ok(())
    }
}

impl<T: std::io::Write> From<T> for Report<T> {
    fn from(inner: T) -> Self {
        Self {
            inner,
            section: Section::Title,
            results: Default::default(),
            printed: false,
        }
    }
}
