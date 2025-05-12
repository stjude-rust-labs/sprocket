//! Implementation of the config command.

use clap::{Parser, Subcommand};

use crate::config::Config;

/// Arguments for the `config` subcommand.
#[derive(Parser, Debug, Clone)]
pub struct Args {
    #[command(subcommand)]
    command: ConfigSubcommand,
}

/// Subcommands for the `config` command.
#[derive(Subcommand, Debug, Clone)]
pub enum ConfigSubcommand {
    /// Generates a default configuration file.
    Init,

    /// Displays the current configuration.
    Resolve,
}


/// Runs the `config` command.
pub fn config(args: Args, config: Config) -> anyhow::Result<()> {
    if let ConfigSubcommand::Init = args.command {
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
