//! Implementation of the configuration module.

use std::env;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use clap::ValueEnum;
use figment::Figment;
use figment::providers::Format;
use figment::providers::Serialized;
use figment::providers::Toml;
use serde::Deserialize;
use serde::Serialize;
use tracing::debug;
use tracing::warn;
use url::Url;
use wdl::ast::SupportedVersion;
use wdl::engine::Config as EngineConfig;

use crate::diagnostics::Mode;

/// Default host.
const DEFAULT_HOST: &str = "127.0.0.1";

/// Default port.
const DEFAULT_PORT: u16 = 8080;

/// Default database filename.
pub const DEFAULT_DATABASE_FILENAME: &str = "sprocket.db";

/// Default output directory.
pub const DEFAULT_OUTPUT_DIRECTORY: &str = "./out";

/// The name of the Sprocket configuration file.
const CONFIG_FILE_NAME: &str = "sprocket.toml";

/// Default output directory function for serde.
fn default_output_directory() -> PathBuf {
    PathBuf::from(DEFAULT_OUTPUT_DIRECTORY)
}

/// Represents the supported output color modes.
#[derive(Debug, Default, Clone, ValueEnum, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorMode {
    /// Automatically colorize output depending on output device.
    #[default]
    Auto,
    /// Always colorize output.
    Always,
    /// Never colorize output.
    Never,
}

impl FromStr for ColorMode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "auto" => Ok(Self::Auto),
            "always" => Ok(Self::Always),
            "never" => Ok(Self::Never),
            _ => bail!("invalid color mode `{s}`"),
        }
    }
}

impl std::fmt::Display for ColorMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::Always => write!(f, "always"),
            Self::Never => write!(f, "never"),
        }
    }
}

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
    /// Configuration for the `server` command.
    pub server: ServerConfig,
    /// Configuration for the `test` command.
    pub test: TestConfig,
    /// Common configuration options for all commands.
    pub common: CommonConfig,
}

/// Represents shared configuration options for Sprocket commands.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct CommonConfig {
    /// Display color output.
    pub color: ColorMode,
    /// The report mode.
    pub report_mode: Mode,
    /// WDL-specific configuration.
    #[serde(default)]
    pub wdl: WdlConfig,
}

/// WDL-specific configuration options shared across all commands.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct WdlConfig {
    /// The fallback version to use when a WDL document declares an
    /// unrecognized version (e.g., `version development`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_version: Option<SupportedVersion>,
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
    /// Enable sorting of input sections.
    pub sort_inputs: bool,
}

impl Default for FormatConfig {
    fn default() -> Self {
        let config = wdl::format::Config::default();
        Self {
            with_tabs: false,
            indentation_size: config.indent.num(),
            max_line_length: config
                .max_line_length
                .get()
                .expect("should have a max line length"),
            sort_inputs: config.sort_inputs,
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
    /// Enable all lint rules, even those outside the default set.
    ///
    /// This cannot be `true` while `only_lint_tags` is populated.
    pub all_lint_rules: bool,
    /// Set of lint tags to opt into. Leave this empty to use the default set of
    /// tags.
    pub only_lint_tags: Vec<String>,
    /// Set of lint tags to filter out of the enabled lint rules.
    pub filter_lint_tags: Vec<String>,
    /// Lint rule configuration.
    pub lint: wdl::lint::Config,
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
    pub engine: EngineConfig,

    /// The output directory (default: `./out`).
    ///
    /// Individual runs are stored at `<output_dir>/runs/<target>/<timestamp>/`.
    #[serde(default = "default_output_directory")]
    pub output_dir: PathBuf,

    /// The capacity of the events channel used to display progress statistics.
    ///
    /// If the number of progress events being generated outpaces Sprocket's
    /// ability to read the events for displaying the progress statistics and
    /// the channel reaches its capacity, Sprocket will stop displaying progress
    /// statistics. This is most likely to occur when Sprocket has concurrently
    /// queued up a large number of tasks.
    ///
    /// Increasing the capacity will increase the size of the memory allocation
    /// made by the events channel.
    ///
    /// The default is `5000`.
    pub events_capacity: Option<usize>,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            engine: EngineConfig::default(),
            output_dir: default_output_directory(),
            events_capacity: None,
        }
    }
}

/// Server database configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ServerDatabaseConfig {
    /// Database URL (e.g., `sqlite://sprocket.db`).
    /// If not provided, defaults to `sprocket.db` in the output directory.
    #[serde(default)]
    pub url: Option<String>,
}

/// Server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ServerConfig {
    /// Host to bind to (default: `127.0.0.1`).
    #[serde(default)]
    pub host: String,
    /// Port to bind to (default: `8080`).
    #[serde(default)]
    pub port: u16,
    /// Allowed CORS origins.
    #[serde(default)]
    pub allowed_origins: Vec<String>,
    /// Database configuration.
    #[serde(default)]
    pub database: ServerDatabaseConfig,
    /// Directory for workflow outputs (default: `./out`).
    #[serde(default = "default_output_directory")]
    pub output_directory: PathBuf,
    /// Allowed file paths for file-based workflows.
    #[serde(default)]
    pub allowed_file_paths: Vec<PathBuf>,
    /// Allowed URL prefixes for URL-based workflows.
    #[serde(default)]
    pub allowed_urls: Vec<String>,
    /// Maximum concurrent workflows (default: `None`).
    ///
    /// `None` means there is no limit on the number of executions.
    #[serde(default)]
    pub max_concurrent_runs: Option<usize>,
    /// The engine configuration to use during execution.
    #[serde(default)]
    pub engine: EngineConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: String::from(DEFAULT_HOST),
            port: DEFAULT_PORT,
            allowed_origins: Vec::new(),
            database: ServerDatabaseConfig::default(),
            output_directory: default_output_directory(),
            allowed_file_paths: Vec::new(),
            allowed_urls: Vec::new(),
            max_concurrent_runs: None,
            engine: EngineConfig::default(),
        }
    }
}

impl ServerConfig {
    /// Validates and normalizes the server configuration.
    ///
    /// This method:
    ///
    /// - Validates that all allowed URLs can be parsed as URLs
    /// - Canonicalizes all allowed file paths
    /// - Deduplicates and sorts allowed file paths
    /// - Deduplicates and sorts allowed URL prefixes
    ///
    /// # Errors
    ///
    /// Returns an error if any URL cannot be parsed or any path cannot be
    /// canonicalized.
    pub fn validate(&mut self) -> anyhow::Result<()> {
        // Validate max concurrent workflows is at least 1
        if let Some(max) = self.max_concurrent_runs
            && max == 0
        {
            anyhow::bail!("`max_concurrent_runs` must be at least 1");
        }

        // Validate that all allowed URLs can be parsed
        for url in &self.allowed_urls {
            Url::parse(url).with_context(|| format!("invalid URL in `allowed_urls`: `{}`", url))?;
        }

        // Canonicalize file paths
        self.allowed_file_paths = self
            .allowed_file_paths
            .iter()
            .map(|p| {
                p.canonicalize().with_context(|| {
                    format!(
                        "failed to canonicalize path in `allowed_file_paths`: `{}`",
                        p.display()
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Deduplicate and sort file paths
        self.allowed_file_paths.sort();
        self.allowed_file_paths.dedup();

        // Deduplicate and sort URLs
        self.allowed_urls.sort();
        self.allowed_urls.dedup();

        Ok(())
    }
}

/// `test` command configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct TestConfig {
    /// Number of test executions to run in parallel. The default is `50`.
    #[serde(default)]
    pub parallelism: usize,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self { parallelism: 50 }
    }
}

impl Config {
    /// Create a new config instance by reading potential configurations.
    pub fn new<'a>(
        paths: impl IntoIterator<Item = &'a Path>,
        skip_config_search: bool,
    ) -> Result<Self> {
        // Check for a config file in the current directory
        // Start a new Figment instance with default values
        let mut figment = Figment::new().admerge(Serialized::from(Config::default(), "default"));

        if !skip_config_search {
            // Start with a configuration file next to the `sprocket` executable
            if let Ok(path) = std::env::current_exe()
                && let Some(parent) = path.parent()
            {
                let path = parent.join(CONFIG_FILE_NAME);
                if path.exists() {
                    debug!("reading configuration from `{path}`", path = path.display());
                    figment = figment.admerge(Toml::file_exact(path));
                }
            }

            // Check XDG_CONFIG_HOME for a config file
            // On MacOS, check HOME for a config file
            #[cfg(target_os = "macos")]
            let dir = dirs::home_dir().map(|p| p.join(".config"));
            #[cfg(not(target_os = "macos"))]
            let dir = dirs::config_dir();

            if let Some(dir) = dir {
                let path = dir.join("sprocket").join(CONFIG_FILE_NAME);
                if path.exists() {
                    debug!("reading configuration from `{path}`", path = path.display());
                    figment = figment.admerge(Toml::file_exact(path));
                }
            }

            // Check PWD for a config file
            let path = Path::new(CONFIG_FILE_NAME);
            if path.exists() {
                debug!("reading configuration from `{path}`", path = path.display());
                figment = figment.admerge(Toml::file_exact(path));
            }

            // If provided, check config file from environment
            if let Ok(path) = env::var("SPROCKET_CONFIG")
                && !path.is_empty()
            {
                let path = Path::new(&path);
                if !path.exists() {
                    warn!(
                        "configuration file `{path}` specified with environment variable \
                         `SPROCKET_CONFIG` does not exist",
                        path = path.display()
                    );
                } else {
                    debug!(
                        "reading configuration from `{path}` via `SPROCKET_CONFIG`",
                        path = path.display()
                    );
                    figment = figment.admerge(Toml::file(path));
                }
            }
        }

        // Merge the given files
        for path in paths {
            if !path.exists() {
                bail!(
                    "configuration file `{path}` does not exist",
                    path = path.display()
                );
            }

            debug!(
                "reading configuration from `{path}` via CLI option",
                path = path.display()
            );
            figment = figment.admerge(Toml::file(path));
        }

        // Get the configuration from the Figment
        figment.extract().context("failed to merge configuration")
    }

    /// Validate a configuration.
    pub fn validate(&mut self) -> Result<()> {
        if self.check.all_lint_rules && !self.check.only_lint_tags.is_empty() {
            bail!("`all_lint_rules` cannot be specified with `only_lint_tags`")
        }

        // Validate server config
        self.server.validate()?;

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
