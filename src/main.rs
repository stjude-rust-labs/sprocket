//! The Sprocket command line tool.

use std::env;
use std::io::IsTerminal;
use std::io::stderr;
use std::path::Path;

use clap::Parser;
use clap_verbosity_flag::Verbosity;
use colored::Colorize;
use dirs::home_dir;
use figment::Figment;
use figment::providers::Format;
use figment::providers::Serialized;
use figment::providers::Toml;
use git_testament::git_testament;
use git_testament::render_testament;
use sprocket::commands;
use sprocket::config;
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
    let mut cli = Cli::parse();

    tracing_log::LogTracer::init()?;

    let subscriber = tracing_subscriber::fmt::Subscriber::builder()
        .with_max_level(cli.verbose.log_level_filter().as_trace())
        .with_writer(std::io::stderr)
        .with_ansi(stderr().is_terminal())
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Start a new Figment instance with default values
    let mut figment =
        Figment::new().admerge(Serialized::from(config::Config::default(), "default"));

    // Check XDG_CONFIG_HOME for a config file
    // If XDG_CONFIG_HOME is not set, check HOME for a config file
    if let Ok(xdg_config_home) = env::var("XDG_CONFIG_HOME") {
        tracing::info!(
            "Reading configuration from XDG_CONFIG_HOME: {xdg_config_home:?}/sprocket.toml"
        );
        figment = figment.admerge(Toml::file(
            Path::new(&xdg_config_home.as_str()).join("sprocket.toml"),
        ));
    } else if let Some(home) = home_dir() {
        tracing::info!("Reading configuration from HOME: {home:?}/.config/sprocket.toml");
        figment = figment.admerge(Toml::file(home.join(".config").join("sprocket.toml")));
    }

    // Check PWD for a config file
    if Path::exists(Path::new("sprocket.toml")) {
        tracing::info!("Reading configuration from PWD/sprocket.toml");
        figment = figment.admerge(Toml::file("sprocket.toml"));
    }

    // If provided, check config file from environment
    if let Ok(config_file) = env::var("SPROCKET_CONFIG") {
        tracing::info!("Reading configuration from SPROCKET_CONFIG: {config_file:?}");
        figment = figment.admerge(Toml::file(config_file));
    }

    // If provided, check command line config file
    if let Some(ref cli) = cli.config {
        tracing::info!("Reading configuration from --config: {cli:?}");
        figment = figment.admerge(Toml::file(cli));
    }

    // Get the configuration from the Figment
    let config: Config = figment.extract().expect("Failed to extract config");

    // Write effective configuration to the log
    tracing::info!(
        "Effective configuration:\n{}",
        toml::to_string(&config).unwrap_or_default()
    );

    cli.command = config.merge_args(cli.command);

    match cli.command {
        Commands::Check(args) => commands::check::check(args).await,
        Commands::Lint(args) => commands::check::lint(args).await,
        Commands::Explain(args) => commands::explain::explain(args),
        Commands::Analyzer(args) => commands::analyzer::analyzer(args).await,
        Commands::Format(args) => commands::format::format(args),
        Commands::ValidateInputs(args) => commands::validate::validate_inputs(args).await,
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
