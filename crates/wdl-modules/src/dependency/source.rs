//! Dependency-source parsing for `modules.json`.

use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;
use serde_with::DeserializeFromStr;
use serde_with::SerializeDisplay;
use thiserror::Error;
use url::Url;

use crate::lockfile::GitCommitish;
use crate::lockfile::GitCommitishError;
use crate::relative_path::RelativePath;
use crate::relative_path::RelativePathError;
use crate::version_requirement::VersionRequirement;
use crate::version_requirement::VersionRequirementError;

/// An error constructing a [`GitModulePath`].
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum GitModulePathError {
    /// The underlying [`RelativePath`] validation failed.
    #[error(transparent)]
    Invalid(#[from] RelativePathError),

    /// The path is `"."`, which is equivalent to no sub-path and
    /// therefore disallowed.
    #[error("git module path must not be `.`")]
    Dot,

    /// The path contains syntax interpreted specially by git pathspecs.
    #[error("git module path contains reserved git pathspec character `{0}`")]
    Pathspec(char),
}

/// A validated, canonical sub-path within a Git-backed dependency.
///
/// Represents the `path` field on a Git dependency source—the relative
/// directory within the repository that contains the module's
/// `module.json`. Wraps [`RelativePath`] and additionally rejects `"."`
/// and empty strings, both of which are semantically equivalent to "no
/// sub-path" (i.e., the module sits at the repository root).
#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, SerializeDisplay, DeserializeFromStr,
)]
pub struct GitModulePath(RelativePath);

impl GitModulePath {
    /// Returns the path as a `/`-separated string slice.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Returns the path as a [`Path`].
    pub fn as_path(&self) -> &Path {
        self.0.as_path()
    }

    /// Consumes the [`GitModulePath`] and returns the underlying
    /// [`RelativePath`].
    pub fn into_relative_path(self) -> RelativePath {
        self.0
    }
}

impl AsRef<str> for GitModulePath {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl AsRef<Path> for GitModulePath {
    fn as_ref(&self) -> &Path {
        self.0.as_ref()
    }
}

impl std::fmt::Display for GitModulePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<GitModulePath> for String {
    fn from(path: GitModulePath) -> Self {
        path.0.into()
    }
}

impl From<GitModulePath> for PathBuf {
    fn from(path: GitModulePath) -> Self {
        path.0.into()
    }
}

impl FromStr for GitModulePath {
    type Err = GitModulePathError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(RelativePathError::Empty.into());
        }
        if s == "." {
            return Err(GitModulePathError::Dot);
        }
        if let Some(character) = s
            .chars()
            .find(|character| matches!(character, '*' | '?' | '[' | ']' | '\\'))
            .or_else(|| {
                s.starts_with([':', '!', '^'])
                    .then(|| s.chars().next().expect("path is not empty"))
            })
        {
            return Err(GitModulePathError::Pathspec(character));
        }
        Ok(Self(RelativePath::from_str(s)?))
    }
}

impl TryFrom<&Path> for GitModulePath {
    type Error = GitModulePathError;

    fn try_from(path: &Path) -> Result<Self, Self::Error> {
        path.to_str().ok_or(RelativePathError::NonUtf8)?.parse()
    }
}

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

    /// A Git commit selector was invalid.
    #[error(transparent)]
    GitCommit(#[from] GitCommitishError),

    /// The Git URL did not parse.
    #[error("invalid Git URL: {0}")]
    InvalidUrl(String),

    /// The `path` field on a Git dependency was invalid.
    #[error("invalid `path` on Git dependency: {0}")]
    InvalidGitPath(#[from] GitModulePathError),
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
        path: Option<GitModulePath>,
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
                    GitSelector::Version(v.parse::<VersionRequirement>()?)
                } else if let Some(t) = tag {
                    GitSelector::Tag(t)
                } else if let Some(b) = branch {
                    GitSelector::Branch(b)
                } else if let Some(c) = commit {
                    GitSelector::Commit(GitCommitish::try_from(c)?)
                } else {
                    // `selector_count` is 1 in this branch, and the
                    // four `if let Some(...)` arms above cover every selector
                    // field, so one of them must match.
                    unreachable!()
                };
                let validated_path = git_subpath
                    .as_deref()
                    .map(GitModulePath::try_from)
                    .transpose()?;
                Ok(Self::Git {
                    url,
                    selector,
                    path: validated_path,
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GitSelector {
    /// A semver requirement matched against the repository's tags.
    Version(VersionRequirement),
    /// An exact Git tag name.
    Tag(String),
    /// A Git branch name.
    Branch(String),
    /// A Git commit SHA, or any unique prefix of one (7–40 hex chars).
    /// The resolver expands a prefix to the full SHA at lock time.
    Commit(GitCommitish),
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
                    path: path.map(PathBuf::from),
                    extra,
                    ..Default::default()
                };
                match selector {
                    GitSelector::Version(v) => fields.version = Some(v.to_string()),
                    GitSelector::Tag(t) => fields.tag = Some(t),
                    GitSelector::Branch(b) => fields.branch = Some(b),
                    GitSelector::Commit(c) => fields.commit = Some(c.to_string()),
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
        let dep = parse(
            r#"{
                "git": "https://github.com/x/y",
                "commit": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2"
            }"#,
        )
        .unwrap();
        match dep {
            DependencySource::Git {
                selector: GitSelector::Commit(commit),
                ..
            } => assert_eq!(commit.as_str(), "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2"),
            _ => panic!("expected `Commit` selector"),
        }
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
            } => assert_eq!(p.as_str(), "wdl"),
            _ => panic!("expected Git source with sub-path"),
        }
    }

    #[test]
    fn rejects_invalid_git_subpaths() {
        for bad in [
            r#"{"git": "https://x/y", "version": "^1", "path": "/abs"}"#,
            r#"{"git": "https://x/y", "version": "^1", "path": "../escape"}"#,
        ] {
            assert!(parse(bad).is_err(), "accepted `{bad}`");
        }
    }

    #[test]
    fn accepts_commit_prefix_selector() {
        let dep = parse(r#"{"git": "https://github.com/x/y", "commit": "a1b2c3d"}"#).unwrap();
        match dep {
            DependencySource::Git {
                selector: GitSelector::Commit(commit),
                ..
            } => {
                assert_eq!(commit.as_str(), "a1b2c3d");
                assert!(!commit.is_full());
            }
            _ => panic!("expected `Commit` selector"),
        }
    }

    #[test]
    fn rejects_too_short_commit_selector() {
        // Fewer than 4 hex characters is rejected.
        let err = parse(r#"{"git": "https://x/y", "commit": "ab"}"#).unwrap_err();
        assert!(
            err.to_string()
                .contains("must be 4 to 40 lowercase hex characters"),
            "wrong error: {err}"
        );
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

    #[test]
    fn rejects_absolute_git_path() {
        let err = parse(r#"{"git":"https://x/y","tag":"v1","path":"/etc/passwd"}"#).unwrap_err();
        assert!(
            err.to_string().contains("invalid `path` on Git dependency"),
            "expected `InvalidGitPath` for absolute path; got: {err}"
        );
    }

    #[test]
    fn rejects_parent_traversal_git_path() {
        let err = parse(r#"{"git":"https://x/y","tag":"v1","path":"../module"}"#).unwrap_err();
        assert!(
            err.to_string().contains("invalid `path` on Git dependency"),
            "expected `InvalidGitPath` for `../module`; got: {err}"
        );
    }

    #[test]
    fn rejects_nested_escape_git_path() {
        let err =
            parse(r#"{"git":"https://x/y","tag":"v1","path":"module/../../secret"}"#).unwrap_err();
        assert!(
            err.to_string().contains("invalid `path` on Git dependency"),
            "expected `InvalidGitPath` for nested escape; got: {err}"
        );
    }

    #[test]
    fn rejects_dot_git_path() {
        let err = parse(r#"{"git":"https://x/y","tag":"v1","path":"."}"#).unwrap_err();
        assert!(
            err.to_string().contains("`.`"),
            "expected dot rejection; got: {err}"
        );
    }

    #[test]
    fn rejects_empty_git_path() {
        let err = parse(r#"{"git":"https://x/y","tag":"v1","path":""}"#).unwrap_err();
        assert!(
            err.to_string().contains("invalid `path` on Git dependency"),
            "expected `InvalidGitPath` for empty path; got: {err}"
        );
    }

    #[test]
    fn accepts_valid_git_subpath() {
        let dep = parse(r#"{"git":"https://x/y","tag":"v1","path":"modules/csvkit"}"#).unwrap();
        match dep {
            DependencySource::Git { path: Some(p), .. } => {
                assert_eq!(p.as_str(), "modules/csvkit");
            }
            _ => panic!("expected Git source with valid sub-path"),
        }
    }
}

#[cfg(test)]
mod git_module_path_tests {
    use super::*;

    #[test]
    fn accepts_valid_subpath() {
        let p = GitModulePath::from_str("modules/csvkit").unwrap();
        assert_eq!(p.as_str(), "modules/csvkit");
    }

    #[test]
    fn rejects_empty_string() {
        let err = GitModulePath::from_str("").unwrap_err();
        assert!(
            matches!(err, GitModulePathError::Invalid(RelativePathError::Empty)),
            "expected `Invalid(Empty)` for empty string"
        );
    }

    #[test]
    fn rejects_dot() {
        let err = GitModulePath::from_str(".").unwrap_err();
        assert!(
            matches!(err, GitModulePathError::Dot),
            "expected `Dot` for `.`"
        );
    }

    #[test]
    fn rejects_absolute_path() {
        let err = GitModulePath::from_str("/tmp/module").unwrap_err();
        assert!(
            matches!(
                err,
                GitModulePathError::Invalid(RelativePathError::Absolute(_))
            ),
            "expected `Invalid(Absolute)` for `/tmp/module`"
        );
    }

    #[test]
    fn rejects_parent_traversal() {
        let err = GitModulePath::from_str("../module").unwrap_err();
        assert!(
            matches!(
                err,
                GitModulePathError::Invalid(RelativePathError::EscapesRoot(_))
            ),
            "expected `Invalid(EscapesRoot)` for `../module`"
        );
    }

    #[test]
    fn rejects_nested_escape() {
        let err = GitModulePath::from_str("module/../../secret").unwrap_err();
        assert!(
            matches!(
                err,
                GitModulePathError::Invalid(RelativePathError::EscapesRoot(_))
            ),
            "expected `Invalid(EscapesRoot)` for `module/../../secret`"
        );
    }

    #[test]
    fn rejects_git_pathspec_syntax() {
        for path in [
            "*",
            "modules/[ab]",
            "modules/?",
            ":/modules",
            "!modules",
            "^modules",
        ] {
            let result = GitModulePath::from_str(path);
            assert!(
                matches!(result, Err(GitModulePathError::Pathspec(_))),
                "expected `{path}` to reject Git pathspec syntax"
            );
        }
    }

    #[test]
    fn round_trips_via_serde() {
        let p = GitModulePath::from_str("modules/csvkit").unwrap();
        let s = serde_json::to_string(&p).unwrap();
        assert_eq!(s, "\"modules/csvkit\"");
        let back: GitModulePath = serde_json::from_str(&s).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn serde_rejects_dot() {
        let err = serde_json::from_str::<GitModulePath>("\".\"").unwrap_err();
        assert!(
            err.to_string().contains("`.`"),
            "expected dot rejection; got: {err}"
        );
    }
}
