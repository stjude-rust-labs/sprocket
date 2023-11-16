//! Reporting.

use std::collections::HashMap;

use colored::Colorize as _;
use indexmap::IndexMap;

use wdl_grammar as grammar;

use grammar::core::lint;

use crate::commands::gauntlet::repository;
use crate::gauntlet::document;

/// The status of a single parsing test.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Status {
    /// The item passed successfully with no warnings.
    Success,

    /// The item was successful, but warnings were emitted.
    Warning,

    /// The item failed, but the error message did not match the expected error
    /// message.
    Mismatch(String),

    /// The item failed when it was expected to succeed.
    Error(String),

    /// The item was skipped because it failed, but that failure was explicitly
    /// ignored.
    Ignored(String),
}

/// A printable section within a report for a repository.
#[derive(Debug, Eq, PartialEq)]
pub enum Section {
    /// Title of the section.
    Title,

    /// Summarized status of each parsing test.
    Summary,

    /// Detailed information on unexpected errors that were encountered.
    Errors,

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
            Status::Success => true,
            Status::Warning => true,
            Status::Mismatch(_) => false,
            Status::Error(_) => false,
            Status::Ignored(_) => false,
        }
    }

    /// Gets whether the status was considered at all.
    ///
    /// This is used to determine the denominator when calculating the
    /// percentage of tests that passed for a repository.
    pub fn considered(&self) -> bool {
        match self {
            Status::Success => true,
            Status::Warning => true,
            Status::Mismatch(_) => true,
            Status::Error(_) => true,
            Status::Ignored(_) => false,
        }
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Status::Success => write!(f, "✅"),
            Status::Warning => write!(f, "⚠️"),
            Status::Mismatch(_) => write!(f, "🔄️"),
            Status::Error(_) => write!(f, "❌"),
            Status::Ignored(_) => write!(f, "👀"),
        }
    }
}

/// A mapping between [document identifiers](document::Identifier) and the
/// [status](Status) of their parsing test.
type Results = HashMap<document::Identifier, Status>;

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
    /// Creates a new [`Report`] that points to a [writer](std::io::Write).
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            section: Section::Title,
            results: Default::default(),
            printed: false,
        }
    }

    /// Gets the [`Results`] for this [`Report`] by reference.
    ///
    /// **Note:** see the note at [Self::results] for a caveat on interpretation
    /// of these results.
    pub fn results(&self) -> &Results {
        &self.results
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

                Section::Errors
            }
            Section::Errors => Section::Footer,
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
    ) -> std::io::Result<()> {
        if self.section != Section::Summary {
            panic!(
                "cannot register a status when the report phase is {:?}",
                self.section
            );
        }

        writeln!(self.inner, "{} {}", status, identifier)?;
        self.results.insert(identifier, status);
        self.printed = true;

        Ok(())
    }

    /// Report a warning for a registered result.
    pub fn report_warning(&mut self, warning: &lint::Warning) -> std::io::Result<()> {
        if self.section != Section::Summary {
            panic!(
                "cannot report a warning when the report phase is {:?}",
                self.section
            );
        }

        writeln!(self.inner, "  ↳ {}", warning)?;
        self.printed = true;

        Ok(())
    }

    /// Reports all unexpected errors for a repository report.
    pub fn report_unexpected_errors_for_repository(
        &mut self,
        repository_identifier: &repository::Identifier,
    ) -> std::io::Result<()> {
        if self.section != Section::Errors {
            panic!(
                "cannot report unexpected errors when the report phase is {:?}",
                self.section
            );
        }

        for (id, message) in self
            .results
            .iter()
            .filter(|(id, _)| id.repository() == repository_identifier)
            .filter_map(|(id, status)| match status {
                Status::Error(msg) => Some((id, msg)),
                _ => None,
            })
        {
            writeln!(self.inner, "{}\n\n{}\n", id.to_string().italic(), message)?;
            self.printed = true;
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

        let results = self
            .results
            .iter()
            .filter(|(identifer, _)| identifer.repository() == repository_identifier)
            .fold(IndexMap::<Status, usize>::new(), |mut hm, (_, status)| {
                *hm.entry(status.clone()).or_default() += 1;
                hm
            });

        let passed = results.iter().filter(|(status, _)| status.success()).fold(
            0usize,
            |mut acc, (_, count)| {
                acc += count;
                acc
            },
        );

        let considered = results
            .iter()
            .filter(|(status, _)| status.considered())
            .fold(0usize, |mut acc, (_, count)| {
                acc += count;
                acc
            });

        write!(self.inner, "Passed {}/{} tests", passed, considered)?;

        let mut with = Vec::new();

        match results
            .iter()
            .flat_map(|(status, count)| match status {
                Status::Ignored(_) => Some(*count),
                _ => None,
            })
            .sum::<usize>()
        {
            0 => {}
            1 => with.push(String::from("1 ignored error")),
            v => with.push(format!("{} ignored errors", v)),
        };

        match results
            .iter()
            .flat_map(|(status, count)| match status {
                Status::Mismatch(_) => Some(*count),
                _ => None,
            })
            .sum::<usize>()
        {
            0 => {}
            1 => with.push(String::from("1 mismatch error")),
            v => with.push(format!("{} mismatch errors", v)),
        };

        match results.get(&Status::Warning).copied() {
            Some(1) => with.push(String::from("1 test containing warnings")),
            Some(v) => with.push(format!("{} tests containing warnings", v)),
            None => {}
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
