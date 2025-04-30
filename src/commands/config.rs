//! Implementation of the config command.

use clap::Parser;

/// Arguments for the `config` subcommand.
#[derive(Parser, Debug)]
pub struct ConfigArgs;

/// Runs the `config` command.
pub fn config(_args: ConfigArgs, config: crate::config::Config) -> anyhow::Result<()> {
    println!("Effective configuration:\n{}",
        toml::to_string(&config).unwrap_or_default());
    Ok(())
}