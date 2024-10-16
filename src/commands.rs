//! Implementation of sprocket CLI commands.

use clap::ValueEnum;
use codespan_reporting::term::Config;
use codespan_reporting::term::DisplayStyle;
use codespan_reporting::term::termcolor::ColorChoice;
use codespan_reporting::term::termcolor::StandardStream;

pub mod analyzer;
pub mod check;
pub mod explain;
pub mod format;

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
fn get_display_config(report_mode: Mode, no_color: bool) -> (Config, StandardStream) {
    let display_style = match report_mode {
        Mode::Full => DisplayStyle::Rich,
        Mode::OneLine => DisplayStyle::Short,
    };

    let config = Config {
        display_style,
        ..Default::default()
    };

    let color_choice = if no_color {
        ColorChoice::Never
    } else {
        ColorChoice::Always
    };

    let writer = StandardStream::stderr(color_choice);

    (config, writer)
}
