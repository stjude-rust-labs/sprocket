//! Implementation of the config command.

use clap::Parser;

use crate::config::Config;

/// Arguments for the `config` subcommand.
#[derive(Parser, Debug)]
pub struct Args {
    /// Generates a default configuration file.
    #[arg(long)]
    generate: bool,
}

/// Runs the `config` command.
pub fn config(args: Args, config: crate::config::Config) -> anyhow::Result<()> {
    if args.generate {
        tracing::info!("Generating default configuration file...");
        let default_config = Config::default();
        println!(
            "{}",
            toml::to_string_pretty(&default_config).unwrap_or_default()
        );
    } else {
        println!("{}", toml::to_string_pretty(&config).unwrap_or_default());
    }
    Ok(())
}
