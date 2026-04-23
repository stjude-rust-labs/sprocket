//! Implementation of the language server protocol (LSP) subcommand.

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use clap::builder::PossibleValuesParser;
use wdl::analysis::FeatureFlags;
use wdl::lint::Baseline;
use wdl::lint::baseline::DEFAULT_BASELINE_FILENAME;
use wdl::lsp::LevelFilter;
use wdl::lsp::LintOptions;
use wdl::lsp::Server;
use wdl::lsp::ServerOptions;

use crate::Config;
use crate::FilterReloadHandle;
use crate::IGNORE_FILENAME;
use crate::Subscriber;
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
pub async fn analyzer(
    mut args: Args,
    config: Config,
    handle: FilterReloadHandle,
) -> CommandResult<()> {
    args.apply(&config);

    Server::<Subscriber>::run(
        ServerOptions {
            name: "Sprocket".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            log_level: LevelFilter::from(
                handle
                    .clone_current()
                    .expect("should exist")
                    .max_level_hint()
                    .unwrap_or(tracing::metadata::LevelFilter::WARN),
            ),
            lint: LintOptions {
                enabled: args.lint,
                config: Arc::new(config.check.lint),
            },
            exceptions: args.except,
            ignore_filename: Some(IGNORE_FILENAME.to_string()),
            feature_flags: FeatureFlags::default(),
            baseline: {
                let baseline_is_configured = config.check.baseline.is_some();
                let path = config
                    .check
                    .baseline
                    .clone()
                    .unwrap_or_else(|| PathBuf::from(DEFAULT_BASELINE_FILENAME));
                Baseline::load_or_default(&path, baseline_is_configured)
                    .map_err(anyhow::Error::from)?
            },
        },
        Some(handle),
    )
    .await
    .map_err(CommandError::from)
}
