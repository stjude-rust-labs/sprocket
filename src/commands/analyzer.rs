//! Implementation of the language server protocol (LSP) subcommand.

use std::sync::Arc;

use clap::Parser;
use clap::builder::PossibleValuesParser;
use wdl::analysis::FeatureFlags;
use wdl::lsp::LintOptions;
use wdl::lsp::Server;
use wdl::lsp::ServerOptions;

use crate::Config;
use crate::IGNORE_FILENAME;
use crate::commands::CommandError;
use crate::commands::CommandResult;
use crate::commands::explain::ALL_RULE_IDS;

/// Arguments for the `analyzer` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// Use stdin and stdout for the RPC transport.
    #[clap(long, required = true)]
    pub stdio: bool,

    /// Whether or not to enable lint rules.
    #[clap(long)]
    pub lint: bool,

    /// Excepts (ignores) an analysis or lint rule.
    ///
    /// Repeat the flag multiple times to except multiple rules.
    #[clap(short, long, value_name = "RULE",
        value_parser = PossibleValuesParser::new(ALL_RULE_IDS.iter()),
        ignore_case = true,
        action = clap::ArgAction::Append,
        num_args = 1,
        hide_possible_values = true,
    )]
    pub except: Vec<String>,
}

impl Args {
    /// Applies the given configuration to the CLI arguments.
    fn apply(&mut self, config: &Config) {
        self.lint |= config.analyzer.lint;
        self.except.extend(config.analyzer.except.iter().cloned());
    }
}

/// Runs the `analyzer` command.
pub async fn analyzer(mut args: Args, config: Config) -> CommandResult<()> {
    args.apply(&config);

    Server::run(ServerOptions {
        name: Some("Sprocket".into()),
        version: Some(env!("CARGO_PKG_VERSION").into()),
        lint: LintOptions {
            enabled: args.lint,
            config: Arc::new(config.check.lint),
        },
        exceptions: args.except,
        ignore_filename: Some(IGNORE_FILENAME.to_string()),
        feature_flags: FeatureFlags::default(),
    })
    .await
    .map_err(CommandError::from)
}
