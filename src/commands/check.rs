use std::path::PathBuf;

use clap::Parser;
use clap::ValueEnum;
use codespan_reporting::term::termcolor::ColorChoice;
use codespan_reporting::term::termcolor::StandardStream;
use codespan_reporting::term::Config;
use codespan_reporting::term::DisplayStyle;

#[derive(Clone, Debug, Default, ValueEnum)]
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

/// Arguments for the `check` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// The files or directories to check.
    #[arg(required = true)]
    paths: Vec<PathBuf>,

    /// Perform lint checks in addition to validation.
    #[arg(short, long)]
    lint: bool,

    /// The extensions to collect when expanding a directory.
    #[arg(short, long, default_value = "wdl")]
    extensions: Vec<String>,

    /// Disables color output.
    #[arg(long)]
    no_color: bool,

    /// The report mode.
    #[arg(short = 'm', long, default_value_t, value_name = "MODE")]
    report_mode: Mode,
}

pub fn check(args: Args) -> anyhow::Result<()> {
    let (config, writer) = get_display_config(&args);

    match sprocket::file::Repository::try_new(args.paths, args.extensions)?
        .report_diagnostics(config, writer, false)?
    {
        // There are parse errors or validation failures.
        (true, _) => std::process::exit(1),
        // There are no diagnostics.
        (false, _) => {}
    }

    Ok(())
}

fn get_display_config(args: &Args) -> (Config, StandardStream) {
    let display_style = match args.report_mode {
        Mode::Full => DisplayStyle::Rich,
        Mode::OneLine => DisplayStyle::Short,
    };

    let config = Config {
        display_style,
        ..Default::default()
    };

    let color_choice = if args.no_color {
        ColorChoice::Never
    } else {
        ColorChoice::Always
    };

    let writer = StandardStream::stderr(color_choice);

    (config, writer)
}
