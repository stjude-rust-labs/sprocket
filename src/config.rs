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
    /// Common configuration options for all commands.
    #[serde(rename = "common")]
    pub common_config: CommonConfig,
}

/// Represents shared configuration options for Sprocket commands.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct CommonConfig {
    /// Display color output.
    pub color: bool,
    /// The report mode.
    pub report_mode: Option<Mode>,
}

/// Represents the configuration for the Sprocket `format` command.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct FormatConfig {
    /// Disables color output.
    pub no_color: bool,
    /// Enable or disable color output.
    pub color: bool,
    /// The report mode.
    pub report_mode: Option<Mode>,
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
    /// Suppress diagnostics from imported documents.
    pub single_document: bool,
    /// Show diagnostics for remote documents.
    pub show_remote_diagnostics: bool,
    /// Run the `shellcheck` program on command sections.
    pub shellcheck: bool,
    /// Hide diagnostics with `note` severity.
    pub hide_notes: bool,
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

    /// Merge the current configuration with commandline arguments.
    pub fn merge_args(&self, args: crate::commands::Commands) -> crate::commands::Commands {
        // Merge the common configuration with the commandline arguments
        match args {
            crate::commands::Commands::Format(format_args) => {
                crate::commands::Commands::Format(crate::commands::format::FormatArgs {
                    path: format_args.path,
                    no_color: format_args.no_color || !self.common_config.color,
                    report_mode: match format_args.report_mode {
                        Some(mode) => Some(mode),
                        None => self.common_config.report_mode,
                    },
                    with_tabs: format_args.with_tabs || self.format_config.with_tabs,
                    indentation_size: match format_args.indentation_size {
                        Some(size) => Some(size),
                        None => self.format_config.indentation_size,
                    },
                    max_line_length: format_args
                        .max_line_length
                        .or(self.format_config.max_line_length),
                    mode: format_args.mode,
                })
            }
            crate::commands::Commands::Check(check_args) => {
                crate::commands::Commands::Check(crate::commands::check::CheckArgs {
                    common: crate::commands::check::Common {
                        file: check_args.common.file,
                        except: check_args
                            .common
                            .except
                            .into_iter()
                            .chain(self.check_config.except.clone())
                            .collect(),
                        deny_warnings: check_args.common.deny_warnings
                            || self.check_config.deny_warnings,
                        deny_notes: check_args.common.deny_notes || self.check_config.deny_notes,
                        single_document: check_args.common.single_document
                            || self.check_config.single_document,
                        show_remote_diagnostics: check_args.common.show_remote_diagnostics
                            || self.check_config.show_remote_diagnostics,
                        shellcheck: check_args.common.shellcheck || self.check_config.shellcheck,
                        hide_notes: check_args.common.hide_notes || self.check_config.hide_notes,
                        no_color: check_args.common.no_color || !self.common_config.color,
                        report_mode: match check_args.common.report_mode {
                            Some(mode) => Some(mode),
                            None => self.common_config.report_mode,
                        },
                    },
                    lint: check_args.lint,
                })
            }
            crate::commands::Commands::ValidateInputs(validate_args) => {
                crate::commands::Commands::ValidateInputs(
                    crate::commands::validate::ValidateInputsArgs {
                        document: validate_args.document,
                        inputs: validate_args.inputs,
                        no_color: validate_args.no_color || !self.common_config.color,
                        report_mode: match validate_args.report_mode {
                            Some(mode) => Some(mode),
                            None => self.common_config.report_mode,
                        },
                    },
                )
            }
            crate::commands::Commands::Lint(lint_args) => {
                crate::commands::Commands::Lint(crate::commands::check::LintArgs {
                    common: crate::commands::check::Common {
                        file: lint_args.common.file,
                        except: lint_args
                            .common
                            .except
                            .into_iter()
                            .chain(self.check_config.except.clone())
                            .collect(),
                        deny_warnings: lint_args.common.deny_warnings
                            || self.check_config.deny_warnings,
                        deny_notes: lint_args.common.deny_notes || self.check_config.deny_notes,
                        single_document: lint_args.common.single_document
                            || self.check_config.single_document,
                        show_remote_diagnostics: lint_args.common.show_remote_diagnostics
                            || self.check_config.show_remote_diagnostics,
                        shellcheck: lint_args.common.shellcheck || self.check_config.shellcheck,
                        hide_notes: lint_args.common.hide_notes || self.check_config.hide_notes,
                        no_color: lint_args.common.no_color || !self.common_config.color,
                        report_mode: match lint_args.common.report_mode {
                            Some(mode) => Some(mode),
                            None => self.common_config.report_mode,
                        },
                    },
                })
            }
            crate::commands::Commands::Config(_) => args,
            crate::commands::Commands::Explain(_) => args,
            crate::commands::Commands::Analyzer(_) => args,
        }
    }
}
