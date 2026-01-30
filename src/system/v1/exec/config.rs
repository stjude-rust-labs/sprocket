//! Execution configuration error types.

use std::path::PathBuf;

use url::Url;

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
