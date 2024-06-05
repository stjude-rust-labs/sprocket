use std::path::PathBuf;

use clap::Parser;
use clap::ValueEnum;
use codespan_reporting::term::termcolor::ColorChoice;
use codespan_reporting::term::termcolor::StandardStream;
use codespan_reporting::term::Config;
use codespan_reporting::term::DisplayStyle;

#[derive(Clone, Debug, Default, ValueEnum)]
pub enum Mode {
    /// Prints concerns as multiple lines.
    #[default]
    Full,

    /// Prints concerns as one line.
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

/// Arguments for the `lint` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// The files or directories to lint.
    #[arg(required = true)]
    paths: Vec<PathBuf>,

    /// The extensions to collect when expanding a directory.
    #[arg(short, long, default_value = "wdl")]
    extensions: Vec<String>,

    /// Disables color output.
    #[arg(long)]
    no_color: bool,

    /// The report mode.
    #[arg(short = 'm', long, default_value_t, value_name = "MODE")]
    report_mode: Mode,

    /// The specification version.
    #[arg(short, long, default_value_t, value_enum, value_name = "VERSION")]
    specification_version: wdl::core::Version,
}

pub fn lint(args: Args) -> anyhow::Result<()> {
    let (config, writer) = get_display_config(&args);

    match sprocket::file::Repository::try_new(args.paths, args.extensions)?
        .report_concerns(config, writer)?
    {
        // There are parse errors or validation failures.
        (true, _) => std::process::exit(1),
        // There are no parse errors or validation failures, but there are lint warnings.
        (false, true) => std::process::exit(2),
        // There are no concerns.
        _ => {}
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
