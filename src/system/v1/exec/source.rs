//! Validated workflow sources.

use url::Url;

use super::ConfigError;
use super::ConfigResult;
use crate::analysis::Source;
use crate::config::ServerConfig;

/// Determines if a given URL is allowed.
///
/// To be allowed, the given URL must:
///
/// * Have a scheme that matches the allowed URL
/// * Have an authority that matches the allowed URL
/// * Have a host that matches the allowed URL
/// * Have a port that matches the allowed URL
/// * Have a path that is an subpath of the allowed URL's path
///
/// Returns `true` if the URL is allowed or `false` if it is not.
///
/// # Panics
///
/// Panics if either URL cannot be a base.
fn is_url_allowed(url: &Url, allowed: &Url) -> bool {
    if url.scheme() != allowed.scheme()
        || url.authority() != allowed.authority()
        || url.host_str() != allowed.host_str()
        || url.port_or_known_default() != allowed.port_or_known_default()
    {
        return false;
    }

    let mut path = url.path_segments().unwrap();
    let mut allowed_path = allowed.path_segments().unwrap();

    loop {
        match (path.next(), allowed_path.next()) {
            (Some(segment), Some(allowed_segment)) if segment == allowed_segment => continue,
            (_, None) => return true,
            (_, Some(allowed_segment)) => return allowed_segment.is_empty(),
        }
    }
}

/// Validates a source string against the server configuration.
///
/// This function validates a source string to prevent:
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
/// A successful return guarantees:
///
/// - File paths are absolute, canonical, and within allowed directories
/// - URLs match at least one configured prefix
///
/// # Preconditions
///
/// The configuration must have been validated via `ServerConfig::validate()`
/// which ensures all allowed paths are canonical.
pub fn validate(source: &str, config: &ServerConfig) -> ConfigResult<Source> {
    // On Windows, paths like `C:\...` are parsed by `Url::parse` as URLs
    // with scheme `c:`. We distinguish URLs from file paths by checking for
    // `://` which all hierarchical URLs (http, https, file, etc.) contain.
    let path = if source.contains("://")
        && let Ok(url) = Url::parse(source)
    {
        // Only non-file URLs are treated like URL sources; otherwise, treat it
        // as a path
        if url.scheme() != "file" {
            let is_allowed = config.allowed_urls.iter().any(|allowed| {
                // SAFETY: configuration validation ensures that the allowed
                // URLs will always have path segments.
                // SAFETY: the above check for `://` in the URL will also
                // guarantee the URL has path segments.
                is_url_allowed(&url, allowed)
            });

            if !is_allowed {
                return Err(ConfigError::UrlForbidden(url));
            }

            return Ok(Source::Url(url));
        }

        url.to_file_path()
            .map_err(|_| ConfigError::InvalidFileUrl(url))?
    } else {
        shellexpand::tilde(source).into_owned().into()
    };

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

    if canonical_path.is_dir() {
        return Ok(Source::Directory(canonical_path));
    }

    // Convert the canonical path to a `file://` URL and return as Source
    let url =
        Url::from_file_path(&canonical_path).expect("canonical path should convert to file URL");
    Ok(Source::File(url))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn make_config(allowed_file_paths: Vec<PathBuf>, allowed_urls: Vec<Url>) -> ServerConfig {
        ServerConfig {
            allowed_file_paths,
            allowed_urls,
            ..Default::default()
        }
    }

    #[test]
    fn allowed_urls() {
        for (url, allowed, expected) in [
            // Check for mismatched scheme
            ("http://example.com/foo/bar", "https://example.com", false),
            // Check for mismatched authority
            (
                "http://example.com/foo/bar",
                "http://foo:bar@example.com",
                false,
            ),
            // Check for mismatched host
            ("http://example.com/foo/bar", "http://exampl.com", false),
            // Check for mismatched port
            (
                "http://example.com/foo/bar",
                "http://example.com:8080",
                false,
            ),
            // Check for mismatched paths
            (
                "http://example.com/foo/bar",
                "http://example.com/bar",
                false,
            ),
            (
                "http://example.com/foo/bar",
                "http://example.com/foo/bar/baz",
                false,
            ),
            (
                "http://example.com/foo/bar/baz.wdl",
                "http://example.com/foo/bar/baz",
                false,
            ),
            (
                "http://example.com/foo/bar/baz",
                "http://example.com/foo/bar/baz/qux",
                false,
            ),
            // Check for allowed paths
            ("http://example.com/foo/bar", "http://example.com/", true),
            ("http://example.com/foo/bar", "http://example.com/foo", true),
            (
                "http://example.com/foo/bar",
                "http://example.com/foo/bar",
                true,
            ),
            (
                "http://example.com/foo/bar/baz",
                "http://example.com/foo/bar",
                true,
            ),
        ] {
            let url: Url = url.parse().unwrap();
            let allowed: Url = allowed.parse().unwrap();

            let is_allowed = is_url_allowed(&url, &allowed);

            if expected {
                assert!(
                    is_allowed,
                    "`{url}` is expected to be allowed by `{allowed}`"
                );
            } else {
                assert!(
                    !is_allowed,
                    "`{url}` is expected to not be allowed by `{allowed}`"
                );
            }
        }
    }

    #[test]
    fn validate_url_allowed() {
        let mut config = make_config(
            vec![],
            vec![
                "https://example.com/".parse().unwrap(),
                "http://localhost/".parse().unwrap(),
            ],
        );
        config.validate().unwrap();

        let result = validate("https://example.com/workflow.wdl", &config);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Source::Url(_)));
    }

    #[test]
    fn validate_url_forbidden() {
        let mut config = make_config(vec![], vec!["https://example.com/".parse().unwrap()]);
        config.validate().unwrap();

        let result = validate("https://forbidden.com/workflow.wdl", &config);
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

        let result = validate(file_path.to_str().unwrap(), &config);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Source::File(_)));

        let result = validate(
            &format!("file://{path}", path = file_path.to_str().unwrap()),
            &config,
        );
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Source::File(_)));

        let result = validate(
            &format!(
                "file://{path}/../../../evil.wdl",
                path = file_path.to_str().unwrap()
            ),
            &config,
        );
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ConfigError::FilePathForbidden(_)
        ));
    }

    #[test]
    fn validate_directory_allowed() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let config = make_config(vec![temp_dir.path().canonicalize().unwrap()], vec![]);

        let result = validate(temp_dir.path().to_str().unwrap(), &config);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Source::Directory(_)));

        let result = validate(
            &format!("file://{path}", path = temp_dir.path().to_str().unwrap()),
            &config,
        );
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Source::Directory(_)));

        let result = validate(
            &format!(
                "file://{path}/../../../evil.wdl",
                path = temp_dir.path().to_str().unwrap()
            ),
            &config,
        );
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ConfigError::FilePathForbidden(_)
        ));
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
        let result1 = validate(existing_file.to_str().unwrap(), &config);
        assert!(matches!(
            result1.unwrap_err(),
            ConfigError::FilePathForbidden(_)
        ));

        let result2 = validate(nonexistent_file.to_str().unwrap(), &config);
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
        let result = validate(nonexistent.to_str().unwrap(), &config);
        assert!(matches!(result.unwrap_err(), ConfigError::FileNotFound(_)));
    }

    #[test]
    fn validate_url_scheme_must_match() {
        let config = make_config(vec![], vec!["https://example.com/".parse().unwrap()]);

        // http should not be allowed when only https is configured
        let result = validate("http://example.com/workflow.wdl", &config);
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
        let result = validate(path_with_dotdot.to_str().unwrap(), &config);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Source::File(_)));
    }

    #[test]
    fn url_trailing_slash() {
        let config = make_config(
            vec![],
            vec!["https://example.com/allowed/".parse().unwrap()],
        );

        let allowed = validate("https://example.com/allowed/workflow.wdl", &config);
        assert!(allowed.is_ok());

        let forbidden = validate("https://example.com/allowedother/workflow.wdl", &config);
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

        let result = validate(symlink_path.to_str().unwrap(), &config);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ConfigError::FilePathForbidden(_)
        ));
    }
}
