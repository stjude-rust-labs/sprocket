//! The Sprocket command line tool.

use std::io::IsTerminal;
use std::io::stderr;

use clap::CommandFactory;
use clap::Parser;
use clap_verbosity_flag::Verbosity;
use clap_verbosity_flag::WarnLevel;
use colored::Colorize;
use git_testament::git_testament;
use git_testament::render_testament;
use sprocket::commands;
use sprocket::config::Config;
use tracing_log::AsTrace;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt as _;

use crate::commands::Commands;

git_testament!(TESTAMENT);
#[derive(Parser, Debug)]
#[command(author, version = render_testament!(TESTAMENT), propagate_version = true, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[command(flatten)]
    verbosity: Verbosity<WarnLevel>,

    /// Path to the configuration file.
    #[arg(long, short)]
    config: Option<String>,
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

    let config = Config::new(cli.config);

    // Write effective configuration to the log
    tracing::debug!(
        "effective configuration:\n{}",
        toml::to_string_pretty(&config).unwrap_or_default()
    );

    match cli.command {
        Commands::Analyzer(args) => commands::analyzer::analyzer(args).await,
        Commands::Check(args) => commands::check::check(args.apply(config)).await,
        Commands::Explain(args) => commands::explain::explain(args),
        Commands::Format(args) => commands::format::format(args.apply(config)),
        Commands::Lint(args) => commands::check::lint(args.apply(config)).await,
        Commands::Run(args) => commands::run::run(args).await,
        Commands::Validate(args) => commands::validate::validate(args.apply(config)).await,
        Commands::Completions(args) => {
            let mut cmd = Cli::command();
            commands::completions::completions(args, &mut cmd).await
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
