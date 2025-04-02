//! Representation of evaluation paths that support URLs.

use std::fmt;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use path_clean::clean;
use url::Url;

use crate::PrimitiveValue;

/// Determines if the given string is prefixed with a `file` URL scheme.
pub fn is_file_url(s: &str) -> bool {
    s.get(0..7)
        .map(|s| s.eq_ignore_ascii_case("file://"))
        .unwrap_or(false)
}

/// Determines if the given string is prefixed with a supported URL scheme.
pub fn is_url(s: &str) -> bool {
    ["http://", "https://", "file://", "az://", "s3://", "gs://"]
        .iter()
        .any(|prefix| {
            s.get(0..prefix.len())
                .map(|s| s.eq_ignore_ascii_case(prefix))
                .unwrap_or(false)
        })
}

/// Parses a string into a URL.
///
/// Returns `None` if the string is not a supported scheme or not a valid URL.
pub fn parse_url(s: &str) -> Option<Url> {
    if !is_url(s) {
        return None;
    }

    s.parse().ok()
}

/// Represents a path used in evaluation that may be either local or remote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvaluationPath {
    /// The path is local (i.e. on the host).
    Local(PathBuf),
    /// The path is remote.
    Remote(Url),
}

impl EvaluationPath {
    /// Joins the given path to this path.
    pub fn join(&self, path: &str) -> Result<Self> {
        // URLs are absolute, so they can't be joined
        if is_url(path) {
            return path.parse();
        }

        // We can't join an absolute local path either
        if Path::new(path).is_absolute() {
            return Ok(Self::Local(clean(path)));
        }

        match self {
            Self::Local(dir) => Ok(Self::Local(dir.join(clean(path)))),
            Self::Remote(dir) => dir
                .join(path)
                .map(Self::Remote)
                .with_context(|| format!("failed to join `{path}` to URL `{dir}`")),
        }
    }

    /// Creates a path from a primitive `File` or `Directory` value.
    pub fn from_primitive_value(v: &PrimitiveValue) -> Result<Self> {
        match v {
            PrimitiveValue::File(path) | PrimitiveValue::Directory(path) => path.parse(),
            _ => bail!("primitive value must be a `File` or a `Directory`"),
        }
    }

    /// Gets a string representation of the path.
    ///
    /// Returns `None` if the path is local and cannot be represented in UTF-8.
    pub fn to_str(&self) -> Option<&str> {
        match self {
            Self::Local(path) => path.to_str(),
            Self::Remote(url) => Some(url.as_str()),
        }
    }

    /// Converts the path to a local path.
    ///
    /// Returns `None` if the path is remote.
    pub fn as_local(&self) -> Option<&Path> {
        match self {
            Self::Local(path) => Some(path),
            Self::Remote(_) => None,
        }
    }

    /// Unwraps the path to a local path.
    ///
    /// # Panics
    ///
    /// Panics if the path is remote.
    pub fn unwrap_local(self) -> PathBuf {
        match self {
            Self::Local(path) => path,
            Self::Remote(_) => panic!("path is remote"),
        }
    }

    /// Converts the path to a remote URL.
    ///
    /// Returns `None` if the path is local.
    pub fn as_remote(&self) -> Option<&Url> {
        match self {
            Self::Local(_) => None,
            Self::Remote(url) => Some(url),
        }
    }

    /// Unwraps the path to a remote URL.
    ///
    /// # Panics
    ///
    /// Panics if the path is local.
    pub fn unwrap_remote(self) -> Url {
        match self {
            Self::Local(_) => panic!("path is local"),
            Self::Remote(url) => url,
        }
    }

    /// Gets the file name of the path.
    ///
    /// Returns `Ok(None)` if the path does not contain a file name (i.e. is
    /// root).
    ///
    /// Returns an error if the file name is not UTF-8.
    pub fn file_name(&self) -> Result<Option<&str>> {
        match self {
            Self::Local(path) => path
                .file_name()
                .map(|n| {
                    n.to_str().with_context(|| {
                        format!("path `{path}` is not UTF-8", path = path.display())
                    })
                })
                .transpose(),
            Self::Remote(url) => Ok(url.path_segments().and_then(|mut s| s.next_back())),
        }
    }

    /// Returns a display implementation for the path.
    pub fn display(&self) -> impl fmt::Display {
        struct Display<'a>(&'a EvaluationPath);

        impl fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self.0 {
                    EvaluationPath::Local(path) => write!(f, "{path}", path = path.display()),
                    EvaluationPath::Remote(url) => write!(f, "{url}"),
                }
            }
        }

        Display(self)
    }
}

impl FromStr for EvaluationPath {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Store `file` schemed URLs as local paths.
        if is_file_url(s) {
            let url = s
                .parse::<Url>()
                .with_context(|| format!("invalid `file` schemed URL `{s}`"))?;
            return url
                .to_file_path()
                .map(|p| Self::Local(clean(p)))
                .map_err(|_| anyhow!("URL `{s}` cannot be represented as a local file path"));
        }

        if let Some(url) = parse_url(s) {
            return Ok(Self::Remote(url));
        }

        Ok(Self::Local(clean(s)))
    }
}

impl TryFrom<EvaluationPath> for String {
    type Error = anyhow::Error;

    fn try_from(value: EvaluationPath) -> Result<Self, Self::Error> {
        match value {
            EvaluationPath::Local(path) => path
                .into_os_string()
                .into_string()
                .map_err(|_| anyhow!("path cannot be represented as a UTF-8 string")),
            EvaluationPath::Remote(url) => Ok(url.into()),
        }
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
        assert!(is_url("http://example.com/foo/bar/baz"));
        assert!(is_url("HtTp://example.com/foo/bar/baz"));
        assert!(is_url("HTTP://example.com/foo/bar/baz"));
        assert!(is_url("https://example.com/foo/bar/baz"));
        assert!(is_url("HtTpS://example.com/foo/bar/baz"));
        assert!(is_url("HTTPS://example.com/foo/bar/baz"));
        assert!(is_url("file:///foo/bar/baz"));
        assert!(is_url("FiLe:///foo/bar/baz"));
        assert!(is_url("FILE:///foo/bar/baz"));
        assert!(is_url("az://foo/bar/baz"));
        assert!(is_url("aZ://foo/bar/baz"));
        assert!(is_url("AZ://foo/bar/baz"));
        assert!(is_url("s3://foo/bar/baz"));
        assert!(is_url("S3://foo/bar/baz"));
        assert!(is_url("gs://foo/bar/baz"));
        assert!(is_url("gS://foo/bar/baz"));
        assert!(is_url("GS://foo/bar/baz"));
        assert!(!is_url("foo://foo/bar/baz"));
    }

    #[test]
    fn test_url_parsing() {
        assert_eq!(
            parse_url("http://example.com/foo/bar/baz")
                .map(String::from)
                .as_deref(),
            Some("http://example.com/foo/bar/baz")
        );
        assert_eq!(
            parse_url("https://example.com/foo/bar/baz")
                .map(String::from)
                .as_deref(),
            Some("https://example.com/foo/bar/baz")
        );
        assert_eq!(
            parse_url("file:///foo/bar/baz")
                .map(String::from)
                .as_deref(),
            Some("file:///foo/bar/baz")
        );
        assert_eq!(
            parse_url("az://foo/bar/baz").map(String::from).as_deref(),
            Some("az://foo/bar/baz")
        );
        assert_eq!(
            parse_url("s3://foo/bar/baz").map(String::from).as_deref(),
            Some("s3://foo/bar/baz")
        );
        assert_eq!(
            parse_url("gs://foo/bar/baz").map(String::from).as_deref(),
            Some("gs://foo/bar/baz")
        );
        assert_eq!(
            parse_url("foo://foo/bar/baz").map(String::from).as_deref(),
            None
        );
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
