use std::path::PathBuf;

use anyhow::bail;
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

/// Common arguments for the `check` and `lint` subcommands.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Common {
    /// The files or directories to check.
    #[arg(required = true)]
    paths: Vec<PathBuf>,

    /// Lint rules to except from running.
    #[arg(short, long, value_name = "RULE")]
    except: Vec<String>,

    /// Disables color output.
    #[arg(long)]
    no_color: bool,

    /// The report mode.
    #[arg(short = 'm', long, default_value_t, value_name = "MODE")]
    report_mode: Mode,
}

/// Arguments for the `check` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct CheckArgs {
    #[command(flatten)]
    common: Common,

    /// Perform lint checks in addition to syntax validation.
    #[arg(short, long)]
    lint: bool,
}

/// Arguments for the `lint` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct LintArgs {
    #[command(flatten)]
    common: Common,
}

pub fn check(args: CheckArgs) -> anyhow::Result<()> {
    if !args.lint && !args.common.except.is_empty() {
        bail!("cannot specify --except without --lint");
    }

    let (config, writer) = get_display_config(&args.common);

    match sprocket::file::Repository::try_new(args.common.paths, vec!["wdl".to_string()])?
        .report_diagnostics(config, writer, args.lint, args.common.except)?
    {
        // There are syntax errors.
        (true, _) => std::process::exit(1),
        // There are lint failures.
        (false, true) => std::process::exit(2),
        // There are no diagnostics.
        (false, false) => {}
    }

    Ok(())
}

pub fn lint(args: LintArgs) -> anyhow::Result<()> {
    check(CheckArgs {
        common: args.common,
        lint: true,
    })
}

fn get_display_config(args: &Common) -> (Config, StandardStream) {
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
