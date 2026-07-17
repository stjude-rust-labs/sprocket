//! Implementation of the `server` subcommand.
//!
//! Hosts the run-management commands that interact with a Sprocket HTTP API
//! server: starting the server itself, plus the server-client actions
//! (`submit`, `status`, `inspect`, `cancel`, `retry`).

use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use clap::Subcommand;
use wdl::diagnostics::Mode;

use crate::Config;
use crate::commands::CommandResult;
use crate::commands::cancel;
use crate::commands::inspect;
use crate::commands::retry;
use crate::commands::status;
use crate::commands::submit;

/// Arguments for the `server` subcommand.
#[derive(Parser, Debug)]
pub struct Args {
    /// The `server` subcommand to run.
    #[command(subcommand)]
    command: ServerSubcommand,
}

/// Subcommands of the `server` command.
#[derive(Subcommand, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ServerSubcommand {
    /// Run the HTTP API server for run execution.
    Start(StartArgs),
    /// Submit a workflow to a Sprocket HTTP API server.
    Submit(submit::Args),
    /// Show the status of one or all runs.
    Status(status::Args),
    /// Show detailed information about a run.
    Inspect(inspect::Args),
    /// Cancel a running or queued run.
    Cancel(cancel::Args),
    /// Retry a previous run, optionally with input overrides.
    Retry(retry::Args),
}

/// Arguments for the `server start` subcommand.
#[derive(Parser, Debug)]
pub struct StartArgs {
    /// Host to bind to.
    #[arg(long)]
    pub host: Option<String>,

    /// Port to bind to.
    #[arg(long)]
    pub port: Option<u16>,

    /// Database URL.
    #[arg(long)]
    pub database_url: Option<String>,

    /// The output directory (default: `./out`).
    #[clap(short, long, value_name = "OUTPUT_DIR")]
    pub output_dir: Option<PathBuf>,

    /// Allowed file paths for file-based workflows.
    #[arg(long)]
    pub allowed_file_paths: Vec<PathBuf>,

    /// Allowed URL prefixes for URL-based workflows.
    #[arg(long)]
    pub allowed_urls: Vec<String>,

    /// Allowed CORS origins.
    #[arg(long)]
    pub allowed_origins: Vec<String>,

    /// The report mode for any emitted diagnostics.
    #[arg(short = 'm', long, value_name = "MODE", global = true)]
    pub report_mode: Option<Mode>,
}

impl StartArgs {
    /// Applies the arguments to the configuration.
    fn apply(mut self, config: &mut Config) {
        if let Some(host) = self.host {
            config.server.host = host;
        }

        if let Some(port) = self.port {
            config.server.port = port;
        }

        if let Some(database_url) = self.database_url {
            config.server.database.url = database_url;
        }

        if let Some(output_dir) = self.output_dir {
            config.server.output_dir = output_dir;
        }

        config
            .server
            .allowed_file_paths
            .append(&mut self.allowed_file_paths);
        config.server.allowed_urls.append(&mut self.allowed_urls);
        config
            .server
            .allowed_origins
            .append(&mut self.allowed_origins);
    }
}

/// Starts the HTTP API server.
async fn start(args: StartArgs, mut config: Config, colorize: bool) -> CommandResult<()> {
    let report_mode = args.report_mode.unwrap_or_default();
    args.apply(&mut config);
    config
        .validate()
        .context("validating server configuration")?;

    // Validate that at least one source type is allowed
    if config.server.allowed_file_paths.is_empty() && config.server.allowed_urls.is_empty() {
        return Err(anyhow::anyhow!(
            "at least one of `allowed-file-paths` or `allowed-urls` must be specified"
        )
        .into());
    }

    crate::server::run(config, report_mode, colorize)
        .await
        .map_err(Into::into)
}

/// The main function for the `server` subcommand.
pub async fn server(args: Args, config: Config, colorize: bool) -> CommandResult<()> {
    match args.command {
        ServerSubcommand::Start(args) => start(args, config, colorize).await,
        ServerSubcommand::Submit(args) => submit::submit(args, config, colorize).await,
        ServerSubcommand::Status(args) => status::status(args, config, colorize).await,
        ServerSubcommand::Inspect(args) => inspect::inspect(args, config, colorize).await,
        ServerSubcommand::Cancel(args) => cancel::cancel(args, config).await,
        ServerSubcommand::Retry(args) => retry::retry(args, config, colorize).await,
    }
}
