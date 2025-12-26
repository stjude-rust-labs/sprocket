//! Representation of evaluation paths that support URLs.

use std::fmt;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use path_clean::PathClean;
use url::Url;

use crate::ContentKind;
use crate::config::ContentDigestMode;
use crate::digest::Digest;
use crate::digest::calculate_local_digest;
use crate::digest::calculate_remote_digest;
use crate::http::Transferer;

/// The URL schemes supported by this crate.
const SUPPORTED_SCHEMES: &[&str] = &["http://", "https://", "file://", "az://", "s3://", "gs://"];

/// Helper to check if a given string starts with the given prefix, ignoring
/// ASCII case.
fn starts_with_ignore_ascii_case(s: &str, prefix: &str) -> bool {
    s.get(0..prefix.len())
        .map(|s| s.eq_ignore_ascii_case(prefix))
        .unwrap_or(false)
}

/// Determines if the given string is prefixed with a `file` URL scheme.
pub(crate) fn is_file_url(s: &str) -> bool {
    starts_with_ignore_ascii_case(s.trim_start(), "file://")
}

/// Determines if the given string is prefixed with a supported URL scheme.
pub(crate) fn is_supported_url(s: &str) -> bool {
    SUPPORTED_SCHEMES
        .iter()
        .any(|scheme| starts_with_ignore_ascii_case(s.trim_start(), scheme))
}

/// Represents the kind of an evaluation path.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum EvaluationPathKind {
    /// The path is local (i.e. on the host).
    Local(PathBuf),
    /// The path is remote.
    Remote(Url),
}

impl fmt::Display for EvaluationPathKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local(path) => write!(f, "{path}", path = path.display()),
            Self::Remote(url) => write!(f, "{url}"),
        }
    }
}

/// Represents a path used in evaluation that may be either local or remote.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EvaluationPath(EvaluationPathKind);

impl EvaluationPath {
    /// Constructs an `EvaluationPath` from a local path.
    ///
    /// This is an internal method where we assume the path is already "clean".
    pub(crate) fn from_local_path(path: PathBuf) -> Self {
        Self(EvaluationPathKind::Local(path))
    }

    /// Joins the given path to this path.
    pub fn join(&self, path: &str) -> Result<Self> {
        // URLs are absolute, so they can't be joined
        if is_supported_url(path) {
            return path.parse();
        }

        // We can't join an absolute local path either
        let p = Path::new(path);
        if p.is_absolute() {
            return Ok(Self(EvaluationPathKind::Local(p.clean())));
        }

        match &self.0 {
            EvaluationPathKind::Local(dir) => {
                Ok(Self(EvaluationPathKind::Local(dir.join(path).clean())))
            }
            EvaluationPathKind::Remote(dir) => Ok(Self(
                dir.join(path)
                    .map(EvaluationPathKind::Remote)
                    .with_context(|| format!("failed to join `{path}` to URL `{dir}`"))?,
            )),
        }
    }

    /// Returns `true` if the path is local.
    pub fn is_local(&self) -> bool {
        matches!(&self.0, EvaluationPathKind::Local(_))
    }

    /// Converts the path to a local path.
    ///
    /// Returns `None` if the path is remote.
    pub fn as_local(&self) -> Option<&Path> {
        match &self.0 {
            EvaluationPathKind::Local(path) => Some(path),
            EvaluationPathKind::Remote(_) => None,
        }
    }

    /// Unwraps the path to a local path.
    ///
    /// # Panics
    ///
    /// Panics if the path is remote.
    pub fn unwrap_local(self) -> PathBuf {
        match self.0 {
            EvaluationPathKind::Local(path) => path,
            EvaluationPathKind::Remote(_) => panic!("path is remote"),
        }
    }

    /// Returns `true` if the path is remote.
    pub fn is_remote(&self) -> bool {
        matches!(&self.0, EvaluationPathKind::Remote(_))
    }

    /// Converts the path to a remote URL.
    ///
    /// Returns `None` if the path is local.
    pub fn as_remote(&self) -> Option<&Url> {
        match &self.0 {
            EvaluationPathKind::Local(_) => None,
            EvaluationPathKind::Remote(url) => Some(url),
        }
    }

    /// Unwraps the path to a remote URL.
    ///
    /// # Panics
    ///
    /// Panics if the path is local.
    pub fn unwrap_remote(self) -> Url {
        match self.0 {
            EvaluationPathKind::Local(_) => panic!("path is local"),
            EvaluationPathKind::Remote(url) => url,
        }
    }

    /// Gets the parent of the given path.
    ///
    /// Returns `None` if the evaluation path isn't valid or has no parent.
    pub fn parent_of(path: &str) -> Option<Self> {
        let path: EvaluationPath = path.parse().ok()?;
        match path.0 {
            EvaluationPathKind::Local(path) => path
                .parent()
                .map(|p| Self(EvaluationPathKind::Local(p.to_path_buf()))),
            EvaluationPathKind::Remote(mut url) => {
                if url.path() == "/" {
                    return None;
                }

                if let Ok(mut segments) = url.path_segments_mut() {
                    segments.pop_if_empty().pop();
                }

                Some(Self(EvaluationPathKind::Remote(url)))
            }
        }
    }

    /// Gets the file name of the path.
    ///
    /// Returns `Ok(None)` if the path does not contain a file name (i.e. is
    /// root).
    ///
    /// Returns an error if the file name is not UTF-8.
    pub fn file_name(&self) -> Result<Option<&str>> {
        match &self.0 {
            EvaluationPathKind::Local(path) => path
                .file_name()
                .map(|n| {
                    n.to_str().with_context(|| {
                        format!("path `{path}` is not UTF-8", path = path.display())
                    })
                })
                .transpose(),
            EvaluationPathKind::Remote(url) => {
                Ok(url.path_segments().and_then(|mut s| s.next_back()))
            }
        }
    }

    /// Calculates the content digest of the evaluation path.
    pub(crate) async fn calculate_digest(
        &self,
        transferer: &dyn Transferer,
        kind: ContentKind,
        mode: ContentDigestMode,
    ) -> Result<Digest> {
        match &self.0 {
            EvaluationPathKind::Local(path) => calculate_local_digest(path, kind, mode).await,
            EvaluationPathKind::Remote(url) => calculate_remote_digest(transferer, url, kind).await,
        }
    }
}

impl FromStr for EvaluationPath {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        // Store `file` schemed URLs as local paths.
        if is_file_url(s) {
            let url = s
                .parse::<Url>()
                .with_context(|| format!("invalid `file` schemed URL `{s}`"))?;
            return url
                .to_file_path()
                .map(|p| Self(EvaluationPathKind::Local(p.clean())))
                .map_err(|_| anyhow!("URL `{s}` cannot be represented as a local file path"));
        }

        if is_supported_url(s) {
            return Ok(Self(EvaluationPathKind::Remote(
                s.parse().with_context(|| format!("URL `{s}` is invalid"))?,
            )));
        }

        Ok(Self(EvaluationPathKind::Local(Path::new(s).clean())))
    }
}

impl fmt::Display for EvaluationPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl TryFrom<&str> for EvaluationPath {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self> {
        value.parse()
    }
}

impl TryFrom<EvaluationPath> for String {
    type Error = anyhow::Error;

    fn try_from(path: EvaluationPath) -> Result<Self> {
        match path.0 {
            EvaluationPathKind::Local(path) => match path.into_os_string().into_string() {
                Ok(s) => Ok(s),
                Err(path) => bail!(
                    "path `{path}` cannot be represented with UTF-8",
                    path = path.display()
                ),
            },
            EvaluationPathKind::Remote(url) => Ok(url.into()),
        }
    }
}

impl From<&Path> for EvaluationPath {
    fn from(path: &Path) -> Self {
        Self(EvaluationPathKind::Local(path.clean()))
    }
}

impl TryFrom<Url> for EvaluationPath {
    type Error = anyhow::Error;

    fn try_from(url: Url) -> std::result::Result<Self, Self::Error> {
        if !is_supported_url(url.as_str()) {
            bail!("URL `{url}` is not supported");
        }

        Ok(Self(EvaluationPathKind::Remote(url)))
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_file_urls() {
        assert!(is_file_url("file:///foo/bar/baz"));
        assert!(is_file_url("FiLe:///foo/bar/baz"));
        assert!(is_file_url("FILE:///foo/bar/baz"));
        assert!(!is_file_url("https://example.com/bar/baz"));
        assert!(!is_file_url("az://foo/bar/baz"));
    }

    #[test]
    fn test_urls() {
        assert!(is_supported_url("http://example.com/foo/bar/baz"));
        assert!(is_supported_url("HtTp://example.com/foo/bar/baz"));
        assert!(is_supported_url("HTTP://example.com/foo/bar/baz"));
        assert!(is_supported_url("https://example.com/foo/bar/baz"));
        assert!(is_supported_url("HtTpS://example.com/foo/bar/baz"));
        assert!(is_supported_url("HTTPS://example.com/foo/bar/baz"));
        assert!(is_supported_url("file:///foo/bar/baz"));
        assert!(is_supported_url("FiLe:///foo/bar/baz"));
        assert!(is_supported_url("FILE:///foo/bar/baz"));
        assert!(is_supported_url("az://foo/bar/baz"));
        assert!(is_supported_url("aZ://foo/bar/baz"));
        assert!(is_supported_url("AZ://foo/bar/baz"));
        assert!(is_supported_url("s3://foo/bar/baz"));
        assert!(is_supported_url("S3://foo/bar/baz"));
        assert!(is_supported_url("gs://foo/bar/baz"));
        assert!(is_supported_url("gS://foo/bar/baz"));
        assert!(is_supported_url("GS://foo/bar/baz"));
        assert!(!is_supported_url("foo://foo/bar/baz"));
    }

    #[test]
    fn test_evaluation_path_parsing() {
        let p: EvaluationPath = "/foo/bar/baz".parse().expect("should parse");
        assert_eq!(
            p.unwrap_local().to_str().unwrap().replace("\\", "/"),
            "/foo/bar/baz"
        );

        let p: EvaluationPath = "foo".parse().expect("should parse");
        assert_eq!(p.unwrap_local().as_os_str(), "foo");

        #[cfg(unix)]
        {
            let p: EvaluationPath = "file:///foo/bar/baz".parse().expect("should parse");
            assert_eq!(p.unwrap_local().as_os_str(), "/foo/bar/baz");
        }

        #[cfg(windows)]
        {
            let p: EvaluationPath = "file:///C:/foo/bar/baz".parse().expect("should parse");
            assert_eq!(p.unwrap_local().as_os_str(), "C:\\foo\\bar\\baz");
        }

        let p: EvaluationPath = "https://example.com/foo/bar/baz"
            .parse()
            .expect("should parse");
        assert_eq!(
            p.unwrap_remote().as_str(),
            "https://example.com/foo/bar/baz"
        );

        let p: EvaluationPath = "az://foo/bar/baz".parse().expect("should parse");
        assert_eq!(p.unwrap_remote().as_str(), "az://foo/bar/baz");

        let p: EvaluationPath = "s3://foo/bar/baz".parse().expect("should parse");
        assert_eq!(p.unwrap_remote().as_str(), "s3://foo/bar/baz");

        let p: EvaluationPath = "gs://foo/bar/baz".parse().expect("should parse");
        assert_eq!(p.unwrap_remote().as_str(), "gs://foo/bar/baz");
    }

    #[test]
    fn test_evaluation_path_join() {
        let p: EvaluationPath = "/foo/bar/baz".parse().expect("should parse");
        assert_eq!(
            p.join("qux/../quux")
                .expect("should join")
                .unwrap_local()
                .to_str()
                .unwrap()
                .replace("\\", "/"),
            "/foo/bar/baz/quux"
        );

        let p: EvaluationPath = "foo".parse().expect("should parse");
        assert_eq!(
            p.join("qux/../quux")
                .expect("should join")
                .unwrap_local()
                .to_str()
                .unwrap()
                .replace("\\", "/"),
            "foo/quux"
        );

        #[cfg(unix)]
        {
            let p: EvaluationPath = "file:///foo/bar/baz".parse().expect("should parse");
            assert_eq!(
                p.join("qux/../quux")
                    .expect("should join")
                    .unwrap_local()
                    .as_os_str(),
                "/foo/bar/baz/quux"
            );
        }

        #[cfg(windows)]
        {
            let p: EvaluationPath = "file:///C:/foo/bar/baz".parse().expect("should parse");
            assert_eq!(
                p.join("qux/../quux")
                    .expect("should join")
                    .unwrap_local()
                    .as_os_str(),
                "C:\\foo\\bar\\baz\\quux"
            );
        }

        let p: EvaluationPath = "https://example.com/foo/bar/baz"
            .parse()
            .expect("should parse");
        assert_eq!(
            p.join("qux/../quux")
                .expect("should join")
                .unwrap_remote()
                .as_str(),
            "https://example.com/foo/bar/quux"
        );

        let p: EvaluationPath = "https://example.com/foo/bar/baz/"
            .parse()
            .expect("should parse");
        assert_eq!(
            p.join("qux/../quux")
                .expect("should join")
                .unwrap_remote()
                .as_str(),
            "https://example.com/foo/bar/baz/quux"
        );

        let p: EvaluationPath = "az://foo/bar/baz/".parse().expect("should parse");
        assert_eq!(
            p.join("qux/../quux")
                .expect("should join")
                .unwrap_remote()
                .as_str(),
            "az://foo/bar/baz/quux"
        );

        let p: EvaluationPath = "s3://foo/bar/baz/".parse().expect("should parse");
        assert_eq!(
            p.join("qux/../quux")
                .expect("should join")
                .unwrap_remote()
                .as_str(),
            "s3://foo/bar/baz/quux"
        );

        let p: EvaluationPath = "gs://foo/bar/baz/".parse().expect("should parse");
        assert_eq!(
            p.join("qux/../quux")
                .expect("should join")
                .unwrap_remote()
                .as_str(),
            "gs://foo/bar/baz/quux"
        );
    }
}
