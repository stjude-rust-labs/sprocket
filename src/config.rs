//! Implementation of the configuration module.

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::env;
use std::fmt;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::LazyLock;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use clap::ValueEnum;
use toml_spanner::Arena;
use toml_spanner::Error as TomlError;
use toml_spanner::Failed;
use toml_spanner::FromToml;
use toml_spanner::Item;
use toml_spanner::ToToml;
use toml_spanner::ToTomlError;
use toml_spanner::Toml;
use toml_spanner::helper::display;
use toml_spanner::helper::flatten_any;
use toml_spanner::helper::parse_string;
use tracing::debug;
use tracing::warn;
use url::Url;
use wdl::ast::Severity;
use wdl::ast::SupportedVersion;
use wdl::diagnostics::Mode;
use wdl::engine::Config as EngineConfig;
use wdl::format::Config as FormatConfig;
use wdl_modules::resolver::ModulesConfig;

/// Default host.
const DEFAULT_HOST: &str = "127.0.0.1";

/// Default port.
const DEFAULT_PORT: u16 = 8080;

/// Default database filename.
pub const DEFAULT_DATABASE_FILENAME: &str = "sprocket.db";

/// Sentinel value for using a local database.
const SENTINEL_DATABASE_FILENAME: &str = "default";

/// Default output directory.
pub const DEFAULT_OUTPUT_DIRECTORY: &str = "./out";

/// The name of the Sprocket configuration file.
const CONFIG_FILENAME: &str = "sprocket.toml";

/// Returns the user-level Sprocket configuration directory, the same root
/// `sprocket.toml` is read from. Use this anywhere a path needs to live
/// alongside the user's Sprocket config.
///
/// On macOS this is `$HOME/.config/sprocket/`, on Linux it follows
/// `$XDG_CONFIG_HOME` (typically `~/.config/sprocket/`), on Windows it lands
/// in `%APPDATA%/sprocket/`. Returns `None` when the underlying base
/// directory cannot be determined (no `$HOME`, etc.).
pub fn config_root() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    let base = dirs::home_dir().map(|p| p.join(".config"));
    #[cfg(not(target_os = "macos"))]
    let base = dirs::config_dir();
    base.map(|d| d.join("sprocket"))
}

/// The capacity for the events channels.
///
/// This is the number of events to buffer in the events channel before
/// receivers become lagged.
///
/// As `tokio::sync::broadcast` channels are used to support multiple receivers,
/// an event is only dropped from the channel once *all* receivers have read it.
///
/// If the senders are sending events faster than all receivers can read the
/// events, the channel buffer will eventually reach capacity.
///
/// When this happens, the oldest events in the buffer are dropped and receivers
/// are notified via an error on the next read that they are lagging behind.
///
/// If the capacity is reached, Sprocket will stop displaying progress
/// statistics.
///
/// The value of `5000` was chosen as a reasonable amount to make reaching
/// capacity unlikely without allocating too much space unnecessarily.
const DEFAULT_EVENTS_CHANNEL_CAPACITY: u32 = 5000;

/// The default parallelism for the `sprocket test` command.
const DEFAULT_TEST_PARALLELISM: u32 = 50;

/// The default throttling for the `sprocket test` command.
const DEFAULT_TEST_THROTTLE: u64 = 100;

/// Represents the supported output color modes.
#[derive(Debug, Default, Clone, ValueEnum, Copy, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone, Default, Toml)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
pub struct Config {
    /// Configuration for the `format` command.
    #[toml(default, style = Header)]
    pub format: FormatConfig,
    /// Configuration for the `check` and `lint` commands.
    #[toml(default, style = Header)]
    pub check: CheckConfig,
    /// Configuration for the `analyzer` command.
    #[toml(default, style = Header)]
    pub analyzer: AnalyzerConfig,
    /// Configuration for the `run` command.
    #[toml(default, style = Header)]
    pub run: RunConfig,
    /// Configuration for the `server` command.
    #[toml(default, style = Header)]
    pub server: ServerConfig,
    /// Configuration for the `test` command.
    #[toml(default, style = Header)]
    pub test: TestConfig,
    /// Configuration for the `doc` command.
    #[toml(default, style = Header)]
    pub doc: DocConfig,
    /// Common configuration options for all commands.
    #[toml(default, style = Header)]
    pub common: CommonConfig,
    /// Configuration for the module system (`[modules]` section).
    #[toml(default, style = Header)]
    pub modules: ModulesConfig,
}

impl Config {
    /// Gets a builder for the `[Config]`.
    pub fn builder() -> wdl::engine::config::ConfigBuilder<Self> {
        Default::default()
    }
}

/// Represents shared configuration options for Sprocket commands.
#[derive(Debug, Default, Clone, PartialEq, Eq, Toml)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
pub struct CommonConfig {
    /// Display color output.
    #[toml(default, FromToml with = parse_string, ToToml with = display)]
    pub color: ColorMode,
    /// The report mode.
    #[toml(default, FromToml with = parse_string, ToToml with = display)]
    pub report_mode: Mode,
    /// WDL-specific configuration.
    #[toml(default, style = Header)]
    pub wdl: WdlConfig,
}

/// Represents a fallback WDL version to use.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub enum FallbackVersion {
    /// Do not use a fallback WDL version.
    #[default]
    None,
    /// Fallback to the specified WDL version.
    Version(SupportedVersion),
}

impl From<SupportedVersion> for FallbackVersion {
    fn from(value: SupportedVersion) -> Self {
        Self::Version(value)
    }
}

impl From<Option<SupportedVersion>> for FallbackVersion {
    fn from(value: Option<SupportedVersion>) -> Self {
        match value {
            Some(value) => Self::Version(value),
            None => Self::None,
        }
    }
}

impl From<FallbackVersion> for Option<SupportedVersion> {
    fn from(value: FallbackVersion) -> Self {
        match value {
            FallbackVersion::None => None,
            FallbackVersion::Version(value) => Some(value),
        }
    }
}

impl<'de> FromToml<'de> for FallbackVersion {
    fn from_toml(ctx: &mut toml_spanner::Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
        if let Some(s) = item.as_str() {
            match s {
                "none" => return Ok(Self::None),
                _ => {
                    if let Ok(v) = s.parse() {
                        return Ok(Self::Version(v));
                    }
                }
            }
        }

        Err(ctx.report_custom_error("expected a supported WDL version or `none`", item))
    }
}

impl fmt::Display for FallbackVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Version(v) => v.fmt(f),
        }
    }
}

/// WDL-specific configuration options shared across all commands.
#[derive(Debug, Clone, Default, PartialEq, Eq, Toml)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
pub struct WdlConfig {
    /// The fallback version to use when a WDL document declares an
    /// unrecognized version (e.g., `version development`).
    #[toml(default, ToToml with = display)]
    pub fallback_version: FallbackVersion,
}

/// Represents the configuration for the Sprocket `check` and `lint` commands.
#[derive(Debug, Clone, Default, PartialEq, Eq, Toml)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
pub struct CheckConfig {
    /// Rule IDs to except from running.
    #[toml(default)]
    pub except: Vec<String>,
    /// Causes the command to fail if any warnings are reported.
    #[toml(default)]
    pub deny_warnings: bool,
    /// Causes the command to fail if any notes or warnings are reported.
    #[toml(default)]
    pub deny_notes: bool,
    /// Hide diagnostics with `note` severity.
    #[toml(default)]
    pub hide_notes: bool,
    /// Hide diagnostics with `warning` and `note` severity.
    #[toml(default)]
    pub hide_warnings: bool,
    /// Enable all lint rules, even those outside the default set.
    ///
    /// This cannot be `true` while `only_lint_tags` is populated.
    #[toml(default)]
    pub all_lint_rules: bool,
    /// Set of lint tags to opt into. Leave this empty to use the default set of
    /// tags.
    #[toml(default)]
    pub only_lint_tags: Vec<String>,
    /// Set of lint tags to filter out of the enabled lint rules.
    #[toml(default)]
    pub filter_lint_tags: Vec<String>,
    /// Path to the diagnostic baseline file.
    pub baseline: Option<PathBuf>,
    /// Per-rule configuration, keyed by rule ID.
    #[toml(default, style = Header)]
    pub rules: RuleConfigs,
    /// The removed `[check.lint]` table.
    ///
    /// This is retained only to detect configurations that predate per-rule
    /// tables so a precise migration error can be reported.
    #[toml(default, style = Header)]
    pub lint: Option<LegacyLint>,
}

/// The removed `[check.lint]` table.
///
/// Its presence triggers a migration error pointing at the per-rule tables.
#[derive(Debug, Clone, Default, PartialEq, Eq, Toml)]
#[toml(Toml, rename_all = "snake_case")]
pub struct LegacyLint {
    /// The old global `allowed_names` parameter.
    #[toml(default)]
    pub allowed_names: Option<Vec<String>>,
    /// The old global `allowed_runtime_keys` parameter.
    #[toml(default)]
    pub allowed_runtime_keys: Option<Vec<String>>,
}

/// All `wdl-lint` rule IDs.
static LINT_RULE_IDS: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| wdl::lint::ALL_RULE_IDS.iter().map(String::as_str).collect());

/// All `wdl-analysis` rule IDs.
static ANALYSIS_RULE_IDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    wdl::analysis::ALL_RULE_IDS
        .iter()
        .map(String::as_str)
        .collect()
});

/// Maps a configurable parameter name to the rules it applies to.
static PARAM_APPLICABILITY: LazyLock<HashMap<&'static str, &'static [&'static str]>> =
    LazyLock::new(|| {
        wdl::lint::Config::params()
            .into_iter()
            .map(|param| (param.name, param.applicable_rules))
            .collect()
    });

/// The unified per-rule configuration table (`[check.rules]`).
///
/// Each entry is keyed by rule ID and may set a `severity` override plus any
/// parameters applicable to that rule. Both `wdl-analysis` and `wdl-lint` rules
/// share this namespace.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuleConfigs(wdl::lint::Config);

impl RuleConfigs {
    /// Returns the underlying lint configuration consumed by the linter.
    pub fn lint_config(&self) -> &wdl::lint::Config {
        &self.0
    }

    /// Returns the analysis-rule severity overrides.
    ///
    /// Only entries for `wdl-analysis` rules that set a severity are included.
    /// A value of `None` disables the corresponding diagnostic.
    pub fn analysis_severity_overrides(&self) -> BTreeMap<String, Option<Severity>> {
        self.0
            .iter()
            .filter(|(id, _)| ANALYSIS_RULE_IDS.contains(id.as_str()))
            .filter_map(|(id, rule)| rule.severity.map(|s| (id.clone(), s.as_severity())))
            .collect()
    }

    /// Returns the IDs of rules disabled via `severity = "off"`.
    pub fn disabled_rules(&self) -> Vec<String> {
        self.0
            .iter()
            .filter(|(_, rule)| rule.severity == Some(wdl::lint::RuleSeverity::Off))
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Returns the IDs of rules that have a concrete (non-`off`) severity
    /// override.
    ///
    /// Configuring a concrete severity opts a rule in regardless of tag
    /// selection.
    pub fn enabled_rules(&self) -> Vec<String> {
        self.0
            .iter()
            .filter(
                |(_, rule)| matches!(rule.severity, Some(s) if s != wdl::lint::RuleSeverity::Off),
            )
            .map(|(id, _)| id.clone())
            .collect()
    }
}

impl<'de> FromToml<'de> for RuleConfigs {
    fn from_toml(ctx: &mut toml_spanner::Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
        let table = item.require_table(ctx)?;
        let mut map = BTreeMap::new();
        let mut failed = false;

        for (key, value) in table {
            // Map deprecated aliases (e.g. `SnakeCase`) to their current rule.
            let rule_id = wdl::analysis::canonical_rule_id(key.name);
            let is_lint = LINT_RULE_IDS.contains(rule_id);
            let is_analysis = ANALYSIS_RULE_IDS.contains(rule_id);

            if !is_lint && !is_analysis {
                let suggestion = wdl::lint::find_nearest_rule(rule_id)
                    .map(|rule| format!("; did you mean `{rule}`?"))
                    .unwrap_or_default();
                ctx.push_error(TomlError::custom(
                    format!("unknown rule `{rule_id}` in `[check.rules]`{suggestion}"),
                    key.span,
                ));
                failed = true;
                continue;
            }

            // Validate that each configured parameter applies to the rule. Keys
            // unknown to every rule are left for the `deny_unknown_fields` check
            // performed when the entry is parsed below.
            if let Some(entry) = value.as_table() {
                for (param, _) in entry {
                    if param.name == "severity" {
                        continue;
                    }

                    if let Some(rules) = PARAM_APPLICABILITY.get(param.name)
                        && !(is_lint && rules.contains(&rule_id))
                    {
                        ctx.push_error(TomlError::custom(
                            format!(
                                "`{param}` is not a configurable parameter for rule `{rule_id}`",
                                param = param.name
                            ),
                            param.span,
                        ));
                        failed = true;
                    }
                }
            }

            match wdl::lint::RuleConfig::from_toml(ctx, value) {
                Ok(config) => {
                    map.insert(rule_id.to_string(), config);
                }
                Err(_) => failed = true,
            }
        }

        if failed {
            Err(Failed)
        } else {
            Ok(Self(wdl::lint::Config::from_map(map)))
        }
    }
}

impl ToToml for RuleConfigs {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        self.0.to_toml(arena)
    }
}

/// Represents the configuration for the Sprocket `analyzer` command.
#[derive(Debug, Clone, Default, Toml, PartialEq, Eq)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
pub struct AnalyzerConfig {
    /// Whether to enable lint rules.
    #[toml(default)]
    pub lint: bool,
    /// Rule IDs to except from running.
    #[toml(default)]
    pub except: Vec<String>,
}

/// Represents the configuration for the Sprocket `run` command.
#[derive(Debug, Clone, PartialEq, Eq, Toml)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
pub struct RunConfig {
    /// The engine configuration.
    #[toml(default, flatten, with = flatten_any)]
    pub engine: EngineConfig,

    /// The output directory (default: `./out`).
    ///
    /// Individual runs are stored at `<output_dir>/runs/<target>/<timestamp>/`.
    #[toml(default = DEFAULT_OUTPUT_DIRECTORY.into())]
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
    #[toml(default = DEFAULT_EVENTS_CHANNEL_CAPACITY)]
    pub events_capacity: u32,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            engine: EngineConfig::default(),
            output_dir: DEFAULT_OUTPUT_DIRECTORY.into(),
            events_capacity: DEFAULT_EVENTS_CHANNEL_CAPACITY,
        }
    }
}

/// Server database configuration.
#[derive(Debug, Clone, PartialEq, Eq, Toml)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
pub struct ServerDatabaseConfig {
    /// Database URL (e.g., `sqlite://sprocket.db`). Defaults to `sprocket.db`
    /// in the output directory. in the output directory.
    #[toml(default = SENTINEL_DATABASE_FILENAME.into())]
    pub url: String,
}

impl Default for ServerDatabaseConfig {
    fn default() -> Self {
        Self {
            url: SENTINEL_DATABASE_FILENAME.into(),
        }
    }
}

/// Represents the maximum concurrent runs for the server.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub enum MaxConcurrentRuns {
    /// Do not limit the number of concurrent runs.
    #[default]
    Unlimited,
    /// Use the specified maximum number of concurrent runs.
    Limited(usize),
}

impl From<usize> for MaxConcurrentRuns {
    fn from(value: usize) -> Self {
        Self::Limited(value)
    }
}

impl From<Option<usize>> for MaxConcurrentRuns {
    fn from(value: Option<usize>) -> Self {
        match value {
            Some(value) => Self::Limited(value),
            None => Self::Unlimited,
        }
    }
}

impl From<MaxConcurrentRuns> for Option<usize> {
    fn from(value: MaxConcurrentRuns) -> Self {
        match value {
            MaxConcurrentRuns::Unlimited => None,
            MaxConcurrentRuns::Limited(value) => Some(value),
        }
    }
}

impl<'de> FromToml<'de> for MaxConcurrentRuns {
    fn from_toml(ctx: &mut toml_spanner::Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
        if let Some("unlimited") = item.as_str() {
            return Ok(Self::Unlimited);
        }

        if let Some(n) = item.as_u64().and_then(|n| usize::try_from(n).ok())
            && n > 0
        {
            return Ok(Self::Limited(n));
        }

        Err(ctx.report_custom_error(
            "expected a positive integer or `unlimited` for maximum concurrent runs",
            item,
        ))
    }
}

impl ToToml for MaxConcurrentRuns {
    fn to_toml<'a>(&'a self, _: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        match self {
            Self::Unlimited => Ok(Item::string("unlimited")),
            Self::Limited(n) => Ok(i64::try_from(*n)
                .map_err(|e| ToTomlError {
                    message: format!("invalid maximum concurrent runs: {e}").into(),
                })?
                .into()),
        }
    }
}

/// Server configuration.
#[derive(Debug, Clone, PartialEq, Eq, Toml)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
pub struct ServerConfig {
    /// Host to bind to.
    #[toml(default = DEFAULT_HOST.into())]
    pub host: String,
    /// Port to bind to.
    #[toml(default = DEFAULT_PORT)]
    pub port: u16,
    /// Allowed CORS origins.
    #[toml(default)]
    pub allowed_origins: Vec<String>,
    /// Database configuration.
    #[toml(default, style = Header)]
    pub database: ServerDatabaseConfig,
    /// Directory for workflow outputs.
    #[toml(default = DEFAULT_OUTPUT_DIRECTORY.into())]
    pub output_dir: PathBuf,
    /// Allowed file paths for file-based workflows.
    #[toml(default)]
    pub allowed_file_paths: Vec<PathBuf>,
    /// Allowed URL prefixes for URL-based workflows.
    #[toml(default)]
    pub allowed_urls: Vec<String>,
    /// Maximum concurrent workflows.
    #[toml(default)]
    pub max_concurrent_runs: MaxConcurrentRuns,
    /// The engine configuration to use during execution.
    #[toml(default)]
    pub engine: EngineConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: DEFAULT_HOST.into(),
            port: DEFAULT_PORT,
            allowed_origins: Vec::new(),
            database: ServerDatabaseConfig::default(),
            output_dir: DEFAULT_OUTPUT_DIRECTORY.into(),
            allowed_file_paths: Vec::new(),
            allowed_urls: Vec::new(),
            max_concurrent_runs: Default::default(),
            engine: EngineConfig::default(),
        }
    }
}

impl ServerConfig {
    /// Get the database URL.
    pub fn database_url(&self) -> String {
        if self.database.url == SENTINEL_DATABASE_FILENAME {
            self.output_dir
                .join(DEFAULT_DATABASE_FILENAME)
                .to_string_lossy()
                .to_string()
        } else {
            self.database.url.to_string()
        }
    }

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
        if let MaxConcurrentRuns::Limited(max) = self.max_concurrent_runs
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

        // Add file paths to allowed URLs with a file:// prefix
        let file_urls = self
            .allowed_file_paths
            .iter()
            .map(|p| {
                Url::from_file_path(p)
                    .map_err(|_| {
                        anyhow::anyhow!(
                            "failed to convert allowed file path to file:// URL: `{}`",
                            p.display()
                        )
                    })
                    .map(|u| u.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Add file URLs to allowed file paths
        let file_paths = self
            .allowed_urls
            .iter()
            .filter_map(|u| match Url::parse(u) {
                Ok(url) => match url.scheme() == "file" {
                    true => match url.to_file_path() {
                        Ok(path) => Some(Ok(path)),
                        Err(_) => Some(Err(anyhow::anyhow!(
                            "failed to convert allowed URL to file path: `{}`",
                            u
                        ))),
                    },
                    false => None,
                },
                Err(e) => Some(Err(anyhow::anyhow!(
                    "failed to parse allowed URL `{}`: {e}",
                    u
                ))),
            })
            .collect::<Result<Vec<_>, _>>()?;

        self.allowed_file_paths.extend(file_paths);
        self.allowed_urls.extend(file_urls);

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
#[derive(Debug, Clone, PartialEq, Eq, Toml)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
pub struct TestConfig {
    /// Number of test executions to run in parallel.
    ///
    /// The default is `50`.
    #[toml(default = DEFAULT_TEST_PARALLELISM)]
    pub parallelism: u32,
    /// Delay between submitting initial test executions, in milliseconds.
    ///
    /// Once the `parallelism`` permits are exhausted, this throttle delay is
    /// ignored and new tests are submitted eagerly as prior tests complete and
    /// free permits.
    ///
    /// The default is `100` milliseconds.
    #[toml(default = DEFAULT_TEST_THROTTLE)]
    pub throttle: u64,
    /// Directory containing test fixture files.
    ///
    /// If not set, fixtures are resolved from `<workspace>/test/fixtures`.
    pub fixtures_dir: Option<PathBuf>,
    /// Directory to use for executing tests.
    ///
    /// If not set, runs are written to `<workspace>/test/runs`.
    pub run_dir: Option<PathBuf>,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            parallelism: DEFAULT_TEST_PARALLELISM,
            throttle: DEFAULT_TEST_THROTTLE,
            fixtures_dir: None,
            run_dir: None,
        }
    }
}

/// Sentinel value used throughout `DocConfig`.
const SENTINEL_DOC_CONFIG_VALUE: &str = "none";

/// `doc` command configuration.
#[derive(Debug, Clone, PartialEq, Eq, Toml)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
pub struct DocConfig {
    /// Path to a Markdown file to embed in the `<output>/index.html` file.
    #[toml(default = SENTINEL_DOC_CONFIG_VALUE.into())]
    pub index_page: String,
    /// Path to an SVG logo to embed on each page.
    ///
    /// If not supplied, the default Sprocket logo will be used.
    #[toml(default = SENTINEL_DOC_CONFIG_VALUE.into())]
    pub logo: String,
    /// Path to an alternate light mode SVG logo to embed on each page.
    ///
    /// If not supplied, the `logo` SVG will be used; or if that is also not
    /// supplied, the default Sprocket logo will be used.
    #[toml(default = SENTINEL_DOC_CONFIG_VALUE.into())]
    pub alt_light_logo: String,
    /// An optional link to the project's homepage.
    #[toml(default = SENTINEL_DOC_CONFIG_VALUE.into())]
    pub homepage_url: String,
    /// An optional link to the project's GitHub repository.
    #[toml(default = SENTINEL_DOC_CONFIG_VALUE.into())]
    pub github_url: String,
    /// Initialize pages in light mode instead of the default dark mode.
    #[toml(default)]
    pub light_mode: bool,
    /// Enables support for documentation comments
    ///
    /// This option is *experimental*. Follow the pre-RFC discussion here: <https://github.com/openwdl/wdl/issues/757>.
    #[toml(default)]
    pub with_doc_comments: bool,
    /// Configuration for custom HTML to embed in generated pages.
    #[toml(default, style = Header)]
    pub extra_html: DocExtraHtmlConfig,
}

impl Default for DocConfig {
    fn default() -> Self {
        Self {
            index_page: SENTINEL_DOC_CONFIG_VALUE.into(),
            logo: SENTINEL_DOC_CONFIG_VALUE.into(),
            alt_light_logo: SENTINEL_DOC_CONFIG_VALUE.into(),
            homepage_url: SENTINEL_DOC_CONFIG_VALUE.into(),
            github_url: SENTINEL_DOC_CONFIG_VALUE.into(),
            light_mode: false,
            with_doc_comments: false,
            extra_html: DocExtraHtmlConfig::default(),
        }
    }
}

impl DocConfig {
    /// Get the path to the Markdown file to be embedded in the root index page,
    /// if configured.
    pub fn index_page(&self) -> Option<PathBuf> {
        if self.index_page == SENTINEL_DOC_CONFIG_VALUE {
            None
        } else {
            Some(PathBuf::from(&self.index_page))
        }
    }

    /// Get the path to the logo file, if configured.
    pub fn logo(&self) -> Option<PathBuf> {
        if self.logo == SENTINEL_DOC_CONFIG_VALUE {
            None
        } else {
            Some(PathBuf::from(&self.logo))
        }
    }

    /// Get the path to the alternate light mode logo file, if configured.
    pub fn alt_light_logo(&self) -> Option<PathBuf> {
        if self.alt_light_logo == SENTINEL_DOC_CONFIG_VALUE {
            None
        } else {
            Some(PathBuf::from(&self.alt_light_logo))
        }
    }

    /// Get the URL to the project's homepage, if configured.
    pub fn homepage_url(&self) -> Option<Url> {
        if self.homepage_url == SENTINEL_DOC_CONFIG_VALUE {
            None
        } else {
            Some(Url::from_str(&self.homepage_url).expect("validated already"))
        }
    }

    /// Get the URL to the project's GitHub, if configured.
    pub fn github_url(&self) -> Option<Url> {
        if self.github_url == SENTINEL_DOC_CONFIG_VALUE {
            None
        } else {
            Some(Url::from_str(&self.github_url).expect("validated already"))
        }
    }

    /// Validates the configuration.
    pub fn validate(&self) -> Result<()> {
        if self.homepage_url != SENTINEL_DOC_CONFIG_VALUE {
            match Url::from_str(&self.homepage_url) {
                Ok(_) => Ok(()),
                Err(e) => Err(anyhow!("error while parsing configured homepage URL: {e}")),
            }?;
        }
        if self.github_url != SENTINEL_DOC_CONFIG_VALUE {
            match Url::from_str(&self.github_url) {
                Ok(_) => Ok(()),
                Err(e) => Err(anyhow!("error while parsing configured GitHub URL: {e}")),
            }?;
        }
        Ok(())
    }
}

/// `doc.extra_html` command configuration.
#[derive(Debug, Clone, PartialEq, Eq, Toml)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
pub struct DocExtraHtmlConfig {
    /// Path to an HTML file that should have its contents embedded in each HTML
    /// page, immediately before the closing `<head>` tag.
    #[toml(default = SENTINEL_DOC_CONFIG_VALUE.into())]
    pub head: String,
    /// Path to an HTML file that should have its contents embedded in each HTML
    /// page, immediately after the opening `<body>` tag.
    #[toml(default = SENTINEL_DOC_CONFIG_VALUE.into())]
    pub body_open: String,
    /// Path to an HTML file that should have its contents embedded in each HTML
    /// page, immediately before the closing `<body>` tag.
    #[toml(default = SENTINEL_DOC_CONFIG_VALUE.into())]
    pub body_close: String,
}

impl DocExtraHtmlConfig {
    /// Get the path to the head open HTML file, if configured.
    pub fn head(&self) -> Option<PathBuf> {
        if self.head == SENTINEL_DOC_CONFIG_VALUE {
            None
        } else {
            Some(PathBuf::from(&self.head))
        }
    }

    /// Get the path to the body open HTML file, if configured.
    pub fn body_open(&self) -> Option<PathBuf> {
        if self.body_open == SENTINEL_DOC_CONFIG_VALUE {
            None
        } else {
            Some(PathBuf::from(&self.body_open))
        }
    }

    /// Get the path to the body close HTML file, if configured.
    pub fn body_close(&self) -> Option<PathBuf> {
        if self.body_close == SENTINEL_DOC_CONFIG_VALUE {
            None
        } else {
            Some(PathBuf::from(&self.body_close))
        }
    }
}

impl Default for DocExtraHtmlConfig {
    fn default() -> Self {
        Self {
            head: SENTINEL_DOC_CONFIG_VALUE.into(),
            body_open: SENTINEL_DOC_CONFIG_VALUE.into(),
            body_close: SENTINEL_DOC_CONFIG_VALUE.into(),
        }
    }
}

impl Config {
    /// Create a new config instance by reading potential configurations.
    ///
    /// This will try to read and build each configuration source eagerly in
    /// order to provide clear error diagnostics complete with path information.
    /// This requires intermediate clones and disk reads that could be avoided.
    pub fn new<'a>(
        paths: impl IntoIterator<Item = &'a Path>,
        skip_config_search: bool,
    ) -> Result<Self, wdl::engine::config::BuilderError> {
        let mut builder = Config::builder();

        if !skip_config_search {
            // Start with a configuration file next to the `sprocket` executable
            if let Ok(path) = std::env::current_exe()
                && let Some(parent) = path.parent()
            {
                let path = parent.join(CONFIG_FILENAME);
                if path.exists() {
                    debug!("using configuration from `{path}`", path = path.display());
                    builder = builder.with_file_source(path);
                }
            }

            // Check the user-level Sprocket config directory.
            if let Some(dir) = config_root() {
                let path = dir.join(CONFIG_FILENAME);
                if path.exists() {
                    debug!("using configuration from `{path}`", path = path.display());
                    builder = builder.with_file_source(path);
                }
            }

            // Check PWD for a config file
            let path = Path::new(CONFIG_FILENAME);
            if path.exists() {
                debug!("using configuration from `{path}`", path = path.display());
                builder = builder.with_file_source(path);
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
                    debug!("using configuration from `{path}`", path = path.display());
                    builder = builder.with_file_source(path);
                }
            }
        }

        // Merge the given files
        for path in paths {
            debug!("using configuration from `{path}`", path = path.display());
            builder = builder.with_file_source(path);
        }

        builder.try_build()
    }

    /// Validate a configuration.
    pub fn validate(&mut self) -> Result<()> {
        if self.check.lint.is_some() {
            bail!(
                "`[check.lint]` has been replaced by per-rule tables\n- move `allowed_names` \
                 under `[check.rules.SnakeCase]` and/or `[check.rules.DeclarationName]` (it \
                 previously applied to both)\n- move `allowed_runtime_keys` under \
                 `[check.rules.ExpectedRuntimeKeys]`"
            )
        }

        if self.check.all_lint_rules && !self.check.only_lint_tags.is_empty() {
            bail!("`all_lint_rules` cannot be specified with `only_lint_tags`")
        }

        if self.run.events_capacity == 0 {
            bail!("`events_capacity` must be at least 1")
        }

        // Shell-expand certain paths
        fn expand(path: &Path) -> Result<PathBuf> {
            match shellexpand::path::full(path) {
                Ok(expanded) => Ok(PathBuf::from(expanded)),
                Err(e) => {
                    bail!(
                        "failed to expand `{}` in path `{}`: {}",
                        e.var_name.to_string_lossy(),
                        path.display(),
                        e.cause
                    );
                }
            }
        }
        self.run.output_dir = expand(&self.run.output_dir)?;
        self.server.output_dir = expand(&self.server.output_dir)?;
        self.run.engine.task.cache_dir = match self
            .run
            .engine
            .task
            .cache_dir()
            .map(|p| expand(&p).map(|p| p.to_string_lossy().to_string()))
            .transpose()?
        {
            Some(s) => s,
            None => self.run.engine.task.cache_dir.clone(),
        };
        if !self.run.engine.http.using_system_cache_dir() {
            self.run.engine.http.cache_dir = self
                .run
                .engine
                .http
                .cache_dir()
                .map(|p| expand(&p).map(|p| p.to_string_lossy().to_string()))??;
        }
        self.server.engine.task.cache_dir = match self
            .server
            .engine
            .task
            .cache_dir()
            .map(|p| expand(&p).map(|p| p.to_string_lossy().to_string()))
            .transpose()?
        {
            Some(s) => s,
            None => self.server.engine.task.cache_dir.clone(),
        };
        if !self.server.engine.http.using_system_cache_dir() {
            self.server.engine.http.cache_dir = self
                .server
                .engine
                .http
                .cache_dir()
                .map(|p| expand(&p).map(|p| p.to_string_lossy().to_string()))??;
        }

        // Validate inner configs
        self.server.validate()?;
        self.doc.validate()?;

        Ok(())
    }

    /// Read a configuration file from the specified path.
    pub fn read_config(path: &str) -> Result<Self> {
        let data = std::fs::read(path).context("failed to open config file")?;
        let text = String::from_utf8(data).expect("failed to read config file");
        let config: Config =
            toml_spanner::from_str(text.as_str()).context("failed to parse config file")?;
        Ok(config)
    }

    /// Write a configuration to the specified path.
    pub fn write_config(&self, path: &str) -> Result<()> {
        let data = toml_spanner::to_string(self).context("failed to serialize config")?;
        std::fs::write(path, data).context("failed to write config file")
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn parses_check_rules() {
        let config: Config = toml_spanner::from_str(
            "[check.rules.NamingConvention]\nseverity = \"error\"\nallowed_names = [\"GATK\"]\n",
        )
        .unwrap();
        assert_eq!(
            config
                .check
                .rules
                .lint_config()
                .severity_override("NamingConvention"),
            Some(wdl::lint::RuleSeverity::Error)
        );
    }

    #[test]
    fn deprecated_rule_alias_is_canonicalized() {
        // `[check.rules.SnakeCase]` maps to the `NamingConvention` rule.
        let config: Config =
            toml_spanner::from_str("[check.rules.SnakeCase]\nseverity = \"off\"\n").unwrap();
        assert_eq!(
            config
                .check
                .rules
                .lint_config()
                .severity_override("NamingConvention"),
            Some(wdl::lint::RuleSeverity::Off)
        );
    }

    #[test]
    fn rejects_unknown_rule() {
        let err = toml_spanner::from_str::<Config>("[check.rules.SnkaeCase]\nseverity = \"off\"\n")
            .unwrap_err();
        assert!(err.to_string().contains("unknown rule"), "{err}");
    }

    #[test]
    fn rejects_inapplicable_parameter() {
        let err = toml_spanner::from_str::<Config>("[check.rules.SnakeCase]\nmax_length = 5\n")
            .unwrap_err();
        assert!(
            err.to_string().contains("not a configurable parameter"),
            "{err}"
        );
    }

    #[test]
    fn collects_analysis_severity_overrides() {
        let config: Config =
            toml_spanner::from_str("[check.rules.UnusedImport]\nseverity = \"error\"\n").unwrap();
        let overrides = config.check.rules.analysis_severity_overrides();
        assert_eq!(overrides.get("UnusedImport"), Some(&Some(Severity::Error)));
    }

    #[test]
    fn legacy_lint_table_reports_migration_error() {
        let mut config: Config =
            toml_spanner::from_str("[check.lint]\nallowed_names = [\"x\"]\n").unwrap();
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("has been replaced"), "{err}");
    }

    #[test]
    fn max_concurrent_runs_serialization() {
        let map: HashMap<&str, MaxConcurrentRuns> =
            HashMap::from_iter([("value", MaxConcurrentRuns::Unlimited)]);
        assert_eq!(
            toml_spanner::to_string(&map).unwrap(),
            format!("value = \"unlimited\"\n")
        );

        let map: HashMap<&str, MaxConcurrentRuns> =
            HashMap::from_iter([("value", MaxConcurrentRuns::Limited(123))]);
        assert_eq!(toml_spanner::to_string(&map).unwrap(), "value = 123\n");
    }

    #[test]
    fn max_concurrent_runs_deserialization() {
        let map: HashMap<String, MaxConcurrentRuns> =
            toml_spanner::from_str("value = 'unlimited'").unwrap();
        assert_eq!(map["value"], MaxConcurrentRuns::Unlimited);

        let map: HashMap<String, MaxConcurrentRuns> = toml_spanner::from_str("value = 12").unwrap();
        assert_eq!(map["value"], MaxConcurrentRuns::Limited(12));

        let expected_error =
            "expected a positive integer or `unlimited` for maximum concurrent runs at `value`";

        let error = toml_spanner::from_str::<HashMap<String, MaxConcurrentRuns>>("value = 'wrong'")
            .unwrap_err();
        assert_eq!(error.to_string(), expected_error);

        let error =
            toml_spanner::from_str::<HashMap<String, MaxConcurrentRuns>>("value = 0").unwrap_err();
        assert_eq!(error.to_string(), expected_error);

        let error = toml_spanner::from_str::<HashMap<String, MaxConcurrentRuns>>("value = -10")
            .unwrap_err();
        assert_eq!(error.to_string(), expected_error);
    }
}
