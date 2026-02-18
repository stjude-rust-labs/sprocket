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

use std::fs::File;
use std::io::IsTerminal as _;
use std::io::stderr;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context as _;
use anyhow::Result;
use anyhow::anyhow;
use clap::CommandFactory as _;
use clap::Parser as _;
use clap_verbosity_flag::Verbosity;
use clap_verbosity_flag::WarnLevel;
use commands::Commands;
pub use config::ColorMode;
pub use config::Config;
pub use config::ServerConfig;
use git_testament::git_testament;
use git_testament::render_testament;
use tracing::level_filters::LevelFilter;
use tracing::trace;
use tracing_indicatif::IndicatifLayer;
use tracing_indicatif::IndicatifWriter;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::Subscriber;
use tracing_subscriber::fmt::format::DefaultFields;
use tracing_subscriber::fmt::format::Format;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::reload;

use crate::commands::CommandResult;

// Access to these modules is useful for integration testing and benchmarking,
// but since this is not intended to be used as a public interface, we hide them
// from generated rustdoc.
#[doc(hidden)]
pub mod analysis;
#[doc(hidden)]
pub mod commands;
mod config;
mod eval;
mod inputs;
pub mod server;
pub mod system;
mod test;

/// The Sprocket ignore file name.
const IGNORE_FILENAME: &str = ".sprocketignore";

git_testament!(TESTAMENT);

/// The `sprocket` CLI arguments.
#[derive(clap::Parser, Debug)]
#[command(author, version = render_testament!(TESTAMENT), propagate_version = true, about, long_about = None)]
struct Cli {
    /// The command to execute.
    #[command(subcommand)]
    command: Commands,

    /// The verbosity for log messages.
    #[command(flatten)]
    verbosity: Verbosity<WarnLevel>,

    /// Controls output colorization.
    #[arg(long, default_value = "auto", global = true)]
    color: ColorMode,

    /// Path to the configuration file.
    #[arg(long, short, global = true)]
    config: Vec<PathBuf>,

    /// The application data directory.
    #[arg(long, env = "SPROCKET_DATA_DIR", global = true)]
    data_dir: Option<PathBuf>,

    /// Skip searching for and loading configuration files.
    ///
    /// Only a configuration file specified as a command line argument will be
    /// used.
    #[arg(long, short, global = true)]
    skip_config_search: bool,
}

/// The data directory for `sprocket`.
///
/// This optionally takes the `--data-dir` specified on the CLI.
fn sprocket_data_dir(cli: Option<&Path>) -> CommandResult<PathBuf> {
    if let Some(dir) = cli {
        return Ok(PathBuf::from(dir));
    }

    if let Some(data_dir) = dirs::data_dir() {
        return Ok(data_dir.join("sprocket"));
    }

    if let Some(home_dir) = dirs::home_dir() {
        return Ok(home_dir.join(".sprocket"));
    }

    Err(anyhow!("Unable to determine sprocket data directory").into())
}

/// The `sprocket` components directory.
///
/// This optionally takes the `--data-dir` specified on the CLI.
fn sprocket_components_dir(cli_data_dir: Option<&Path>) -> CommandResult<PathBuf> {
    Ok(sprocket_data_dir(cli_data_dir)?.join("components"))
}

/// Logic for [`sprocket_main()`].
async fn real_main() -> CommandResult<()> {
    let cli = Cli::parse();

    let config = match &cli.command {
        Commands::Config(config_args) if config_args.is_init() => {
            // For `config init`, skip loading and use default
            Config::default()
        }
        _ => {
            // For all other commands, load config normally
            let mut config = Config::new(
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

    let color_mode = match (cli.color, config.common.color) {
        (ColorMode::Auto, ColorMode::Auto) => ColorMode::Auto,
        (ColorMode::Auto, ColorMode::Always) | (ColorMode::Always, _) => ColorMode::Always,
        (ColorMode::Auto, ColorMode::Never) | (ColorMode::Never, _) => ColorMode::Never,
    };
    let colorize = match color_mode {
        ColorMode::Auto => stderr().is_terminal(),
        ColorMode::Always => true,
        ColorMode::Never => false,
    };

    colored::control::set_override(colorize);
    let handle =
        initialize_logging(cli.verbosity, colorize).context("failed to initialize logging")?;

    match cli.command {
        Commands::Analyzer(args) => commands::analyzer::analyzer(args, config).await,
        Commands::Check(args) => commands::check::check(args, config, colorize).await,
        Commands::Completions(args) => {
            let mut cmd = Cli::command();
            commands::completions::completions(args, &mut cmd).await
        }
        Commands::Config(args) => commands::config::config(args, config),
        Commands::Explain(args) => commands::explain::explain(args),
        Commands::Format(args) => commands::format::format(args, config, colorize).await,
        Commands::Inputs(args) => commands::inputs::inputs(args, config).await,
        Commands::Lint(args) => commands::check::lint(args, config, colorize).await,
        Commands::Run(args) => commands::run::run(args, config, colorize, handle).await,
        Commands::Validate(args) => commands::validate::validate(args, config).await,
        Commands::Dev(commands::DevCommands::Doc(args)) => {
            commands::doc::doc(args, config, color_mode).await
        }
        Commands::Dev(commands::DevCommands::Lock(args)) => {
            commands::lock::lock(args, config).await
        }
        Commands::Dev(commands::DevCommands::Server(args)) => {
            commands::server::server(args, config).await
        }
        Commands::Dev(commands::DevCommands::Test(args)) => {
            commands::test::test(args, config).await
        }
    }
}

/// A type alias for a tracing format subscriber.
type FmtSubscriber =
    tracing_subscriber::FmtSubscriber<DefaultFields, Format, EnvFilter, IndicatifWriter>;

/// A type alias for a layered subscriber (wraps the indicatif layer)
type Layered = tracing_subscriber::layer::Layered<IndicatifLayer<FmtSubscriber>, FmtSubscriber>;

/// A type alias for the layer used by file logging.
type Layer = tracing_subscriber::fmt::Layer<Layered, DefaultFields, Format, File>;

/// A type alias for a logging reload handle.
///
/// This is used to initialize file logging *after* the global tracing
/// subscriber has been installed.
///
/// Initially the inner layer will be `None` which means file logging will not
/// take place.
type LoggingReloadHandle = reload::Handle<Option<Layer>, Layered>;

/// Initializes logging given the verbosity level and whether or not to colorize
/// log output.
///
/// This will also attempt to initialize logging in the presence of a `RUST_LOG`
/// environment variable; if a `RUST_LOG` environment variable is present, it
/// will take precedence over the given verbosity.
fn initialize_logging(
    verbosity: Verbosity<WarnLevel>,
    colorize: bool,
) -> Result<LoggingReloadHandle> {
    // Try to get a default environment filter via `RUST_LOG`
    let env_filter = match EnvFilter::try_from_default_env()
        .context("invalid `RUST_LOG` environment variable")
    {
        Ok(filter) => filter,
        Err(e) => {
            // If there was an error and the variable was set, then the error was due to
            // parsing an invalid directive
            if std::env::var("RUST_LOG").is_ok() {
                return Err(e);
            }

            // Otherwise, use a default directive env filter that disables noisy hyper
            // output
            EnvFilter::builder()
                .with_default_directive(LevelFilter::from(verbosity).into())
                .from_env_lossy()
                .add_directive("hyper_util=off".parse()?)
        }
    };

    // Set up an indicatif layer so that progress bars don't interfere with logging
    // output
    let indicatif_layer = IndicatifLayer::new();

    // To start, the file layer is `None` and may be reloaded later
    let (file_layer, handle) = reload::Layer::new(None);

    // Build the subscriber and set it as the global default
    let subscriber = Subscriber::builder()
        .with_env_filter(env_filter)
        .with_writer(indicatif_layer.get_stderr_writer())
        .with_ansi(colorize)
        .finish()
        .with(indicatif_layer)
        .with(file_layer);

    tracing::subscriber::set_global_default(subscriber)
        .context("failed to set tracing subscriber")?;

    Ok(handle)
}

/// The Sprocket command line entrypoint.
pub async fn sprocket_main<Guard>(guard: Guard) {
    if let Err(e) = real_main().await {
        drop(guard);
        eprintln!("{e}");
        std::process::exit(1);
    }
}
