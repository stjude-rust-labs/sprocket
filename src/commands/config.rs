//! Implementation of the config command.

use anyhow::Context;
use clap::Parser;
use clap::Subcommand;

use crate::commands::CommandError;
use crate::commands::CommandResult;
use crate::config::Config;

/// The [Taplo schema directive] for `sprocket.toml`.
///
/// [Taplo schema directive]: https://taplo.tamasfe.dev/configuration/directives.html#the-schema-directive
const SCHEMA_DIRECTIVE: &str = "#:schema https://raw.githubusercontent.com/stjude-rust-labs/sprocket/refs/heads/main/jsonschemas/sprocket.toml.json";

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
    /// Print the JSON schema for `sprocket.toml` to stdout.
    Schema,

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
    let mut include_schema_directive = false;
    let config = match args.command {
        ConfigSubcommand::Schema => {
            let schema = schemars::schema_for!(Config);
            let schema_pretty =
                serde_json::to_string_pretty(&schema).context("serializing config schema")?;
            println!("{schema_pretty}");
            return Ok(());
        }
        ConfigSubcommand::Init => {
            include_schema_directive = true;
            Config::default()
        }
        ConfigSubcommand::Resolve(args) => {
            // Redact any secrets unless explicitly requested not to
            if !args.unredact {
                config.run.engine = config.run.engine.redact();
            }

            config
        }
    };

    println!(
        "{}{}",
        if include_schema_directive {
            format!("{SCHEMA_DIRECTIVE}\n\n")
        } else {
            String::new()
        },
        toml_spanner::to_string(&config)
            .context("failed to serialize configuration")
            .map_err(CommandError::Single)?
    );
    Ok(())
}
