//! The Sprocket command line tool.

use std::env;
use std::io::IsTerminal;
use std::io::stderr;
use std::path::Path;

use clap::Parser;
use clap::Subcommand;
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

    /// Validates an input JSON or YAML file against a task or workflow input
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

    /// Path to the configuration file.
    #[arg(long, short)]
    config: Option<String>,
}

pub async fn inner() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Start a new Figment instance with default values
    let mut figment =
        Figment::new().admerge(Serialized::from(config::Config::default(), "default"));

    // If provided, check config file from environment
    if let Ok(config_file) = env::var("SPROCKET_CONFIG") {
        figment = figment.admerge(Toml::file(config_file));
    }

    // Check XDG_CONFIG_HOME for a config file
    if let Ok(xdg_config_home) = env::var("XDG_CONFIG_HOME") {
        figment = figment.admerge(Toml::file(
            Path::new(&xdg_config_home.as_str()).join("sprocket.toml"),
        ));
    }

    // Check HOME for a config file
    if let Some(home) = home_dir() {
        figment = figment.admerge(Toml::file(home.join(".config").join("sprocket.toml")));
    }

    // Check PWD for a config file
    figment = figment.admerge(Toml::file("sprocket.toml"));

    // If provided, check command line config file
    if let Some(cli) = cli.config {
        figment = figment.admerge(Toml::file(cli));
    }

    // Get the configuration from the Figment
    let config: Config = figment.extract().expect("Failed to extract config");

    tracing_log::LogTracer::init()?;

    let subscriber = tracing_subscriber::fmt::Subscriber::builder()
        .with_max_level(cli.verbose.log_level_filter().as_trace())
        .with_writer(std::io::stderr)
        .with_ansi(stderr().is_terminal())
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Write effective configuration to the log
    tracing::info!("Effective configuration: {config:?}");

    match cli.command {
        Commands::Check(args) => commands::check::check(args, config.check_config).await,
        Commands::Lint(args) => commands::check::lint(args, config.check_config).await,
        Commands::Explain(args) => commands::explain::explain(args),
        Commands::Analyzer(args) => commands::analyzer::analyzer(args).await,
        Commands::Format(args) => commands::format::format(args, config.format_config),
        Commands::ValidateInputs(args) => {
            commands::validate::validate_inputs(args, config.validate_config).await
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
