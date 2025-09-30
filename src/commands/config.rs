//! Implementation of the config command.

use clap::Parser;
use clap::Subcommand;

use crate::config::Config;

/// Arguments for the `config` subcommand.
#[derive(Parser, Debug, Clone)]
pub struct Args {
    /// Subcommand for the `config` command.
    #[command(subcommand)]
    command: ConfigSubcommand,
}

/// Subcommands for the `config` command.
#[derive(Subcommand, Debug, Clone)]
pub enum ConfigSubcommand {
    /// Generates a default configuration file.
    Init,

    /// Displays the current configuration.
    Resolve(ResolveArgs),
}

/// Arguments for the `config resolve` subcommand.
#[derive(Parser, Debug, Clone)]
pub struct ResolveArgs {
    /// Unredacts any redacted secrets in the configuration.
    #[clap(long)]
    unredact: bool,
}

/// Runs the `config` command.
pub fn config(args: Args, mut config: Config) -> anyhow::Result<()> {
    let config = match args.command {
        ConfigSubcommand::Init => Config::default(),
        ConfigSubcommand::Resolve(args) => {
            // Unredact any secrets if requested to
            if args.unredact {
                config.run.engine.unredact();
            }

            config
        }
    };

    println!("{}", toml::to_string_pretty(&config).unwrap_or_default());
    Ok(())
}
