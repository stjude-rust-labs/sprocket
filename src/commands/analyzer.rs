//! Implementation of the language server protocol (LSP) subcommand.

use clap::Parser;
use clap::builder::PossibleValuesParser;
use wdl::lsp::Server;
use wdl::lsp::ServerOptions;

use crate::IGNORE_FILENAME;
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
    )]
    pub except: Vec<String>,
}

impl Args {
    /// Applies the configuration from the given config file to the command line
    /// arguments.
    pub fn apply(mut self, config: crate::config::Config) -> Self {
        self.lint = self.lint || config.analyzer.lint;
        self.except = self
            .except
            .clone()
            .into_iter()
            .chain(config.analyzer.except.clone())
            .collect();

        self
    }
}

/// Runs the `analyzer` command.
pub async fn analyzer(args: Args) -> anyhow::Result<()> {
    Server::run(ServerOptions {
        name: Some("Sprocket".into()),
        version: Some(env!("CARGO_PKG_VERSION").into()),
        lint: args.lint,
        exceptions: args.except,
        ignore_filename: Some(IGNORE_FILENAME.to_string()),
    })
    .await
}
