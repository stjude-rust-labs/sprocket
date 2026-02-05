//! Utilities for reporting diagnostics to the terminal.

use std::io::IsTerminal as _;
use std::io::Write as _;
use std::sync::LazyLock;

use anyhow::Context as _;
use anyhow::anyhow;
use clap::ValueEnum;
use codespan_reporting::diagnostic::Label;
use codespan_reporting::diagnostic::LabelStyle;
use codespan_reporting::files::SimpleFiles;
use codespan_reporting::term::Config as TermConfig;
use codespan_reporting::term::DisplayStyle;
use codespan_reporting::term::emit_to_write_style;
use codespan_reporting::term::termcolor::ColorChoice;
use codespan_reporting::term::termcolor::StandardStream;
use serde::Deserialize;
use serde::Serialize;
use wdl::ast::AstNode as _;
use wdl::ast::Diagnostic;
use wdl::engine::CallLocation;

/// The maximum number of call locations to print for evaluation errors.
const MAX_CALL_LOCATIONS: usize = 10;

/// Configuration for full display style.
static FULL_CONFIG: LazyLock<TermConfig> = LazyLock::new(|| TermConfig {
    display_style: DisplayStyle::Rich,
    ..Default::default()
});

/// Configuration for one-line display style.
static ONE_LINE_CONFIG: LazyLock<TermConfig> = LazyLock::new(|| TermConfig {
    display_style: DisplayStyle::Short,
    ..Default::default()
});

/// A counter tracking the types of diagnostics emitted during analysis.
#[derive(Default)]
pub struct DiagnosticCounts {
    /// The number of errors encountered.
    pub errors: usize,
    /// The number of warnings encountered.
    pub warnings: usize,
    /// The number of notes encountered.
    pub notes: usize,
}

impl DiagnosticCounts {
    /// Returns an error if the `errors` count is 1 or more
    pub fn verify_no_errors(&self) -> Option<anyhow::Error> {
        if self.errors == 0 {
            return None;
        }

        Some(anyhow!(
            "failing due to {errors} error{s}",
            errors = self.errors,
            s = if self.errors == 1 { "" } else { "s" }
        ))
    }

    /// Returns an error if the `warnings` count is 1 or more
    pub fn verify_no_warnings(&self, user_requested: bool) -> Option<anyhow::Error> {
        if self.warnings == 0 {
            return None;
        }

        Some(anyhow!(
            "failing due to {warnings} warning{s}{cli_note}",
            warnings = self.warnings,
            s = if self.warnings == 1 { "" } else { "s" },
            cli_note = if user_requested {
                " (`--deny-warnings` was specified)"
            } else {
                ""
            },
        ))
    }

    /// Returns an error if the `notes` count is 1 or more
    pub fn verify_no_notes(&self, user_requested: bool) -> Option<anyhow::Error> {
        if self.notes == 0 {
            return None;
        }

        Some(anyhow!(
            "failing due to {notes} note{s}{cli_note}",
            notes = self.notes,
            s = if self.notes == 1 { "" } else { "s" },
            cli_note = if user_requested {
                " (`--deny-notes` was specified)"
            } else {
                ""
            },
        ))
    }
}

/// The diagnostic mode to use for reporting diagnostics.
#[derive(Clone, Copy, Debug, Default, ValueEnum, PartialEq, Eq, Deserialize, Serialize)]
pub enum Mode {
    /// Prints diagnostics as multiple lines.
    #[default]
    Full,

    /// Prints diagnostics as one line.
    OneLine,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Full => write!(f, "full"),
            Mode::OneLine => write!(f, "one-line"),
        }
    }
}

/// Gets the diagnostics display configuration based on the user's preferences.
pub fn get_diagnostics_display_config(
    report_mode: Mode,
    no_color: bool,
) -> (&'static TermConfig, StandardStream) {
    let config = match report_mode {
        Mode::Full => &FULL_CONFIG,
        Mode::OneLine => &ONE_LINE_CONFIG,
    };

    let color_choice = if no_color {
        ColorChoice::Never
    } else if std::io::stderr().is_terminal() {
        ColorChoice::Auto
    } else {
        ColorChoice::Never
    };

    let stream = StandardStream::stderr(color_choice);

    (config, stream)
}

/// Emits the given diagnostics to the terminal.
pub fn emit_diagnostics<'a>(
    path: &str,
    source: String,
    diagnostics: impl IntoIterator<Item = &'a Diagnostic>,
    backtrace: &[CallLocation],
    report_mode: Mode,
    no_color: bool,
) -> anyhow::Result<()> {
    let mut map = std::collections::HashMap::new();
    let mut files = SimpleFiles::new();

    let file_id = files.add(std::borrow::Cow::Borrowed(path), source);

    let (config, mut stream) = get_diagnostics_display_config(report_mode, no_color);

    for diagnostic in diagnostics {
        let diagnostic = diagnostic.to_codespan(file_id).with_labels_iter(
            backtrace.iter().take(MAX_CALL_LOCATIONS).map(|l| {
                let id = l.document.id();
                let file_id = *map.entry(id).or_insert_with(|| {
                    files.add(l.document.path(), l.document.root().text().to_string())
                });

                Label {
                    style: LabelStyle::Secondary,
                    file_id,
                    range: l.span.start()..l.span.end(),
                    message: "called from this location".into(),
                }
            }),
        );

        emit_to_write_style(&mut stream, config, &files, &diagnostic)
            .context("failed to emit diagnostic")?;

        if backtrace.len() > MAX_CALL_LOCATIONS {
            writeln!(
                &mut stream,
                "  and {count} more call{s}...",
                count = backtrace.len() - MAX_CALL_LOCATIONS,
                s = if backtrace.len() - MAX_CALL_LOCATIONS == 1 {
                    ""
                } else {
                    "s"
                }
            )
            .unwrap();
        }
    }

    Ok(())
}
