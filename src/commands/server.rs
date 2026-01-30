//! Implementation of the `server` subcommand.

use std::path::PathBuf;

use clap::Parser;

use crate::Config;
use crate::commands::CommandResult;

/// Arguments to the `server` subcommand.
#[derive(Parser, Debug)]
pub struct Args {
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
    pub output_directory: Option<PathBuf>,

    /// Allowed file paths for file-based workflows.
    #[arg(long)]
    pub allowed_file_paths: Vec<PathBuf>,

    /// Allowed URL prefixes for URL-based workflows.
    #[arg(long)]
    pub allowed_urls: Vec<String>,

    /// Allowed CORS origins.
    #[arg(long)]
    pub allowed_origins: Vec<String>,
}

impl Args {
    /// Applies the arguments to the configuration.
    fn apply(mut self, config: &mut Config) {
        if let Some(host) = self.host {
            config.server.host = host;
        }

        if let Some(port) = self.port {
            config.server.port = port;
        }

        if let Some(database_url) = self.database_url {
            config.server.database.url = Some(database_url);
        }

        if let Some(output_directory) = self.output_directory {
            config.execution.output_directory = output_directory;
        }

        config
            .execution
            .allowed_file_paths
            .append(&mut self.allowed_file_paths);
        config.execution.allowed_urls.append(&mut self.allowed_urls);
        config
            .server
            .allowed_origins
            .append(&mut self.allowed_origins);
    }
}

/// The main function for the `server` subcommand.
pub async fn server(args: Args, mut config: Config) -> CommandResult<()> {
    args.apply(&mut config);

    // Validate that at least one source type is allowed
    if config.execution.allowed_file_paths.is_empty() && config.execution.allowed_urls.is_empty() {
        return Err(anyhow::anyhow!(
            "at least one of `allowed_file_paths` or `allowed_urls` must be specified"
        )
        .into());
    }

    crate::server::run(config.server, config.execution)
        .await
        .map_err(Into::into)
}
