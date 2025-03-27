//! The Sprocket command line tool.

use std::io::IsTerminal;
use std::io::stderr;

use clap::Parser;
use clap::Subcommand;
use clap_verbosity_flag::Verbosity;
use colored::Colorize;
use git_testament::git_testament;
use git_testament::render_testament;
use sprocket::commands;
use tracing_log::AsTrace;

git_testament!(TESTAMENT);

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum Commands {
    /// Checks a WDL document (or a directory containing WDL documents) and
    /// reports diagnostics.
    Check(commands::check::CheckArgs),

    /// Lints Workflow Description Language files.
    Lint(commands::check::LintArgs),

    /// Explains a rule.
    Explain(commands::explain::Args),

    /// Runs the analyzer LSP server.
    Analyzer(commands::analyzer::AnalyzerArgs),

    /// Formats a WDL document.
    #[clap(alias = "fmt")]
    Format(commands::format::FormatArgs),

    /// Validates an input file (JSON or YAML) against a task or workflow input
    /// schema.
    ///
    /// This ensures that every required input is supplied, every supplied input
    /// is correctly typed, that no extraneous inputs are provided, and that any
    /// provided `File` or `Directory` inputs exist.
    ///
    /// It will not catch potential runtime errors that
    /// may occur when running the task or workflow.
    ValidateInputs(commands::validate::ValidateInputsArgs),
}

#[derive(Parser)]
#[command(author, version = render_testament!(TESTAMENT), propagate_version = true, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[command(flatten)]
    verbose: Verbosity,
}

pub async fn inner() -> anyhow::Result<()> {
    let cli = Cli::parse();

    tracing_log::LogTracer::init()?;

    let subscriber = tracing_subscriber::fmt::Subscriber::builder()
        .with_max_level(cli.verbose.log_level_filter().as_trace())
        .with_writer(std::io::stderr)
        .with_ansi(stderr().is_terminal())
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    match cli.command {
        Commands::Check(args) => commands::check::check(args).await,
        Commands::Lint(args) => commands::check::lint(args).await,
        Commands::Explain(args) => commands::explain::explain(args),
        Commands::Analyzer(args) => commands::analyzer::analyzer(args).await,
        Commands::Format(args) => commands::format::format(args),
        Commands::ValidateInputs(args) => commands::validate::validate_inputs(args).await,
    }
}

#[tokio::main]
pub async fn main() {
    if let Err(e) = inner().await {
        eprintln!(
            "{error}: {e:?}",
            error = if std::io::stderr().is_terminal() {
                "error".red().bold()
            } else {
                "error".normal()
            }
        );
        std::process::exit(1);
    }
}
