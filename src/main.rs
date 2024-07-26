//! The Sprocket command line tool.

use std::io::stderr;
use std::io::IsTerminal;

use clap::Parser;
use clap::Subcommand;
use git_testament::git_testament;
use git_testament::render_testament;

pub mod commands;

git_testament!(TESTAMENT);

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum Commands {
    /// Checks the syntactic validity of Workflow Description Language files.
    Check(commands::check::CheckArgs),

    /// Lints Workflow Description Language files.
    Lint(commands::check::LintArgs),

    /// Explains a lint warning.
    Explain(commands::explain::Args),

    /// Runs the analyzer LSP server.
    Analyzer(commands::analyzer::AnalyzerArgs),
}

#[derive(Parser)]
#[command(author, version = render_testament!(TESTAMENT), propagate_version = true, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Only errors are printed to the stderr stream.
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// All available information, including debug information, is printed to
    /// stderr.
    #[arg(short, long, global = true)]
    pub verbose: bool,
}

pub async fn inner() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let level = if cli.verbose {
        tracing::Level::DEBUG
    } else if cli.quiet {
        tracing::Level::ERROR
    } else {
        tracing::Level::INFO
    };

    tracing_log::LogTracer::init()?;

    let subscriber = tracing_subscriber::fmt::Subscriber::builder()
        .with_max_level(level)
        .with_writer(std::io::stderr)
        .with_ansi(stderr().is_terminal())
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    match cli.command {
        Commands::Check(args) => commands::check::check(args),
        Commands::Lint(args) => commands::check::lint(args),
        Commands::Explain(args) => commands::explain::explain(args),
        Commands::Analyzer(args) => commands::analyzer::analyzer(args).await,
    }
}

#[tokio::main]
pub async fn main() {
    if let Err(err) = inner().await {
        eprintln!("error: {}", err);
    }
}
