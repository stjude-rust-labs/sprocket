//! Dependency-source parsing for `modules.json`.

use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;
use url::Url;

use crate::VersionRequirement;
use crate::VersionRequirementError;

/// An error parsing a [`DependencySource`].
#[derive(Debug, Error)]
pub enum DependencySourceError {
    /// The dependency does not name a valid source.
    ///
    /// The dependency must specify exactly one of `path` for a local-path
    /// source, or `git` with exactly one of `version`, `tag`, `branch`, or
    /// `commit` for a Git source. The `reason` describes which rule was
    /// violated by the input.
    #[error(
        "dependency source is invalid: {reason}; must specify either `path` for a local-path \
         source, or `git` with exactly one of `version`, `tag`, `branch`, or `commit` for a Git \
         source"
    )]
    InvalidSource {
        /// A short description of which validation rule was violated.
        reason: &'static str,
    },

    /// A version requirement on a Git dependency was invalid.
    #[error(transparent)]
    VersionRequirement(#[from] VersionRequirementError),

    /// The `git` URL did not parse.
    #[error("invalid `git` URL: {0}")]
    InvalidUrl(String),
}

/// A dependency source.
///
/// The two possible sources are a Git repository (with one of four
/// selectors plus an optional sub-path) or a local filesystem path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "DependencySourceFields", into = "DependencySourceFields")]
pub enum DependencySource {
    /// A Git-backed dependency.
    Git {
        /// The Git repository URL.
        url: Url,
        /// The selector controlling which revision to resolve to.
        selector: GitSelector,
        /// Optional sub-path within the repository where the module lives.
        path: Option<PathBuf>,
        /// Unknown fields, preserved for round-trip and inspection.
        extra: serde_json::Map<String, serde_json::Value>,
    },
    /// A local filesystem dependency.
    LocalPath {
        /// The path to the local module directory.
        path: PathBuf,
        /// Unknown fields, preserved for round-trip and inspection.
        extra: serde_json::Map<String, serde_json::Value>,
    },
}

impl TryFrom<DependencySourceFields> for DependencySource {
    type Error = DependencySourceError;

    fn try_from(fields: DependencySourceFields) -> Result<Self, Self::Error> {
        let DependencySourceFields {
            git,
            path,
            version,
            tag,
            branch,
            commit,
            extra,
        } = fields;

        let selector_count = [&version, &tag, &branch, &commit]
            .iter()
            .filter(|s| s.is_some())
            .count();

        match (git, path) {
            (Some(g), git_subpath) => {
                if selector_count == 0 {
                    return Err(DependencySourceError::InvalidSource {
                        reason: "Git dependency is missing a selector",
                    });
                }
                if selector_count > 1 {
                    return Err(DependencySourceError::InvalidSource {
                        reason: "Git dependency specifies more than one selector",
                    });
                }
                let url =
                    Url::parse(&g).map_err(|e| DependencySourceError::InvalidUrl(e.to_string()))?;
                let selector = if let Some(v) = version {
                    GitSelector::Version(VersionRequirement::try_from(v)?)
                } else if let Some(t) = tag {
                    GitSelector::Tag(t)
                } else if let Some(b) = branch {
                    GitSelector::Branch(b)
                } else if let Some(c) = commit {
                    GitSelector::Commit(c)
                } else {
                    // SAFETY: `selector_count` is 1 in this branch, and the
                    // four `if let Some(...)` arms above cover every selector
                    // field, so one of them must match.
                    unreachable!()
                };
                Ok(Self::Git {
                    url,
                    selector,
                    path: git_subpath,
                    extra,
                })
            }
            (None, Some(p)) => {
                if selector_count > 0 {
                    return Err(DependencySourceError::InvalidSource {
                        reason: "local-path dependency cannot specify a selector",
                    });
                }
                Ok(Self::LocalPath { path: p, extra })
            }
            (None, None) => Err(DependencySourceError::InvalidSource {
                reason: "neither `git` nor `path` was specified",
            }),
        }
    }
}

/// A Git revision selector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GitSelector {
    /// A semver requirement matched against the repository's tags.
    Version(VersionRequirement),
    /// An exact Git tag name.
    Tag(String),
    /// A Git branch name.
    Branch(String),
    /// A full Git commit SHA.
    Commit(String),
}

/// Flat field set of a dependency declaration as it appears in
/// `module.json`, before mutual-exclusion validation projects it onto
/// [`DependencySource`].
#[derive(Debug, Default, Serialize, Deserialize)]
struct DependencySourceFields {
    /// The Git URL, if the dependency is Git-backed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    git: Option<String>,
    /// Either the local-path source, or a sub-path within a Git source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    path: Option<PathBuf>,
    /// The semver requirement on a Git source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    /// The Git tag selector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tag: Option<String>,
    /// The Git branch selector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    branch: Option<String>,
    /// The Git commit selector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    commit: Option<String>,
    /// Unknown fields, preserved for round-trip and inspection.
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

impl From<DependencySource> for DependencySourceFields {
    fn from(source: DependencySource) -> Self {
        match source {
            DependencySource::Git {
                url,
                selector,
                path,
                extra,
            } => {
                let mut fields = DependencySourceFields {
                    git: Some(url.to_string()),
                    path,
                    extra,
                    ..Default::default()
                };
                match selector {
                    GitSelector::Version(v) => fields.version = Some(v.inner().to_string()),
                    GitSelector::Tag(t) => fields.tag = Some(t),
                    GitSelector::Branch(b) => fields.branch = Some(b),
                    GitSelector::Commit(c) => fields.commit = Some(c),
                }
                fields
            }
            DependencySource::LocalPath { path, extra } => DependencySourceFields {
                path: Some(path),
                extra,
                ..Default::default()
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Result<DependencySource, serde_json::Error> {
        serde_json::from_str(s)
    }

    #[test]
    fn parses_git_with_version() {
        let dep = parse(r#"{"git": "https://github.com/x/y", "version": "^1.0.0"}"#).unwrap();
        match dep {
            DependencySource::Git {
                selector: GitSelector::Version(_),
                ..
            } => {}
            _ => panic!("expected `Version` selector"),
        }
    }

    #[test]
    fn parses_git_with_tag() {
        let dep = parse(r#"{"git": "https://github.com/x/y", "tag": "v1.2.3"}"#).unwrap();
        assert!(matches!(
            dep,
            DependencySource::Git {
                selector: GitSelector::Tag(_),
                ..
            }
        ));
    }

    #[test]
    fn parses_git_with_branch() {
        let dep = parse(r#"{"git": "https://github.com/x/y", "branch": "main"}"#).unwrap();
        assert!(matches!(
            dep,
            DependencySource::Git {
                selector: GitSelector::Branch(_),
                ..
            }
        ));
    }

    #[test]
    fn parses_git_with_commit() {
        let dep = parse(r#"{"git": "https://github.com/x/y", "commit": "abc123"}"#).unwrap();
        assert!(matches!(
            dep,
            DependencySource::Git {
                selector: GitSelector::Commit(_),
                ..
            }
        ));
    }

    #[test]
    fn parses_local_path() {
        let dep = parse(r#"{"path": "../local"}"#).unwrap();
        assert!(matches!(dep, DependencySource::LocalPath { .. }));
    }

    #[test]
    fn parses_git_with_subpath() {
        let dep = parse(r#"{"git": "https://github.com/x/y", "version": "^1.0.0", "path": "wdl"}"#)
            .unwrap();
        match dep {
            DependencySource::Git {
                selector: GitSelector::Version(_),
                path: Some(p),
                ..
            } => assert_eq!(p, std::path::Path::new("wdl")),
            _ => panic!("expected Git source with sub-path"),
        }
    }

    #[test]
    fn captures_unknown_fields() {
        let dep =
            parse(r#"{"git": "https://github.com/x/y", "version": "^1.0.0", "deprecated": true}"#)
                .unwrap();
        match dep {
            DependencySource::Git { extra, .. } => {
                assert_eq!(
                    extra.get("deprecated"),
                    Some(&serde_json::Value::Bool(true))
                );
            }
            _ => panic!("expected Git source"),
        }
    }

    #[test]
    fn rejects_invalid_structures() {
        for bad in [
            r#"{"git": "https://x/y", "version": "^1", "tag": "v1"}"#,
            r#"{"git": "https://x/y"}"#,
            r#"{"path": "p", "version": "^1"}"#,
            r#"{}"#,
        ] {
            let err = parse(bad).unwrap_err();
            assert!(
                err.to_string().contains("dependency source is invalid"),
                "wrong message for `{bad}`: {err}"
            );
        }
    }
}
