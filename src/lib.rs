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
use std::io;
use std::io::IsTerminal as _;
use std::io::stderr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use anyhow::Context as _;
use anyhow::Result;
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
use tracing_indicatif::writer;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::MakeWriter;
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
mod diagnostics;
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

    /// Skip searching for and loading configuration files.
    ///
    /// Only a configuration file specified as a command line argument will be
    /// used.
    #[arg(long, short, global = true)]
    skip_config_search: bool,
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

    let colorize = match (cli.color, config.common.color) {
        (ColorMode::Auto, ColorMode::Auto) => stderr().is_terminal(),
        (ColorMode::Auto, ColorMode::Always) => true,
        (ColorMode::Auto, ColorMode::Never) => false,
        (ColorMode::Always, _) => true,
        (ColorMode::Never, _) => false,
    };

    colored::control::set_override(colorize);
    let (writer, file_handle) =
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
        Commands::Format(args) => commands::format::format(args.apply(config), colorize).await,
        Commands::Inputs(args) => commands::inputs::inputs(args).await,
        Commands::Lint(args) => commands::check::lint(args, config, colorize).await,
        Commands::Run(args) => commands::run::run(args, config, colorize, file_handle).await,
        Commands::Validate(args) => commands::validate::validate(args.apply(config)).await,
        Commands::Dev(commands::DevCommands::Doc(args)) => commands::doc::doc(args, colorize).await,
        Commands::Dev(commands::DevCommands::Lock(args)) => commands::lock::lock(args).await,
        Commands::Dev(commands::DevCommands::Server(args)) => {
            commands::server::server(args, config).await
        }
        Commands::Dev(commands::DevCommands::Test(args)) => {
            commands::test::test(args.apply(config), writer).await
        }
    }
}

/// A type alias for the stderr writer.
pub type StdErrWriter = Writer<IndicatifWriter<writer::Stderr>>;

/// A type alias for a tracing format subscriber.
pub type FmtSubscriber =
    tracing_subscriber::FmtSubscriber<DefaultFields, Format, EnvFilter, StdErrWriter>;

/// A type alias for a layered subscriber (wraps the indicatif layer)
pub type Layered = tracing_subscriber::layer::Layered<IndicatifLayer<FmtSubscriber>, FmtSubscriber>;

/// A type alias for the layer used by file logging.
pub type FileLayer = tracing_subscriber::fmt::Layer<Layered, DefaultFields, Format, File>;

/// A type alias for a logging reload handle.
///
/// This is used to initialize file logging *after* the global tracing
/// subscriber has been installed.
///
/// Initially the inner layer will be `None` which means file logging will not
/// take place.
pub type FileLoggingReloadHandle = reload::Handle<Option<FileLayer>, Layered>;

/// A logging writer implementation that can be disabled at runtime.
#[derive(Debug, Clone)]
pub struct Writer<W> {
    /// The underlying writer to write to when this writer is enabled.
    writer: W,
    /// Stores the count of requests to disable the writer.
    ///
    /// When this is non-zero, the writer is considered to be disabled.
    disabled: Arc<AtomicUsize>,
}

impl<W> Writer<W> {
    /// Constructs a new `Writer` to wrap the given writer.
    fn new(writer: W) -> Self {
        Self {
            writer,
            disabled: Arc::default(),
        }
    }

    /// Decrements the internal disabled count.
    ///
    /// # Panics
    ///
    /// Panics if the internal disabled count underflows.
    fn enable(&self) {
        if self.disabled.fetch_sub(1, Ordering::SeqCst) == usize::MAX {
            panic!("writer disable count has underflowed");
        }
    }

    /// Increments the internal disabled count.
    ///
    /// # Panics
    ///
    /// Panics if the internal disabled count overflows.
    fn disable(&self) {
        if self.disabled.fetch_add(1, Ordering::SeqCst) == usize::MIN {
            panic!("writer disable count has overflowed");
        }
    }
}

impl<W: io::Write> io::Write for Writer<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.disabled.load(Ordering::SeqCst) == 0 {
            self.writer.write(buf)
        } else {
            Ok(buf.len())
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.disabled.load(Ordering::SeqCst) == 0 {
            self.writer.flush()
        } else {
            Ok(())
        }
    }
}

impl<'a, W: io::Write + Clone> MakeWriter<'a> for Writer<W> {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// Initializes logging given the verbosity level and whether or not to colorize
/// log output.
///
/// This will also attempt to initialize logging in the presence of a `RUST_LOG`
/// environment variable; if a `RUST_LOG` environment variable is present, it
/// will take precedence over the given verbosity.
fn initialize_logging(
    verbosity: Verbosity<WarnLevel>,
    colorize: bool,
) -> Result<(StdErrWriter, FileLoggingReloadHandle)> {
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
    // output; the indicatif writer is wrapped with a writer that can be disabled
    let indicatif_layer = IndicatifLayer::new();
    let writer = Writer::new(indicatif_layer.get_stderr_writer());

    // To start, the file layer is `None` and may be reloaded later
    let (file_layer, file_handle) = reload::Layer::new(None);

    // Build the subscriber and set it as the global default
    let subscriber = Subscriber::builder()
        .with_env_filter(env_filter)
        .with_writer(writer.clone())
        .with_ansi(colorize)
        .finish()
        .with(indicatif_layer)
        .with(file_layer);

    tracing::subscriber::set_global_default(subscriber)
        .context("failed to set tracing subscriber")?;

    Ok((writer, file_handle))
}

/// The Sprocket command line entrypoint.
pub async fn sprocket_main<Guard>(guard: Guard) {
    if let Err(e) = real_main().await {
        drop(guard);
        eprintln!("{e}");
        std::process::exit(1);
    }
}
