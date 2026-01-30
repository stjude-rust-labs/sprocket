//! Validated workflow sources.

use std::path::Path;
use std::path::PathBuf;

use url::Url;

use super::ConfigError;
use super::ConfigResult;
use crate::config::ServerConfig;

/// A validated workflow source.
///
/// This enum represents a workflow source that has been validated against the
/// execution configuration to prevent:
///
/// - **Path traversal attacks.** File paths are canonicalized and checked
///   against allowed directories using prefix matching.
/// - **Information leakage.** File existence is only revealed for paths within
///   allowed directories.
/// - **URL restriction.** URLs must match configured prefixes exactly,
///   including scheme.
///
/// # Security Invariants
///
/// Once constructed, an [`AllowedSource`] guarantees:
///
/// - File paths are absolute, canonical, and within allowed directories
/// - File paths contain valid UTF-8
/// - URLs match at least one configured prefix
#[derive(Debug, Clone)]
pub enum AllowedSource {
    /// A URL source that has been validated against allowed URL prefixes.
    Url(Url),
    /// A file path that has been shell-expanded, canonicalized, and validated
    /// against allowed file paths.
    File(PathBuf),
}

impl AllowedSource {
    /// Validates a source path against the server configuration.
    ///
    /// # Preconditions
    ///
    /// The configuration must have been validated via
    /// `ServerConfig::validate()` which ensures all allowed paths are
    /// canonical.
    pub fn validate(source: &str, config: &ServerConfig) -> ConfigResult<Self> {
        if let Ok(url) = Url::parse(source) {
            let url_str = url.as_str();
            let is_allowed = config
                .allowed_urls
                .iter()
                .any(|prefix| url_str.starts_with(prefix));

            if !is_allowed {
                return Err(ConfigError::UrlForbidden(url));
            }

            Ok(AllowedSource::Url(url))
        } else {
            let expanded = shellexpand::tilde(source);
            let path = Path::new(expanded.as_ref());

            let Ok(canonical_path) = path.canonicalize() else {
                if let Some(parent) = path.parent()
                    && let Ok(parent_canonical) = parent.canonicalize()
                    && let Some(filename) = path.file_name()
                {
                    let would_be_path = parent_canonical.join(filename);
                    let is_allowed = config
                        .allowed_file_paths
                        .iter()
                        .any(|allowed| would_be_path.starts_with(allowed));

                    if is_allowed {
                        return Err(if path.exists() {
                            ConfigError::FailedToCanonicalize(path.to_path_buf())
                        } else {
                            ConfigError::FileNotFound(path.to_path_buf())
                        });
                    }
                }
                return Err(ConfigError::FilePathForbidden(path.to_path_buf()));
            };

            // Check to make sure the path is valid UTF-8.
            canonical_path.to_str().expect("path is not UTF-8");

            // Check to make sure the path is allowed.
            let is_allowed = config
                .allowed_file_paths
                .iter()
                .any(|allowed| canonical_path.starts_with(allowed));

            if !is_allowed {
                return Err(ConfigError::FilePathForbidden(canonical_path));
            }

            Ok(AllowedSource::File(canonical_path))
        }
    }

    /// Returns a reference to the URL if this is an [`AllowedSource::Url`].
    pub fn as_url(&self) -> Option<&Url> {
        match self {
            AllowedSource::Url(url) => Some(url),
            AllowedSource::File(_) => None,
        }
    }

    /// Consumes self and returns the URL if this is an [`AllowedSource::Url`].
    pub fn into_url(self) -> Option<Url> {
        match self {
            AllowedSource::Url(url) => Some(url),
            AllowedSource::File(_) => None,
        }
    }

    /// Returns a reference to the file path if this is an
    /// [`AllowedSource::File`].
    pub fn as_file_path(&self) -> Option<&Path> {
        match self {
            AllowedSource::Url(_) => None,
            AllowedSource::File(path) => Some(path),
        }
    }

    /// Consumes self and returns the file path if this is an
    /// [`AllowedSource::File`].
    pub fn into_file_path(self) -> Option<PathBuf> {
        match self {
            AllowedSource::Url(_) => None,
            AllowedSource::File(path) => Some(path),
        }
    }

    /// Returns the source as a string slice.
    ///
    /// For [`AllowedSource::Url`]s, this returns the URL string.  For file
    /// paths, this returns the path as a string.
    pub fn as_str(&self) -> &str {
        match self {
            AllowedSource::Url(url) => url.as_str(),
            AllowedSource::File(path) => {
                // SAFETY: path was checked to ensure valid UTF-8 at creation.
                path.to_str().expect("path should be valid UTF-8")
            }
        }
    }

    /// Converts the source to a URL.
    ///
    /// For [`AllowedSource::Url`]s, this clones the URL. For file paths, this
    /// converts the path to a `file://` URL.
    pub fn to_url(&self) -> Url {
        match self {
            AllowedSource::Url(url) => url.clone(),
            AllowedSource::File(path) => {
                // SAFETY: path is absolute (canonicalized at creation).
                Url::from_file_path(path).expect("file path should convert to URL")
            }
        }
    }
}

impl std::fmt::Display for AllowedSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AllowedSource::Url(url) => write!(f, "{}", url),
            AllowedSource::File(path) => write!(f, "{}", path.display()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(allowed_file_paths: Vec<PathBuf>, allowed_urls: Vec<String>) -> ServerConfig {
        ServerConfig {
            allowed_file_paths,
            allowed_urls,
            ..Default::default()
        }
    }

    #[test]
    fn validate_url_allowed() {
        let mut config = make_config(
            vec![],
            vec![
                String::from("https://example.com/"),
                String::from("http://localhost/"),
            ],
        );
        config.validate().unwrap();

        let result = AllowedSource::validate("https://example.com/workflow.wdl", &config);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), AllowedSource::Url(_)));
    }

    #[test]
    fn validate_url_forbidden() {
        let mut config = make_config(vec![], vec![String::from("https://example.com/")]);
        config.validate().unwrap();

        let result = AllowedSource::validate("https://forbidden.com/workflow.wdl", &config);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::UrlForbidden(_)));
    }

    #[test]
    fn validate_file_allowed() {
        use std::fs::File;

        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("workflow.wdl");
        File::create(&file_path).unwrap();

        let config = make_config(vec![temp_dir.path().canonicalize().unwrap()], vec![]);

        let result = AllowedSource::validate(file_path.to_str().unwrap(), &config);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), AllowedSource::File(_)));
    }

    #[test]
    fn validate_file_forbidden() {
        use std::fs::File;

        use tempfile::TempDir;

        let allowed_dir = TempDir::new().unwrap();
        let forbidden_dir = TempDir::new().unwrap();

        // Create a file in the forbidden directory
        let existing_file = forbidden_dir.path().join("workflow.wdl");
        File::create(&existing_file).unwrap();

        // Also test with non-existent file in forbidden directory
        let nonexistent_file = forbidden_dir.path().join("missing.wdl");

        let config = make_config(vec![allowed_dir.path().canonicalize().unwrap()], vec![]);

        // Both should return `FilePathForbidden` without leaking existence info
        let result1 = AllowedSource::validate(existing_file.to_str().unwrap(), &config);
        assert!(matches!(
            result1.unwrap_err(),
            ConfigError::FilePathForbidden(_)
        ));

        let result2 = AllowedSource::validate(nonexistent_file.to_str().unwrap(), &config);
        assert!(matches!(
            result2.unwrap_err(),
            ConfigError::FilePathForbidden(_)
        ));
    }

    #[test]
    fn validate_file_not_found() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let nonexistent = temp_dir.path().join("missing.wdl");

        let config = make_config(vec![temp_dir.path().canonicalize().unwrap()], vec![]);

        // Should reveal `FileNotFound` since it's in an allowed directory
        let result = AllowedSource::validate(nonexistent.to_str().unwrap(), &config);
        assert!(matches!(result.unwrap_err(), ConfigError::FileNotFound(_)));
    }

    #[test]
    fn validate_url_scheme_must_match() {
        let config = make_config(vec![], vec![String::from("https://example.com/")]);

        // http should not be allowed when only https is configured
        let result = AllowedSource::validate("http://example.com/workflow.wdl", &config);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::UrlForbidden(_)));
    }

    #[test]
    fn path_with_dotdot() {
        use std::fs::File;

        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let subdir = temp_dir.path().join("subdir");
        std::fs::create_dir(&subdir).unwrap();
        let file_path = subdir.join("workflow.wdl");
        File::create(&file_path).unwrap();

        let config = make_config(vec![temp_dir.path().canonicalize().unwrap()], vec![]);

        let path_with_dotdot = subdir.join("..").join("subdir").join("workflow.wdl");
        let result = AllowedSource::validate(path_with_dotdot.to_str().unwrap(), &config);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), AllowedSource::File(_)));
    }

    #[test]
    fn url_trailing_slash() {
        let config = make_config(vec![], vec![String::from("https://example.com/allowed/")]);

        let allowed = AllowedSource::validate("https://example.com/allowed/workflow.wdl", &config);
        assert!(allowed.is_ok());

        let forbidden =
            AllowedSource::validate("https://example.com/allowedother/workflow.wdl", &config);
        assert!(forbidden.is_err());
        assert!(matches!(
            forbidden.unwrap_err(),
            ConfigError::UrlForbidden(_)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape() {
        use std::fs::File;
        use std::os::unix::fs::symlink;

        use tempfile::TempDir;

        let allowed_dir = TempDir::new().unwrap();
        let forbidden_dir = TempDir::new().unwrap();

        let forbidden_file = forbidden_dir.path().join("secret.wdl");
        File::create(&forbidden_file).unwrap();

        let symlink_path = allowed_dir.path().join("escape.wdl");
        symlink(&forbidden_file, &symlink_path).unwrap();

        let config = make_config(vec![allowed_dir.path().canonicalize().unwrap()], vec![]);

        let result = AllowedSource::validate(symlink_path.to_str().unwrap(), &config);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ConfigError::FilePathForbidden(_)
        ));
    }
}
