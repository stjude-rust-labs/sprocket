//! The Sprocket command line tool.

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::missing_docs_in_private_items)]
#![warn(rustdoc::broken_intra_doc_links)]

use clap::ValueEnum;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::Config;
use codespan_reporting::term::DisplayStyle;
use codespan_reporting::term::emit;
use codespan_reporting::term::termcolor::ColorChoice;
use codespan_reporting::term::termcolor::StandardStream;
use std::sync::LazyLock;
use wdl::ast::Diagnostic;

pub mod commands;

static FULL_CONFIG: LazyLock<Config> = LazyLock::new(|| Config {
    display_style: DisplayStyle::Rich,
    ..Default::default()
});

static ONE_LINE_CONFIG: LazyLock<Config> = LazyLock::new(|| Config {
    display_style: DisplayStyle::Short,
    ..Default::default()
});

/// The diagnostic mode to use for reporting diagnostics.
#[derive(Clone, Copy, Debug, Default, ValueEnum, PartialEq, Eq)]
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

/// Gets the display config to use for reporting diagnostics.
fn get_display_config(report_mode: Mode, no_color: bool) -> (&'static Config, StandardStream) {
    let config = match report_mode {
        Mode::Full => &FULL_CONFIG,
        Mode::OneLine => &ONE_LINE_CONFIG,
    };

    let color_choice = if no_color {
        ColorChoice::Never
    } else {
        ColorChoice::Always
    };

    let writer = StandardStream::stderr(color_choice);

    (config, writer)
}

/// Emits the given diagnostics to the terminal.
fn emit_diagnostics<'a>(
    diagnostics: impl IntoIterator<Item = &'a Diagnostic>,
    file_name: &str,
    source: &str,
    report_mode: Mode,
    no_color: bool,
) {
    let file = SimpleFile::new(file_name, source);

    let (config, writer) = get_display_config(report_mode, no_color);
    let mut writer = writer.lock();
    for diagnostic in diagnostics {
        emit(&mut writer, &config, &file, &diagnostic.to_codespan()).unwrap();
    }
}
