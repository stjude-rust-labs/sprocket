//! Implementation of the `server` subcommand.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

use crate::Config;

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

    /// The output directory.
    #[clap(short, long, default_value = "./out", value_name = "OUTPUT_DIR")]
    pub output_directory: PathBuf,

    /// Maximum database connections.
    #[arg(long)]
    pub max_connections: Option<u32>,

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
    pub fn apply(mut self, mut config: Config) -> Config {
        if let Some(host) = self.host {
            config.server.host = host;
        }

        if let Some(port) = self.port {
            config.server.port = port;
        }

        if let Some(database_url) = self.database_url {
            config.server.database.url = Some(database_url);
        }

        if let Some(max_connections) = self.max_connections {
            config.server.database.max_connections = max_connections;
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

        config
    }
}

/// The main function for the `server` subcommand.
pub async fn server(args: Args, config: Config) -> Result<()> {
    let config = args.apply(config);
    crate::server::run(config.server, config.execution).await
}
