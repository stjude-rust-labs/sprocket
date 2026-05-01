//! A validated, canonical, NFC-normalized relative path under a module
//! root.
//!
//! [`RelativePath`] enforces the spec's per-path structural rules at
//! construction time. Once constructed, the value is guaranteed to be a
//! `/`-separated, lexically-clean path that is not absolute, not Windows
//! drive-prefixed, free of `\` separators and null bytes, and does not
//! escape the module root via `..`. Two inputs that name the same logical
//! file (e.g. `foo/./bar.wdl` and `foo/bar.wdl`, or two distinct Unicode
//! spellings of `café.wdl`) compare equal once normalized.

use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;

/// An error constructing a [`RelativePath`].
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum RelativePathError {
    /// The path contains non-UTF-8 bytes.
    #[error("path contains non-UTF-8 bytes")]
    NonUtf8,
    /// The path is empty.
    #[error("path is empty")]
    Empty,
    /// The path contains a null byte.
    #[error("path contains a null byte")]
    NullByte,
    /// The path uses Windows-style `\` separators.
    #[error("path uses Windows-style `\\` separators")]
    Backslash,
    /// The path starts with `/`.
    #[error("path starts with `/`")]
    Absolute,
    /// The path uses a Windows-style drive letter.
    #[error("path uses a Windows-style drive letter")]
    DriveLetter,
    /// The path resolves to nothing after lexical cleanup.
    #[error("path resolves to empty")]
    ResolvesToEmpty,
    /// The path escapes the module root via leading `..` segments.
    #[error("path escapes the module root")]
    EscapesRoot,
}

/// A validated, canonical, NFC-normalized relative path under a module
/// root.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct RelativePath(String);

impl RelativePath {
    /// Returns the path as a `/`-separated string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the path as a [`Path`].
    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }

    /// Consumes the [`RelativePath`] and returns its inner string.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl AsRef<str> for RelativePath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<Path> for RelativePath {
    fn as_ref(&self) -> &Path {
        Path::new(&self.0)
    }
}

impl std::fmt::Display for RelativePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl From<RelativePath> for String {
    fn from(path: RelativePath) -> Self {
        path.0
    }
}

impl From<RelativePath> for PathBuf {
    fn from(path: RelativePath) -> Self {
        path.0.into()
    }
}

impl TryFrom<&Path> for RelativePath {
    type Error = RelativePathError;

    fn try_from(path: &Path) -> Result<Self, Self::Error> {
        let s = path.to_str().ok_or(RelativePathError::NonUtf8)?;
        if s.is_empty() {
            return Err(RelativePathError::Empty);
        }
        if s.contains('\0') {
            return Err(RelativePathError::NullByte);
        }
        if s.contains('\\') {
            return Err(RelativePathError::Backslash);
        }
        if s.starts_with('/') {
            return Err(RelativePathError::Absolute);
        }
        if crate::starts_with_windows_drive(s) {
            return Err(RelativePathError::DriveLetter);
        }
        let cleaned = path_clean::clean(path)
            .into_os_string()
            .into_string()
            .map_err(|_| RelativePathError::NonUtf8)?;
        // `path_clean` produces native separators; normalize back to `/` since
        // the invariant of `RelativePath` is that the stored form is
        // `/`-separated. The earlier no-`\` check rejects user-supplied
        // backslashes, so any backslashes here came from `path_clean` on
        // Windows.
        let cleaned = cleaned.replace('\\', "/");
        if cleaned.is_empty() || cleaned == "." {
            return Err(RelativePathError::ResolvesToEmpty);
        }
        if cleaned.starts_with("..") {
            return Err(RelativePathError::EscapesRoot);
        }
        Ok(Self(cleaned.nfc().collect()))
    }
}

impl TryFrom<PathBuf> for RelativePath {
    type Error = RelativePathError;

    fn try_from(path: PathBuf) -> Result<Self, Self::Error> {
        Self::try_from(path.as_path())
    }
}

impl TryFrom<String> for RelativePath {
    type Error = RelativePathError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::try_from(Path::new(&s))
    }
}

impl FromStr for RelativePath {
    type Err = RelativePathError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(Path::new(s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_simple_relative_path() {
        let p = RelativePath::from_str("foo/bar.wdl").unwrap();
        assert_eq!(p.as_str(), "foo/bar.wdl");
    }

    #[test]
    fn cleans_dot_segments() {
        let p = RelativePath::from_str("./foo/./bar.wdl").unwrap();
        assert_eq!(p.as_str(), "foo/bar.wdl");
    }

    #[test]
    fn cleans_inner_double_dot() {
        let p = RelativePath::from_str("foo/../bar.wdl").unwrap();
        assert_eq!(p.as_str(), "bar.wdl");
    }

    #[test]
    fn collapses_duplicate_separators() {
        let p = RelativePath::from_str("foo//bar.wdl").unwrap();
        assert_eq!(p.as_str(), "foo/bar.wdl");
    }

    #[test]
    fn nfc_normalizes_on_construction() {
        let composed = RelativePath::from_str("caf\u{00E9}.wdl").unwrap();
        let decomposed = RelativePath::from_str("cafe\u{0301}.wdl").unwrap();
        assert_eq!(composed, decomposed);
        assert_eq!(composed.as_str(), "caf\u{00E9}.wdl");
    }

    #[test]
    fn rejects_per_path_violations() {
        for (bad, expected) in [
            ("", RelativePathError::Empty),
            ("has\0null", RelativePathError::NullByte),
            ("a\\b", RelativePathError::Backslash),
            ("/abs", RelativePathError::Absolute),
            ("C:/win", RelativePathError::DriveLetter),
            ("c:\\win", RelativePathError::Backslash),
            (".", RelativePathError::ResolvesToEmpty),
            ("..", RelativePathError::EscapesRoot),
            ("../escape", RelativePathError::EscapesRoot),
            ("a/..", RelativePathError::ResolvesToEmpty),
        ] {
            let err = RelativePath::from_str(bad).unwrap_err();
            assert_eq!(err, expected, "wrong error for `{bad}`");
        }
    }

    #[test]
    fn round_trips_via_serde() {
        let p = RelativePath::from_str("foo/bar.wdl").unwrap();
        let s = serde_json::to_string(&p).unwrap();
        assert_eq!(s, "\"foo/bar.wdl\"");
        let back: RelativePath = serde_json::from_str(&s).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn deserialize_normalizes_input() {
        let p: RelativePath = serde_json::from_str("\"./foo/./bar.wdl\"").unwrap();
        assert_eq!(p.as_str(), "foo/bar.wdl");
    }

    #[test]
    fn deserialize_rejects_invalid() {
        assert!(serde_json::from_str::<RelativePath>("\"/abs\"").is_err());
    }
}
