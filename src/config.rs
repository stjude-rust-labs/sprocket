//! Implementation of the configuration module.

use std::env;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use clap::ValueEnum;
use serde::Deserialize;
use serde::Serialize;
use tracing::debug;
use tracing::warn;
use url::Url;
use wdl::ast::SupportedVersion;
use wdl::engine::Config as EngineConfig;
use wdl::engine::nullable_config_type;
use wdl::format::Config as FormatConfig;

use crate::diagnostics::Mode;

/// Default host.
const DEFAULT_HOST: &str = "127.0.0.1";

/// Default port.
const DEFAULT_PORT: u16 = 8080;

/// Default database filename.
pub const DEFAULT_DATABASE_FILENAME: &str = "sprocket.db";

/// Sentinel value for using a local database.
const SENTINEL_DATABASE_FILENAME: &str = "default";

/// Helper for `serde`.
fn get_sentinel_database_name() -> String {
    SENTINEL_DATABASE_FILENAME.to_string()
}

/// Default output directory.
pub const DEFAULT_OUTPUT_DIRECTORY: &str = "./out";

/// The name of the Sprocket configuration file.
const CONFIG_FILENAME: &str = "sprocket.toml";

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
const DEFAULT_EVENTS_CHANNEL_CAPACITY: usize = 5000;

/// Default output directory function for serde.
fn default_output_dir() -> PathBuf {
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
    /// Configuration for the `doc` command.
    pub doc: DocConfig,
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
    pub wdl: WdlConfig,
}

nullable_config_type!(
    FallbackVersion,
    SupportedVersion,
    "none",
    value,
    true,
    "a supported version",
    None
);

/// WDL-specific configuration options shared across all commands.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct WdlConfig {
    /// The fallback version to use when a WDL document declares an
    /// unrecognized version (e.g., `version development`).
    pub fallback_version: FallbackVersion,
}

/// Represents the configuration for the Sprocket `check` and `lint` commands.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct CheckConfig {
    /// Rule IDs to except from running.
    #[serde(default)]
    pub except: Vec<String>,
    /// Causes the command to fail if any warnings are reported.
    pub deny_warnings: bool,
    /// Causes the command to fail if any notes or warnings are reported.
    pub deny_notes: bool,
    /// Hide diagnostics with `note` severity.
    pub hide_notes: bool,
    /// Hide diagnostics with `warning` and `note` severity.
    pub hide_warnings: bool,
    /// Enable all lint rules, even those outside the default set.
    ///
    /// This cannot be `true` while `only_lint_tags` is populated.
    pub all_lint_rules: bool,
    /// Set of lint tags to opt into. Leave this empty to use the default set of
    /// tags.
    #[serde(default)]
    pub only_lint_tags: Vec<String>,
    /// Set of lint tags to filter out of the enabled lint rules.
    #[serde(default)]
    pub filter_lint_tags: Vec<String>,
    /// Path to the diagnostic baseline file.
    #[serde(default)]
    pub baseline: Option<PathBuf>,
    /// Lint rule configuration.
    #[serde(default)]
    pub lint: wdl::lint::Config,
}

/// Represents the configuration for the Sprocket `analyzer` command.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct AnalyzerConfig {
    /// Whether to enable lint rules.
    pub lint: bool,
    /// Rule IDs to except from running.
    #[serde(default)]
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
    #[serde(default = "default_output_dir")]
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
    pub events_capacity: usize,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            engine: EngineConfig::default(),
            output_dir: default_output_dir(),
            events_capacity: DEFAULT_EVENTS_CHANNEL_CAPACITY,
        }
    }
}

/// Server database configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ServerDatabaseConfig {
    /// Database URL (e.g., `sqlite://sprocket.db`). Defaults to `sprocket.db`
    /// in the output directory. in the output directory.
    #[serde(default = "get_sentinel_database_name")]
    pub url: String,
}

impl Default for ServerDatabaseConfig {
    fn default() -> Self {
        Self {
            url: get_sentinel_database_name(),
        }
    }
}

nullable_config_type!(
    MaxConcurrentRuns,
    usize,
    "unlimited",
    value,
    value > 0,
    "a positive number",
    None
);

/// Server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ServerConfig {
    /// Host to bind to.
    pub host: String,
    /// Port to bind to.
    pub port: u16,
    /// Allowed CORS origins.
    #[serde(default)]
    pub allowed_origins: Vec<String>,
    /// Database configuration.
    #[serde(default)]
    pub database: ServerDatabaseConfig,
    /// Directory for workflow outputs.
    #[serde(default = "default_output_dir")]
    pub output_dir: PathBuf,
    /// Allowed file paths for file-based workflows.
    #[serde(default)]
    pub allowed_file_paths: Vec<PathBuf>,
    /// Allowed URL prefixes for URL-based workflows.
    #[serde(default)]
    pub allowed_urls: Vec<String>,
    /// Maximum concurrent workflows.
    pub max_concurrent_runs: MaxConcurrentRuns,
    /// The engine configuration to use during execution.
    pub engine: EngineConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: String::from(DEFAULT_HOST),
            port: DEFAULT_PORT,
            allowed_origins: Vec::new(),
            database: ServerDatabaseConfig::default(),
            output_dir: default_output_dir(),
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
        if let Some(max) = self.max_concurrent_runs.inner()
            && *max == 0
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
    pub parallelism: usize,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self { parallelism: 50 }
    }
}

/// Sentinel value used throughout `DocConfig`.
const SENTINEL_DOC_CONFIG_VALUE: &str = "none";

/// serde helper.
fn get_sentinel_doc_config_value() -> String {
    SENTINEL_DOC_CONFIG_VALUE.to_string()
}

/// `doc` command configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct DocConfig {
    /// Path to a Markdown file to embed in the `<output>/index.html` file.
    #[serde(default = "get_sentinel_doc_config_value")]
    pub index_page: String,
    /// Path to an SVG logo to embed on each page.
    ///
    /// If not supplied, the default Sprocket logo will be used.
    #[serde(default = "get_sentinel_doc_config_value")]
    pub logo: String,
    /// Path to an alternate light mode SVG logo to embed on each page.
    ///
    /// If not supplied, the `logo` SVG will be used; or if that is also not
    /// supplied, the default Sprocket logo will be used.
    #[serde(default = "get_sentinel_doc_config_value")]
    pub alt_light_logo: String,
    /// An optional link to the project's homepage.
    #[serde(default = "get_sentinel_doc_config_value")]
    pub homepage_url: String,
    /// An optional link to the project's GitHub repository.
    #[serde(default = "get_sentinel_doc_config_value")]
    pub github_url: String,
    /// Initialize pages in light mode instead of the default dark mode.
    pub light_mode: bool,
    /// Enables support for documentation comments
    ///
    /// This option is *experimental*. Follow the pre-RFC discussion here: <https://github.com/openwdl/wdl/issues/757>.
    pub with_doc_comments: bool,
    /// Configuration for custom HTML to embed in generated pages.
    #[serde(default)]
    pub extra_html: DocExtraHtmlConfig,
}

impl Default for DocConfig {
    fn default() -> Self {
        Self {
            index_page: get_sentinel_doc_config_value(),
            logo: get_sentinel_doc_config_value(),
            alt_light_logo: get_sentinel_doc_config_value(),
            homepage_url: get_sentinel_doc_config_value(),
            github_url: get_sentinel_doc_config_value(),
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct DocExtraHtmlConfig {
    /// Path to an HTML file that should have its contents embedded in each HTML
    /// page, immediately before the closing `<head>` tag.
    #[serde(default = "get_sentinel_doc_config_value")]
    pub head: String,
    /// Path to an HTML file that should have its contents embedded in each HTML
    /// page, immediately after the opening `<body>` tag.
    #[serde(default = "get_sentinel_doc_config_value")]
    pub body_open: String,
    /// Path to an HTML file that should have its contents embedded in each HTML
    /// page, immediately before the closing `<body>` tag.
    #[serde(default = "get_sentinel_doc_config_value")]
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
            head: get_sentinel_doc_config_value(),
            body_open: get_sentinel_doc_config_value(),
            body_close: get_sentinel_doc_config_value(),
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
    ) -> Result<Self> {
        let mut builder = config::Config::builder().add_source(
            config::Config::try_from(&Config::default()).expect("default should serialize"),
        );

        if !skip_config_search {
            // Start with a configuration file next to the `sprocket` executable
            if let Ok(path) = std::env::current_exe()
                && let Some(parent) = path.parent()
            {
                let path = parent.join(CONFIG_FILENAME);
                if path.exists() {
                    debug!("reading configuration from `{path}`", path = path.display());
                    builder = builder.add_source(config::File::from(path.as_path()));
                    let _ = builder
                        .build_cloned()
                        .with_context(|| format!("reading `{path}`", path = path.display()))?
                        .try_deserialize::<Config>()
                        .with_context(|| format!("parsing `{path}`", path = path.display()))?;
                }
            }

            // Check XDG_CONFIG_HOME for a config file
            // On MacOS, check HOME for a config file
            #[cfg(target_os = "macos")]
            let dir = dirs::home_dir().map(|p| p.join(".config"));
            #[cfg(not(target_os = "macos"))]
            let dir = dirs::config_dir();

            if let Some(dir) = dir {
                let path = dir.join("sprocket").join(CONFIG_FILENAME);
                if path.exists() {
                    debug!("reading configuration from `{path}`", path = path.display());
                    builder = builder.add_source(config::File::from(path.as_path()));
                    let _ = builder
                        .build_cloned()
                        .with_context(|| format!("reading `{path}`", path = path.display()))?
                        .try_deserialize::<Config>()
                        .with_context(|| format!("parsing `{path}`", path = path.display()))?;
                }
            }

            // Check PWD for a config file
            let path = Path::new(CONFIG_FILENAME);
            if path.exists() {
                debug!("reading configuration from `{path}`", path = path.display());
                builder = builder.add_source(config::File::from(path));
                let _ = builder
                    .build_cloned()
                    .with_context(|| format!("reading `{path}`", path = path.display()))?
                    .try_deserialize::<Config>()
                    .with_context(|| format!("parsing `{path}`", path = path.display()))?;
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
                    debug!("reading configuration from `{path}`", path = path.display());
                    builder = builder.add_source(config::File::from(path));
                    let _ = builder
                        .build_cloned()
                        .with_context(|| format!("reading `{path}`", path = path.display()))?
                        .try_deserialize::<Config>()
                        .with_context(|| format!("parsing `{path}`", path = path.display()))?;
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
            debug!("reading configuration from `{path}`", path = path.display());
            builder = builder.add_source(config::File::from(path));
            let _ = builder
                .build_cloned()
                .with_context(|| format!("reading `{path}`", path = path.display()))?
                .try_deserialize::<Config>()
                .with_context(|| format!("parsing `{path}`", path = path.display()))?;
        }

        builder
            .build()
            .context("failed to read configuration sources")?
            .try_deserialize()
            .context("failed to merge configuration sources")
    }

    /// Validate a configuration.
    pub fn validate(&mut self) -> Result<()> {
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
            toml::from_str(text.as_str()).context("failed to parse config file")?;
        Ok(config)
    }

    /// Write a configuration to the specified path.
    pub fn write_config(&self, path: &str) -> Result<()> {
        let data = toml::to_string(self).context("failed to serialize config")?;
        std::fs::write(path, data).context("failed to write config file")
    }
}
