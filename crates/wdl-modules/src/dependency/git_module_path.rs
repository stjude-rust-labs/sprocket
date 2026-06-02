//! A validated sub-path within a Git-backed dependency.
//!
//! A Git repository may host multiple modules in distinct subdirectories.
//! When a consumer's `module.json` declares a Git dependency with a
//! `path` field (e.g., `"path": "csvkit"`), that value identifies which
//! subdirectory within the cloned repository contains the target
//! module's `module.json`. The same path appears in the lockfile's
//! `source` object so the resolver can locate the module after checkout.
//!
//! [`GitModulePath`] wraps [`RelativePath`] with one additional
//! restriction: the path must not be `"."` (or empty), because a
//! single-dot path is semantically equivalent to "no sub-path" and
//! would create an ambiguous cache layout. When a module sits at the
//! repository root, the `path` field is omitted entirely rather than
//! set to `"."`.

use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

use crate::RelativePath;
use crate::RelativePathError;

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

    /// The path contains a Git pathspec metacharacter.
    ///
    /// The sub-path is interpolated into a libgit2 checkout pathspec, so
    /// glob metacharacters (`*`, `?`, `[`, `]`) or a leading pathspec magic
    /// marker (`:`) would alter which tree entries are materialized.
    #[error("git module path `{0}` contains a disallowed pathspec metacharacter")]
    PathspecMetacharacter(String),
}

/// A validated, canonical sub-path within a Git-backed dependency.
///
/// Represents the `path` field on a Git dependency source—the relative
/// directory within the repository that contains the module's
/// `module.json`. Wraps [`RelativePath`] and additionally rejects `"."`
/// and empty strings, both of which are semantically equivalent to "no
/// sub-path" (i.e., the module sits at the repository root).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
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

impl TryFrom<String> for GitModulePath {
    type Error = GitModulePathError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::try_from(PathBuf::from(s))
    }
}

impl TryFrom<PathBuf> for GitModulePath {
    type Error = GitModulePathError;

    fn try_from(p: PathBuf) -> Result<Self, Self::Error> {
        let s = p.to_str().ok_or(RelativePathError::NonUtf8)?.to_string();
        if s.is_empty() {
            return Err(RelativePathError::Empty.into());
        }
        if s == "." {
            return Err(GitModulePathError::Dot);
        }
        if s.contains(['*', '?', '[', ']']) || s.starts_with(':') {
            return Err(GitModulePathError::PathspecMetacharacter(s));
        }
        let rp = RelativePath::try_from(s)?;
        Ok(Self(rp))
    }
}

impl FromStr for GitModulePath {
    type Err = GitModulePathError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(PathBuf::from(s))
    }
}

#[cfg(test)]
mod tests {
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
    fn rejects_pathspec_metacharacters() {
        for input in ["mod*", "mod?", "mod[a-z]", "a]b", ":(top)mod"] {
            let err = GitModulePath::from_str(input).unwrap_err();
            assert!(
                matches!(err, GitModulePathError::PathspecMetacharacter(_)),
                "expected `PathspecMetacharacter` for `{input}`; got: {err:?}"
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
