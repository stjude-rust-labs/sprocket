//! Implementation of the `server` subcommand.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

/// Arguments to the `server` subcommand.
#[derive(Parser, Debug)]
pub struct Args {
    /// Configuration file path.
    #[arg(short, long)]
    pub config: Option<PathBuf>,
}

/// The main function for the `server` subcommand.
pub async fn server(args: Args) -> Result<()> {
    let config = if let Some(path) = args.config {
        crate::server::Config::from_file(&path)?
    } else {
        crate::server::Config::default()
    };

    crate::server::run(config).await
}
