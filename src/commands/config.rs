//! Implementation of the config command.

use anyhow::Context;
use clap::Parser;
use clap::Subcommand;

use crate::commands::CommandError;
use crate::commands::CommandResult;
use crate::config::Config;

/// Arguments for the `config` subcommand.
#[derive(Parser, Debug, Clone)]
pub struct Args {
    /// Subcommand for the `config` command.
    #[command(subcommand)]
    command: ConfigSubcommand,
}

impl Args {
    /// Returns `true` if the subcommand is 'Init'.
    pub fn is_init(&self) -> bool {
        matches!(self.command, ConfigSubcommand::Init)
    }
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
pub fn config(args: Args, mut config: Config) -> CommandResult<()> {
    let config = match args.command {
        ConfigSubcommand::Init => Config::default(),
        ConfigSubcommand::Resolve(args) => {
            // Redact any secrets unless explicitly requested not to
            if !args.unredact {
                config.run.engine = config.run.engine.redact();
            }

            config
        }
    };

    println!(
        "{}",
        toml_spanner::to_string(&config)
            .context("failed to serialize configuration")
            .map_err(CommandError::Single)?
    );
    Ok(())
}
