//! Reporting.

use std::collections::HashMap;

use colored::Colorize as _;
use indexmap::IndexMap;

use crate::config::ReportableConcern;
use crate::document;
use crate::repository;

/// The status of a single parsing test.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Status {
    /// The document passed parsing, validation, and linting successfully with
    /// no concerns for both the parse tree and the abstract syntax tree.
    Success,

    /// The document had concerns, but the concerns exactly matched what was
    /// already expected in the configuration, meaning that this [`Status`] is
    /// considered successful.
    ConcernsMatched,

    /// The document had concerns, and some concerns were missing from what was
    /// expected in the configuration. This [`Status`] is considered a test
    /// failure.
    UnexpectedConcerns(Vec<ReportableConcern>),

    /// The document had concerns, and all of those concerns were accounted for
    /// in the configuration. However, there were some concerns declared in the
    /// configuration that were not found. This [`Status`] is considered a test
    /// failure.
    ///
    /// Note: this represents a situation where a concern was removed from the
    /// source document. In this case, the concern likely needs to be removed
    /// from the expected concerns.
    MissingExpectedConcerns(Vec<ReportableConcern>),
}

/// A printable section within a report for a repository.
#[derive(Debug, Eq, PartialEq)]
pub enum Section {
    /// Title of the section.
    Title,

    /// Summarized status of each parsing test.
    Summary,

    /// Detailed information on unexpected concerns that were encountered.
    Concerns,

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
            Status::ConcernsMatched => true,
            Status::UnexpectedConcerns(_) => false,
            Status::MissingExpectedConcerns(_) => false,
        }
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Status::Success => write!(f, "✅"),
            Status::ConcernsMatched => write!(f, "📘"),
            Status::UnexpectedConcerns(_) => write!(f, "❌"),
            Status::MissingExpectedConcerns(_) => write!(f, "🔄️"),
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

                Section::Concerns
            }
            Section::Concerns => Section::Footer,
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

    /// Reports all unexpected errors within a repository report.
    pub fn report_unexpected_errors(
        &mut self,
        repository_identifier: &repository::Identifier,
    ) -> std::io::Result<()> {
        if self.section != Section::Concerns {
            panic!(
                "cannot report unexpected concerns when the report phase is {:?}",
                self.section
            );
        }

        for (id, concerns) in self
            .results
            .iter()
            .filter(|(id, _)| id.repository() == repository_identifier)
            .filter_map(|(id, status)| match status {
                Status::MissingExpectedConcerns(concerns) => Some((id.to_string(), concerns)),
                Status::UnexpectedConcerns(concerns) => Some((id.to_string(), concerns)),
                _ => None,
            })
        {
            for concern in concerns {
                writeln!(self.inner, "{}\n\n{}\n", id.italic(), concern)?;
                self.printed = true;
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

        let test_results = self
            .results
            .iter()
            .filter(|(identifer, _)| identifer.repository() == repository_identifier)
            .fold(IndexMap::<Status, usize>::new(), |mut hm, (_, status)| {
                *hm.entry(status.clone()).or_default() += 1;
                hm
            });

        let passed = test_results
            .iter()
            .filter(|(status, _)| status.success())
            .fold(0usize, |mut acc, (_, count)| {
                acc += count;
                acc
            });

        let considered = test_results.iter().fold(0usize, |mut acc, (_, count)| {
            acc += count;
            acc
        });

        write!(self.inner, "Passed {}/{} tests", passed, considered)?;

        let mut with = Vec::new();

        match self
            .results
            .iter()
            .flat_map(|(_, status)| match status {
                Status::MissingExpectedConcerns(concerns) => Some(concerns.len()),
                _ => None,
            })
            .sum::<usize>()
        {
            0 => {}
            1 => with.push(String::from("1 missing concern")),
            v => with.push(format!("{} missing concerns", v)),
        };

        match self
            .results
            .iter()
            .flat_map(|(_, status)| match status {
                Status::UnexpectedConcerns(concerns) => Some(concerns.len()),
                _ => None,
            })
            .sum::<usize>()
        {
            0 => {}
            1 => with.push(String::from("1 unexpected concern")),
            v => with.push(format!("{} unexpected concerns", v)),
        };

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
