//! Implementation of the language server protocol (LSP) subcommand.

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use clap::builder::PossibleValuesParser;
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
        // The `except` list lives under `[check]` and is shared with the
        // `check` command; see `CheckConfig::except`.
        self.except.extend(config.check.except.iter().cloned());
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
            exceptions: args.except,
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
                config: Arc::new(config.check.lint),
            },
        },
        Some(handle),
    )
    .await
    .map_err(CommandError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `except` list configured under `[check]` should be picked up by
    /// the `analyzer` command as well, since the two commands share a single
    /// except list (see #1008).
    #[test]
    fn apply_uses_check_except_list() {
        let mut config = Config::default();
        config.check.except = vec!["ContainerUri".to_string()];

        let mut args = Args {
            stdio: true,
            lint: false,
            except: Vec::new(),
        };
        args.apply(&config);

        assert_eq!(args.except, vec!["ContainerUri".to_string()]);
    }

    /// CLI-provided exceptions and config-provided exceptions should both be
    /// present after applying the configuration.
    #[test]
    fn apply_merges_cli_and_config_except_lists() {
        let mut config = Config::default();
        config.check.except = vec!["ContainerUri".to_string()];

        let mut args = Args {
            stdio: true,
            lint: false,
            except: vec!["MissingRequirements".to_string()],
        };
        args.apply(&config);

        assert_eq!(
            args.except,
            vec!["MissingRequirements".to_string(), "ContainerUri".to_string()]
        );
    }
}
