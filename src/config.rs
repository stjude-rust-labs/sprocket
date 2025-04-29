//! Implementation of the configuration module.

use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;

use crate::Mode;

/// Represents the configuration for the Sprocket CLI tool.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct Config {
    /// Configuration for the `format` command.
    #[serde(rename = "format")]
    pub format_config: FormatConfig,
    /// Configuration for the `check` and `lint` commands.
    #[serde(rename = "check")]
    pub check_config: CheckConfig,
    /// Configuration for the `validate` command.
    #[serde(rename = "validate")]
    pub validate_config: ValidateInputs,
}

/// Represents the configuration for the Sprocket `format` command.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct FormatConfig {
    /// Disables color output.
    pub no_color: bool,
    /// The report mode.
    pub report_mode: Option<Mode>,
    /// Use tabs for indentation (default is spaces).
    pub with_tabs: bool,
    /// The number of spaces to use for indentation levels (default is 4).
    pub indentation_size: Option<usize>,
    /// The maximum line length (default is 90).
    pub max_line_length: Option<usize>,
    /// Overwrite the WDL documents with the formatted versions.
    pub overwrite: bool,
    /// Check if the files are formatted correctly and print diff if not.
    pub check: bool,
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
    /// Suppress diagnostics from imported documents.
    pub single_document: bool,
    /// Show diagnostics for remote documents.
    pub show_remote_diagnostics: bool,
    /// Run the `shellcheck` program on command sections.
    pub shellcheck: bool,
    /// Hide diagnostics with `note` severity.
    pub hide_notes: bool,
    /// Disables color output.
    pub no_color: bool,
    /// The report mode.
    pub report_mode: Mode,
}

/// Represents the configuration for the Sprocket `validate` command.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ValidateInputs {
    /// Disables color output.
    pub no_color: bool,
    /// The report mode.
    pub report_mode: Mode,
}

impl Config {
    /// Validate a configuration
    pub fn validate(&self) -> Result<()> {
        // Validate the configuration here
        Ok(())
    }

    /// Read a configuration file from the specified path.
    pub fn read_config(path: &str) -> Result<Self> {
        let data = std::fs::read(path).context("Failed to open config file")?;
        let text = String::from_utf8(data).expect("Failed to read config file");
        let config: Config =
            toml::from_str(text.as_str()).context("Failed to parse config file")?;
        Ok(config)
    }

    /// Write a configuration to the specified path.
    pub fn write_config(&self, path: &str) -> Result<()> {
        let data = toml::to_string(self).context("Failed to serialize config")?;
        std::fs::write(path, data).context("Failed to write config file")
    }
}
