//! Implementation of the configuration module.

use std::env;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use figment::Figment;
use figment::providers::Format;
use figment::providers::Serialized;
use figment::providers::Toml;
use serde::Deserialize;
use serde::Serialize;
use tracing::trace;
use wdl::engine;

use crate::Mode;

/// Represents the configuration for the Sprocket CLI tool.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct Config {
    /// Configuration for the `format` command.
    pub format: FormatConfig,
    /// Configuration for the `check` and `lint` commands.
    pub check: CheckConfig,
    /// Configuration for the `analyzer` command.
    pub analyzer: AnalyzerConfig,
    /// Configuration for the `run` command.
    pub run: RunConfig,
    /// Common configuration options for all commands.
    pub common: CommonConfig,
}

/// Represents shared configuration options for Sprocket commands.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct CommonConfig {
    /// Display color output.
    pub color: bool,
    /// The report mode.
    pub report_mode: Mode,
}

impl Default for CommonConfig {
    fn default() -> Self {
        Self {
            color: true,
            report_mode: Mode::default(),
        }
    }
}

/// Represents the configuration for the Sprocket `format` command.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct FormatConfig {
    /// Use tabs for indentation (default is spaces).
    pub with_tabs: bool,
    /// The number of spaces to use for indentation levels (default is 4).
    pub indentation_size: usize,
    /// The maximum line length (default is 90).
    pub max_line_length: usize,
}

impl Default for FormatConfig {
    fn default() -> Self {
        let config = wdl::format::config::Config::default();
        Self {
            with_tabs: false,
            indentation_size: config.indent().num(),
            max_line_length: config
                .max_line_length()
                .expect("should have a max line length"),
        }
    }
}

/// Represents the configuration for the Sprocket `check` and `lint` commands.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct CheckConfig {
    /// Rule IDs to except from running.
    pub except: Vec<String>,
    /// Causes the command to fail if any warnings are reported.
    pub deny_warnings: bool,
    /// Causes the command to fail if any notes are reported.
    pub deny_notes: bool,
    /// Hide diagnostics with `note` severity.
    pub hide_notes: bool,
}

/// Represents the configuration for the Sprocket `analyzer` command.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct AnalyzerConfig {
    /// Whether to enable lint rules.
    pub lint: bool,
    /// Rule IDs to except from running.
    pub except: Vec<String>,
}

/// Represents the configuration for the Sprocket `run` command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct RunConfig {
    /// The engine configuration.
    #[serde(flatten)]
    pub engine: engine::config::Config,

    /// The "runs" directory under which new `run` invocations' execution
    /// directories will be placed.
    pub runs_dir: PathBuf,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            engine: engine::config::Config::default(),
            runs_dir: crate::commands::run::DEFAULT_RUNS_DIR.into(),
        }
    }
}

impl Config {
    /// Create a new config instance by reading potential configurations.
    pub fn new(path: Option<&Path>, skip_config_search: bool) -> Result<Self> {
        // Check for a config file in the current directory
        // Start a new Figment instance with default values
        let mut figment = Figment::new().admerge(Serialized::from(Config::default(), "default"));

        if !skip_config_search {
            // Check XDG_CONFIG_HOME for a config file
            // On MacOS, check HOME for a config file
            #[cfg(target_os = "macos")]
            let dir = dirs::home_dir().map(|p| p.join(".config"));
            #[cfg(not(target_os = "macos"))]
            let dir = dirs::config_dir();

            if let Some(dir) = dir {
                let path = dir.join("sprocket").join("sprocket.toml");
                if path.exists() {
                    trace!("reading configuration from `{path}`", path = path.display());
                    figment = figment.admerge(Toml::file_exact(path));
                }
            }

            // Check PWD for a config file
            let path = Path::new("sprocket.toml");
            if path.exists() {
                trace!("reading configuration from `{path}`", path = path.display());
                figment = figment.admerge(Toml::file_exact(path));
            }

            // If provided, check config file from environment
            if let Ok(path) = env::var("SPROCKET_CONFIG") {
                let path = Path::new(&path);
                if !path.exists() {
                    bail!(
                        "configuration file `{path}` specified with environment variable \
                         `SPROCKET_CONFIG` does not exist",
                        path = path.display()
                    );
                }

                trace!(
                    "reading configuration from `{path}` via `SPROCKET_CONFIG`",
                    path = path.display()
                );
                figment = figment.admerge(Toml::file(path));
            }
        }

        // If provided, check command line config file
        if let Some(path) = path {
            if !path.exists() {
                bail!(
                    "configuration file `{path}` does not exist",
                    path = path.display()
                );
            }

            trace!(
                "reading configuration from `{path}` via CLI option",
                path = path.display()
            );
            figment = figment.admerge(Toml::file(path));
        }

        // Get the configuration from the Figment
        figment.extract().context("failed to merge configuration")
    }

    /// Validate a configuration
    pub fn validate(&self) -> Result<()> {
        // Validate the configuration here
        Ok(())
    }

    /// Read a configuration file from the specified path.
    pub fn read_config(path: &str) -> Result<Self> {
        let data = std::fs::read(path).context("failed to open config file")?;
        let text = String::from_utf8(data).expect("failed to read config file");
        let config: Config =
            toml::from_str(text.as_str()).context("failed to parse config file")?;
        Ok(config)
    }

    /// Write a configuration to the specified path.
    pub fn write_config(&self, path: &str) -> Result<()> {
        let data = toml::to_string(self).context("failed to serialize config")?;
        std::fs::write(path, data).context("failed to write config file")
    }
}
