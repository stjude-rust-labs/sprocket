//! The Sprocket command line tool.

use clap::Parser;
use clap::Subcommand;
use git_testament::git_testament;
use git_testament::render_testament;

pub mod commands;

git_testament!(TESTAMENT);

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum Commands {
    /// Lints Workflow Description Language files.
    Lint(commands::lint::Args),
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

pub fn inner() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let level = if cli.verbose {
        tracing::Level::DEBUG
    } else if cli.quiet {
        tracing::Level::ERROR
    } else {
        tracing::Level::INFO
    };

    let subscriber = tracing_subscriber::fmt::Subscriber::builder()
        .with_max_level(level)
        .with_writer(std::io::stderr)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    match cli.command {
        Commands::Lint(args) => commands::lint::lint(args),
    }
}

pub fn main() {
    if let Err(err) = inner() {
        eprintln!("error: {}", err);
    }
}
