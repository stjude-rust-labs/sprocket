//! Execution configuration.

use std::path::PathBuf;

use anyhow::Context as _;
use bon::Builder;
use serde::Deserialize;
use serde::Serialize;
use url::Url;
use wdl::engine::Config as EngineConfig;

/// Default output directory.
const DEFAULT_OUTPUT_DIRECTORY: &str = "./out";

/// Default output directory function for serde.
fn default_output_directory() -> PathBuf {
    PathBuf::from(DEFAULT_OUTPUT_DIRECTORY)
}

/// Configuration validation errors.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// File does not exist.
    #[error("file `{0}` does not exist")]
    FileNotFound(PathBuf),

    /// File path not in allowed file paths.
    #[error("file path `{0}` is not in an allowed directory")]
    FilePathForbidden(PathBuf),

    /// URL not in allowed URLs.
    #[error("url `{0}` does not have an allowed prefix")]
    UrlForbidden(Url),

    /// Failed to canonicalize file path.
    #[error("failed to canonicalize path `{0}`")]
    FailedToCanonicalize(PathBuf),
}

/// Result type for configuration operations.
pub type ConfigResult<T> = std::result::Result<T, ConfigError>;

/// Execution configuration.
#[derive(Builder, Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfig {
    /// Directory for workflow outputs (default: `./out`).
    #[serde(default = "default_output_directory")]
    #[builder(default = default_output_directory())]
    pub output_directory: PathBuf,
    /// Allowed file paths for file-based workflows.
    #[serde(default)]
    #[builder(default)]
    pub allowed_file_paths: Vec<PathBuf>,
    /// Allowed URL prefixes for URL-based workflows.
    #[serde(default)]
    #[builder(default)]
    pub allowed_urls: Vec<String>,
    /// Maximum concurrent workflows (default: `None`).
    ///
    /// `None` means there is no limit on the number of executions.
    #[serde(default)]
    pub max_concurrent_runs: Option<usize>,
    /// The engine configuration to use during execution.
    #[serde(flatten, default)]
    #[builder(default)]
    pub engine: EngineConfig,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            output_directory: default_output_directory(),
            allowed_file_paths: vec![],
            allowed_urls: vec![],
            max_concurrent_runs: None,
            engine: EngineConfig::default(),
        }
    }
}

impl ExecutionConfig {
    /// Validates and normalizes the execution configuration.
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
