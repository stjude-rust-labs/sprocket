//! A validated sub-path within a Git-backed dependency.
//!
//! [`GitModulePath`] wraps [`RelativePath`] with one additional
//! restriction: the path must not be `"."` (or empty), because a
//! single-dot path is semantically equivalent to "no sub-path" and
//! would create an ambiguous cache layout.

use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

use crate::RelativePath;
use crate::RelativePathError;

////////////////////////////////////////////////////////////////////////////////////////
// Errors
////////////////////////////////////////////////////////////////////////////////////////

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
}

////////////////////////////////////////////////////////////////////////////////////////
// Main type
////////////////////////////////////////////////////////////////////////////////////////

/// A validated, canonical sub-path within a Git-backed dependency.
///
/// Wraps [`RelativePath`] and additionally rejects `"."` and empty
/// strings, both of which are semantically equivalent to "no sub-path"
/// in the Git cache layout.
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

    /// Consumes the [`GitModulePath`] and returns its inner
    /// [`RelativePath`].
    pub fn into_inner(self) -> RelativePath {
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
        std::fmt::Display::fmt(&self.0, f)
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
        if s.is_empty() {
            return Err(RelativePathError::Empty.into());
        }
        if s == "." {
            return Err(GitModulePathError::Dot);
        }
        let rp = RelativePath::try_from(s)?;
        Ok(Self(rp))
    }
}

impl FromStr for GitModulePath {
    type Err = GitModulePathError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s.to_string())
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
