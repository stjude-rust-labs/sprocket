//! Implementation of the language server protocol (LSP) subcommand.

use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use clap::builder::PossibleValuesParser;
use wdl::lint::Baseline;
use wdl::lint::baseline::DEFAULT_BASELINE_FILENAME;
use wdl::lsp::ConfigReload;
use wdl::lsp::LevelFilter;
use wdl::lsp::LintOptions;
use wdl::lsp::Server;
use wdl::lsp::ServerOptions;
use wdl::lsp::UserOptions;

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

/// Builds the analyzer-affecting portion of a [`ServerOptions`] /
/// [`ConfigReload`] from a freshly loaded [`Config`], merging in the
/// CLI-only overrides (`--lint` and `--except`) that aren't part of
/// `sprocket.toml`.
fn build_config_reload(
    config: &Config,
    cli_lint: bool,
    cli_except: &[String],
    cwd: &Path,
) -> anyhow::Result<ConfigReload> {
    let mut exceptions = cli_except.to_vec();
    exceptions.extend(config.analyzer.except.iter().cloned());

    let resolution_context = crate::analysis::resolution_context_from_paths(
        &config.modules,
        &config.common.wdl.feature_flags,
        &[cwd.to_path_buf()],
    )?;

    let baseline_is_configured = config.check.baseline.is_some();
    let baseline_path = config
        .check
        .baseline
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_BASELINE_FILENAME));
    let baseline = Baseline::load_or_default(&baseline_path, baseline_is_configured)?;

    Ok(ConfigReload {
        exceptions,
        feature_flags: config.common.wdl.feature_flags,
        resolution_context,
        baseline,
        format: config.format,
        lint: LintOptions {
            enabled: cli_lint || config.analyzer.lint,
            config: Arc::new(config.check.lint.clone()),
        },
    })
}

/// Runs the `analyzer` command.
pub async fn analyzer(
    mut args: Args,
    config: Config,
    handle: FilterReloadHandle,
    config_paths: Vec<PathBuf>,
    skip_config_search: bool,
) -> CommandResult<()> {
    // Preserve the CLI-only overrides before merging them with the initial
    // configuration, so that a later reload (triggered by a `sprocket.toml`
    // change) can re-merge them with the *new* on-disk configuration rather
    // than accumulating the initial file's values forever.
    let cli_lint = args.lint;
    let cli_except = args.except.clone();

    args.apply(&config);

    let cwd = std::env::current_dir().map_err(anyhow::Error::from)?;

    let reload = build_config_reload(&config, cli_lint, &cli_except, &cwd)?;

    let reload_cwd = cwd.clone();
    let reload_config: Arc<dyn Fn() -> anyhow::Result<ConfigReload> + Send + Sync> =
        Arc::new(move || {
            // Re-run the same configuration search/merge used at startup (user
            // config directory, current working directory, `SPROCKET_CONFIG`,
            // any `--config` paths, etc.) so that a change to any applicable
            // `sprocket.toml` is picked up consistently with how the server was
            // first launched.
            let config = Config::new(
                config_paths.iter().map(PathBuf::as_path),
                skip_config_search,
            )?;
            build_config_reload(&config, cli_lint, &cli_except, &reload_cwd)
        });

    Server::<Subscriber>::run(
        ServerOptions {
            name: "Sprocket".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            exceptions: reload.exceptions.clone(),
            ignore_filename: Some(IGNORE_FILENAME.to_string()),
            feature_flags: reload.feature_flags,
            resolution_context: reload.resolution_context.clone(),
            baseline: reload.baseline.clone(),
            format: reload.format,
            config_filename: Some(String::from("sprocket.toml")),
            reload_config: Some(reload_config),
        },
        UserOptions {
            log_level: LevelFilter::from(
                handle
                    .clone_current()
                    .expect("should exist")
                    .max_level_hint()
                    .unwrap_or(tracing::metadata::LevelFilter::WARN),
            ),
            lint: reload.lint,
        },
        Some(handle),
    )
    .await
    .map_err(CommandError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_cli_and_config_exceptions_and_lint() {
        let mut config = Config::default();
        config.analyzer.except = vec![String::from("MetaSections")];
        config.analyzer.lint = false;

        let cwd = std::env::current_dir().expect("should get cwd");
        let reload = build_config_reload(
            &config,
            // cli_lint
            true,
            // cli_except
            &[String::from("UnusedInput")],
            &cwd,
        )
        .expect("should build reload");

        // Both the CLI-provided exception and the config-provided exception
        // should be present.
        assert!(reload.exceptions.contains(&String::from("UnusedInput")));
        assert!(reload.exceptions.contains(&String::from("MetaSections")));

        // `--lint` (cli_lint=true) should be OR'd in even though
        // `config.analyzer.lint` is false.
        assert!(reload.lint.enabled);
    }

    #[test]
    fn config_lint_flag_alone_enables_linting() {
        let mut config = Config::default();
        config.analyzer.lint = true;

        let cwd = std::env::current_dir().expect("should get cwd");
        let reload = build_config_reload(&config, /* cli_lint */ false, &[], &cwd)
            .expect("should build reload");

        assert!(reload.lint.enabled);
    }

    #[test]
    fn no_baseline_configured_yields_no_baseline() {
        let config = Config::default();

        let cwd = std::env::current_dir().expect("should get cwd");
        let reload = build_config_reload(&config, false, &[], &cwd).expect("should build reload");

        assert!(reload.baseline.is_none());
    }
}
