//! The Sprocket command line tool.

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::missing_docs_in_private_items)]
#![warn(rustdoc::broken_intra_doc_links)]

use std::borrow::Cow;
use std::collections::HashMap;
use std::io::IsTerminal as _;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::LazyLock;

use anyhow::Context as _;
use anyhow::Result;
use clap::ValueEnum;
use codespan_reporting::diagnostic::Label;
use codespan_reporting::diagnostic::LabelStyle;
use codespan_reporting::files::SimpleFiles;
use codespan_reporting::term::Config;
use codespan_reporting::term::DisplayStyle;
use codespan_reporting::term::emit;
use codespan_reporting::term::termcolor::ColorChoice;
use codespan_reporting::term::termcolor::StandardStream;
use serde::Deserialize;
use serde::Serialize;
use wdl::ast::AstNode as _;
use wdl::ast::Diagnostic;
use wdl::cli::analysis::Source;
use wdl::engine::CallLocation;

pub mod commands;
pub mod config;

/// The maximum number of call locations to print for evaluation errors.
const MAX_CALL_LOCATIONS: usize = 10;

/// Configuration for full display style.
static FULL_CONFIG: LazyLock<Config> = LazyLock::new(|| Config {
    display_style: DisplayStyle::Rich,
    ..Default::default()
});

/// Configuration for one-line display style.
static ONE_LINE_CONFIG: LazyLock<Config> = LazyLock::new(|| Config {
    display_style: DisplayStyle::Short,
    ..Default::default()
});

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

/// Gets the display configuration based on the user's preferences.
fn get_display_config(report_mode: Mode, no_color: bool) -> (&'static Config, StandardStream) {
    let config = match report_mode {
        Mode::Full => &FULL_CONFIG,
        Mode::OneLine => &ONE_LINE_CONFIG,
    };

    let color_choice = if no_color {
        ColorChoice::Never
    } else if std::io::stderr().is_terminal() {
        ColorChoice::Always
    } else {
        ColorChoice::Never
    };

    let stream = StandardStream::stderr(color_choice);

    (config, stream)
}

/// Emits the given diagnostics to the terminal.
fn emit_diagnostics<'a>(
    path: &str,
    source: String,
    diagnostics: impl IntoIterator<Item = &'a Diagnostic>,
    backtrace: &[CallLocation],
    report_mode: Mode,
    no_color: bool,
) -> Result<()> {
    let mut map = HashMap::new();
    let mut files = SimpleFiles::new();

    let file_id = files.add(Cow::Borrowed(path), source);

    let (config, mut stream) = get_display_config(report_mode, no_color);

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

        emit(&mut stream, config, &files, &diagnostic).context("failed to emit diagnostic")?;

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

/// Returns a vector containing a single source for the current working
/// directory.
fn cwd_source() -> Vec<Source> {
    vec![Source::Directory(PathBuf::from(
        std::path::Component::CurDir.as_os_str(),
    ))]
}
