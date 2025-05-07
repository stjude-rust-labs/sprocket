//! Implementation of the configuration module.

use std::env;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use figment::Figment;
use figment::providers::Format;
use figment::providers::Serialized;
use figment::providers::Toml;
use serde::Deserialize;
use serde::Serialize;

use crate::Mode;

/// Represents the configuration for the Sprocket CLI tool.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct Config {
    /// Configuration for the `format` command.
    pub format: FormatConfig,
    /// Configuration for the `check` and `lint` commands.
    pub check: CheckConfig,
    /// Common configuration options for all commands.
    pub common: CommonConfig,
}

/// Represents shared configuration options for Sprocket commands.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct CommonConfig {
    /// Display color output.
    pub color: bool,
    /// The report mode.
    pub report_mode: Option<Mode>,
}

impl Default for CommonConfig {
    fn default() -> Self {
        Self {
            color: true,
            report_mode: None,
        }
    }
}

/// Represents the configuration for the Sprocket `format` command.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct FormatConfig {
    /// Use tabs for indentation (default is spaces).
    pub with_tabs: bool,
    /// The number of spaces to use for indentation levels (default is 4).
    pub indentation_size: Option<usize>,
    /// The maximum line length (default is 90).
    pub max_line_length: Option<usize>,
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
    /// Run the `shellcheck` program on command sections.
    pub shellcheck: bool,
    /// Hide diagnostics with `note` severity.
    pub hide_notes: bool,
}

impl Config {
    /// Create a new config instance by reading potential configurations.
    pub fn new(path: Option<String>) -> Self {
        // Check for a config file in the current directory
        // Start a new Figment instance with default values
        let mut figment = Figment::new().admerge(Serialized::from(Config::default(), "default"));

        // Check XDG_CONFIG_HOME for a config file
        // If XDG_CONFIG_HOME is not set, check HOME for a config file
        if let Some(xdg_config_home) = dirs::config_dir() {
            tracing::info!(
                "reading configuration from XDG_CONFIG_HOME: \
                 {xdg_config_home:?}/sprocket/sprocket.toml"
            );
            figment = figment.admerge(Toml::file(
                xdg_config_home.join("sprocket").join("sprocket.toml"),
            ));
        }

        // Check PWD for a config file
        if Path::exists(Path::new("sprocket.toml")) {
            tracing::info!("reading configuration from PWD/sprocket.toml");
            figment = figment.admerge(Toml::file("sprocket.toml"));
        }

        // If provided, check config file from environment
        if let Ok(config_file) = env::var("SPROCKET_CONFIG") {
            tracing::info!("reading configuration from SPROCKET_CONFIG: {config_file:?}");
            figment = figment.admerge(Toml::file(config_file));
        }

        // If provided, check command line config file
        if let Some(ref cli) = path {
            tracing::info!("reading configuration from --config: {cli:?}");
            figment = figment.admerge(Toml::file(cli));
        }

        // Get the configuration from the Figment
        let config: Config = match figment.extract() {
            Ok(config) => config,
            Err(e) => {
                tracing::error!("failed to read configuration: {e}");
                panic!("failed to read configuration: {e}");
            }
        };

        config
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
