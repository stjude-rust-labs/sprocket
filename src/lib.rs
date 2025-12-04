//! [The Sprocket command line tool](https://sprocket.bio/).
//!
//! This library crate only exports the items necessary to build the `sprocket`
//! binary crate and associated integration tests. It is not meant to be used by
//! any other crates.

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::missing_docs_in_private_items)]
#![warn(rustdoc::broken_intra_doc_links)]

use std::io::IsTerminal as _;
use std::io::stderr;
use std::path::PathBuf;

use anyhow::Context as _;
use clap::CommandFactory as _;
use clap::Parser as _;
use clap_verbosity_flag::Verbosity;
use clap_verbosity_flag::WarnLevel;
use commands::Commands;
pub use config::Config;
use git_testament::git_testament;
use git_testament::render_testament;
use tracing::trace;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt as _;

use crate::commands::CommandResult;

// Access to these modules is useful for integration testing and benchmarking,
// but since this is not intended to be used as a public interface, we hide them
// from generated rustdoc.
#[doc(hidden)]
pub mod analysis;
#[doc(hidden)]
pub mod commands;
mod config;
mod diagnostics;
mod eval;
mod inputs;

/// ignorefile basename to respect.
const IGNORE_FILENAME: &str = ".sprocketignore";

git_testament!(TESTAMENT);

#[derive(clap::Parser, Debug)]
#[command(author, version = render_testament!(TESTAMENT), propagate_version = true, about, long_about = None)]
struct Cli {
    /// The command to execute.
    #[command(subcommand)]
    pub command: Commands,

    /// The verbosity for log messages.
    #[command(flatten)]
    verbosity: Verbosity<WarnLevel>,

    /// Path to the configuration file.
    #[arg(long, short, global = true)]
    config: Vec<PathBuf>,

    /// Skip searching for and loading configuration files.
    ///
    /// Only a configuration file specified as a command line argument will be
    /// used.
    #[arg(long, short, global = true)]
    skip_config_search: bool,
}

async fn inner() -> CommandResult<()> {
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

            tracing::subscriber::set_global_default(subscriber)
                .context("failed to set tracing subscriber")?;
        }
        Err(_) => {
            let indicatif_layer = tracing_indicatif::IndicatifLayer::new();

            let subscriber = tracing_subscriber::fmt()
                .with_max_level(cli.verbosity)
                .with_writer(indicatif_layer.get_stderr_writer())
                .with_ansi(stderr().is_terminal())
                .finish()
                .with(indicatif_layer);

            tracing::subscriber::set_global_default(subscriber)
                .context("failed to set tracing subscriber")?;
        }
    };

    let config = match &cli.command {
        Commands::Config(config_args) if config_args.is_init() => {
            // For `config init`, skip loading and use default
            Config::default()
        }
        _ => {
            // For all other commands, load config normally
            let config = Config::new(
                cli.config.iter().map(PathBuf::as_path),
                cli.skip_config_search,
            )?;
            config
                .validate()
                .with_context(|| "validating provided configuration")?;
            config
        }
    };
    // Write effective configuration to the log
    trace!(
        "effective configuration:\n{}",
        toml::to_string_pretty(&config).unwrap_or_default()
    );

    match cli.command {
        Commands::Analyzer(args) => commands::analyzer::analyzer(args.apply(config)).await,
        Commands::Check(args) => commands::check::check(args.apply(config)).await,
        Commands::Completions(args) => {
            let mut cmd = Cli::command();
            commands::completions::completions(args, &mut cmd).await
        }
        Commands::Config(args) => commands::config::config(args, config),
        Commands::Explain(args) => commands::explain::explain(args),
        Commands::Format(args) => commands::format::format(args.apply(config)).await,
        Commands::Inputs(args) => commands::inputs::inputs(args).await,
        Commands::Lint(args) => commands::check::lint(args.apply(config)).await,
        Commands::Run(args) => commands::run::run(args.apply(config)).await,
        Commands::Validate(args) => commands::validate::validate(args.apply(config)).await,
        Commands::Dev(commands::DevCommands::Doc(args)) => commands::doc::doc(args).await,
        Commands::Dev(commands::DevCommands::Lock(args)) => commands::lock::lock(args).await,
    }
}

/// The Sprocket command line entrypoint.
pub async fn sprocket_main<Guard>(guard: Guard) {
    if let Err(e) = inner().await {
        drop(guard);
        eprintln!("{e}");
        std::process::exit(1);
    }
}
