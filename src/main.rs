//! The Sprocket command line tool.

use std::io::IsTerminal;
use std::io::stderr;

use clap::Parser;
use clap_verbosity_flag::Verbosity;
use colored::Colorize;
use git_testament::git_testament;
use git_testament::render_testament;
use sprocket::commands;
use sprocket::config::Config;
use tracing_log::AsTrace;

use crate::commands::Commands;

git_testament!(TESTAMENT);

#[derive(Parser, Debug)]
#[command(author, version = render_testament!(TESTAMENT), propagate_version = true, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[command(flatten)]
    verbose: Verbosity,

    /// Path to the configuration file.
    #[arg(long, short)]
    config: Option<String>,
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

    let config = Config::new(cli.config);

    // Write effective configuration to the log
    tracing::debug!(
        "effective configuration:\n{}",
        toml::to_string_pretty(&config).unwrap_or_default()
    );

    match cli.command {
        Commands::Check(args) => commands::check::check(args.apply(config)).await,
        Commands::Lint(args) => commands::check::lint(args.apply(config)).await,
        Commands::Explain(args) => commands::explain::explain(args),
        Commands::Analyzer(args) => commands::analyzer::analyzer(args).await,
        Commands::Format(args) => commands::format::format(args.apply(config)),
        Commands::ValidateInputs(args) => {
            commands::validate::validate_inputs(args.apply(config)).await
        }
        Commands::Config(args) => commands::config::config(args, config),
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
