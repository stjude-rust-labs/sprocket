//! The Sprocket command line tool.

use std::io::IsTerminal;
use std::io::stderr;

use clap::CommandFactory;
use clap::Parser;
use clap::Subcommand;
use clap_verbosity_flag::Verbosity;
use clap_verbosity_flag::WarnLevel;
use colored::Colorize;
use git_testament::git_testament;
use git_testament::render_testament;
use sprocket::commands;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt as _;

git_testament!(TESTAMENT);

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum Commands {
    /// Runs the Language Server Protocol (LSP) server.
    Analyzer(commands::analyzer::Args),

    /// Checks a document or a directory containing documents.
    Check(commands::check::CheckArgs),

    /// Explains linting and validation rules.
    Explain(commands::explain::Args),

    /// Formats a document.
    #[clap(alias = "fmt")]
    Format(commands::format::Args),

    /// Writes the input schema for a WDL document.
    Input(commands::input::Args),

    /// Lints a document or a directory containing documents.
    Lint(commands::check::LintArgs),

    /// Runs a task or workflow.
    Run(commands::run::Args),

    /// Validate a set of inputs against a task or workflow.
    ///
    /// This ensures that every required input is supplied, every supplied input
    /// is correctly typed, that no extraneous inputs are provided, and that any
    /// provided `File` or `Directory` inputs exist.
    ///
    /// It will not catch potential runtime errors that may occur when running
    /// the task or workflow.
    Validate(commands::validate::Args),

    /// Generates shell completions.
    Completions(commands::completions::Args),
}

#[derive(Parser)]
#[command(author, version = render_testament!(TESTAMENT), propagate_version = true, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[command(flatten)]
    verbosity: Verbosity<WarnLevel>,
}

pub async fn inner() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match std::env::var("RUST_LOG") {
        Ok(_) => {
            let indicatif_layer = tracing_indicatif::IndicatifLayer::new();

            let subscriber = tracing_subscriber::fmt::Subscriber::builder()
                .with_env_filter(EnvFilter::from_default_env())
                .with_writer(indicatif_layer.get_stderr_writer())
                .with_ansi(stderr().is_terminal())
                .finish()
                .with(indicatif_layer);

            tracing::subscriber::set_global_default(subscriber)?;
        }
        Err(_) => {
            let indicatif_layer = tracing_indicatif::IndicatifLayer::new();

            let subscriber = tracing_subscriber::fmt()
                .with_max_level(cli.verbosity)
                .with_writer(indicatif_layer.get_stderr_writer())
                .with_ansi(stderr().is_terminal())
                .finish()
                .with(indicatif_layer);

            tracing::subscriber::set_global_default(subscriber)?;
        }
    };

    match cli.command {
        Commands::Analyzer(args) => commands::analyzer::analyzer(args).await,
        Commands::Check(args) => commands::check::check(args).await,
        Commands::Explain(args) => commands::explain::explain(args),
        Commands::Format(args) => commands::format::format(args),
        Commands::Input(args) => commands::input::input(args).await,
        Commands::Lint(args) => commands::check::lint(args).await,
        Commands::Run(args) => commands::run::run(args).await,
        Commands::Validate(args) => commands::validate::validate(args).await,
        Commands::Completions(args) => {
            let mut cmd = Cli::command();
            commands::completions::completions(args, &mut cmd).await
        }
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
