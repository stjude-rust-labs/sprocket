//! Implementation of the language server protocol (LSP) subcommand.

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use wdl::lint::Baseline;
use wdl::lint::baseline::DEFAULT_BASELINE_FILENAME;
use wdl::lsp::LevelFilter;
use wdl::lsp::LintOptions;
use wdl::lsp::Server;
use wdl::lsp::ServerOptions;
use wdl::lsp::UserOptions;

use crate::Config;
use crate::FilterReloadHandle;
use crate::Subscriber;
use crate::commands::CommandError;
use crate::commands::CommandResult;

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
}

impl Args {
    /// Applies the given configuration to the CLI arguments.
    fn apply(&mut self, config: &Config) {
        self.lint |= config.analyzer.lint;
    }
}

/// Runs the `analyzer` command.
pub async fn analyzer(
    mut args: Args,
    config: Config,
    handle: FilterReloadHandle,
) -> CommandResult<()> {
    args.apply(&config);

    let cwd = std::env::current_dir().map_err(anyhow::Error::from)?;
    let resolution_context =
        crate::analysis::resolution_context_from_paths(&config.modules, &[cwd])?;

    Server::<Subscriber>::run(
        ServerOptions {
            name: "Sprocket".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            disable: config.check.disable,
            ignore_filename: config.common.ignore_filename(),
            feature_flags: config.common.wdl.feature_flags,
            resolution_context,
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
            format: config.format,
        },
        UserOptions {
            log_level: LevelFilter::from(
                handle
                    .clone_current()
                    .expect("should exist")
                    .max_level_hint()
                    .unwrap_or(tracing::metadata::LevelFilter::WARN),
            ),
            lint: LintOptions {
                enabled: args.lint,
                config: Arc::new(config.check.rules),
            },
        },
        Some(handle),
    )
    .await
    .map_err(CommandError::from)
}
