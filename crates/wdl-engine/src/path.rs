//! Representation of evaluation paths that support URLs.

use std::fmt;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use path_clean::PathClean;
use url::Url;

use crate::HostPath;

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

/// Determines if the given local path is a symbolic link whose target cannot
/// be resolved.
pub(crate) fn is_broken_symlink(path: &Path) -> bool {
    path.is_symlink() && !path.exists()
}

/// Normalizes a local path while preserving symbolic links traversed before
/// `..`.
///
/// When `path` contains no `..` components, this simply returns `path.clean()`.
///
/// Otherwise, `path` is split at its last `..` component into `(prefix, tail)`
/// and `prefix` is canonicalized on the filesystem:
/// - If `prefix` and `prefix.clean()` canonicalize to the same directory (no
///   symlink was traversed, including platform aliases like `/var` vs
///   `/private/var` on macOS or non-verbatim vs `\\?\` on Windows), returns
///   `path.clean()` so existing path prefixes are preserved.
/// - Otherwise, returns `canonical_prefix.join(tail).clean()` so symbolic links
///   are followed before resolving `..` without dereferencing `tail`
///   (preserving final symlinks, broken symlinks, and non-existent suffixes).
/// - If canonicalizing `prefix` fails, falls back to `path.clean()`.
fn normalize_local_path(path: PathBuf) -> PathBuf {
    let Some(last_parent_idx) = path
        .components()
        .enumerate()
        .filter_map(|(i, c)| matches!(c, Component::ParentDir).then_some(i))
        .last()
    else {
        return path.clean();
    };

    let mut prefix = PathBuf::new();
    let mut tail = PathBuf::new();
    for (i, component) in path.components().enumerate() {
        if i <= last_parent_idx {
            prefix.push(component);
        } else {
            tail.push(component);
        }
    }

    let lexical = path.clean();
    if let Ok(canonical_prefix) = std::fs::canonicalize(&prefix) {
        let lexical_prefix = prefix.clean();
        let lexical_prefix_path = if lexical_prefix.as_os_str().is_empty() {
            Path::new(".")
        } else {
            lexical_prefix.as_path()
        };

        if std::fs::canonicalize(lexical_prefix_path).ok().as_ref() == Some(&canonical_prefix) {
            return lexical;
        }

        return canonical_prefix.join(tail).clean();
    }

    lexical
}

/// Represents the kind of an evaluation path.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum EvaluationPathKind {
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
            return Ok(Self(EvaluationPathKind::Local(normalize_local_path(
                p.to_path_buf(),
            ))));
        }

        match &self.0 {
            EvaluationPathKind::Local(dir) => Ok(Self(EvaluationPathKind::Local(
                normalize_local_path(dir.join(path)),
            ))),
            EvaluationPathKind::Remote(dir) => Ok(Self(
                dir.join(path)
                    .map(EvaluationPathKind::Remote)
                    .with_context(|| format!("failed to join `{path}` to URL `{dir}`"))?,
            )),
        }
    }

    /// Resolves a relative path against this base directory.
    ///
    /// Returns `None` if `path` is empty, already absolute, a supported URL,
    /// or cannot be joined and represented as UTF-8.
    pub(crate) fn resolve_relative(&self, path: &str) -> Option<HostPath> {
        if path.is_empty() || is_supported_url(path) || Path::new(path).is_absolute() {
            return None;
        }

        let joined = self.join(path).ok()?;
        String::try_from(joined).ok().map(HostPath::new)
    }

    /// Gets the underlying evaluation path kind.
    pub(crate) fn kind(&self) -> &EvaluationPathKind {
        &self.0
    }

    /// Converts to the underlying evaluation path kind.
    pub(crate) fn into_kind(self) -> EvaluationPathKind {
        self.0
    }

    /// Gets a string representation of the path.
    ///
    /// Returns `None` if the path can't be represented as UTF-8.
    pub fn as_str(&self) -> Option<&str> {
        match &self.0 {
            EvaluationPathKind::Local(path) => path.to_str(),
            EvaluationPathKind::Remote(url) => Some(url.as_str()),
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

                // Keep the trailing slash so that joining relative paths
                // resolves within the parent directory
                if let Ok(mut segments) = url.path_segments_mut() {
                    segments.pop_if_empty().pop().push("");
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
mod tests {
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

    #[test]
    fn test_evaluation_path_parent_of() {
        let p = EvaluationPath::parent_of("https://example.com/foo/bar/main.wdl")
            .expect("should have parent");
        assert_eq!(p.to_string(), "https://example.com/foo/bar/");
        assert_eq!(
            p.join("data/x.txt").expect("should join").to_string(),
            "https://example.com/foo/bar/data/x.txt"
        );

        let p =
            EvaluationPath::parent_of("https://example.com/main.wdl").expect("should have parent");
        assert_eq!(p.to_string(), "https://example.com/");

        assert!(EvaluationPath::parent_of("https://example.com/").is_none());

        #[cfg(unix)]
        {
            let p = EvaluationPath::parent_of("/foo/bar/main.wdl").expect("should have parent");
            assert_eq!(
                p.join("data/x.txt").expect("should join").to_string(),
                "/foo/bar/data/x.txt"
            );
        }
    }

    #[test]
    fn test_evaluation_path_resolve_relative() {
        let p: EvaluationPath = "foo/bar/baz".parse().expect("should parse");
        assert_eq!(
            p.resolve_relative("data/hello.txt")
                .expect("should resolve relative")
                .as_str(),
            p.join("data/hello.txt").expect("should join").to_string()
        );
        assert_eq!(
            p.resolve_relative("qux/../quux")
                .expect("should resolve relative")
                .as_str(),
            p.join("qux/../quux").expect("should join").to_string()
        );
        assert!(p.resolve_relative("").is_none());
        assert!(p.resolve_relative("https://example.com/a.txt").is_none());

        #[cfg(unix)]
        assert!(p.resolve_relative("/abs/path.txt").is_none());

        #[cfg(windows)]
        assert!(p.resolve_relative(r"C:\abs\path.txt").is_none());

        let remote: EvaluationPath = "https://example.com/foo/bar/"
            .parse()
            .expect("should parse");
        assert_eq!(
            remote
                .resolve_relative("qux/../quux")
                .expect("should resolve relative remote")
                .as_str(),
            "https://example.com/foo/bar/quux"
        );
    }

    #[test]
    fn test_join_preserves_lexical_path_for_non_symlink_parent() {
        let root = tempfile::tempdir().expect("failed to create tempdir");
        let a = root.path().join("a");
        let b = root.path().join("b");
        std::fs::create_dir(&a).expect("failed to create dir a");
        std::fs::create_dir(&b).expect("failed to create dir b");

        let base = EvaluationPath::from_local_path(a);
        assert_eq!(base.join("../b").expect("should join").unwrap_local(), b);
    }

    #[cfg(unix)]
    #[test]
    fn test_join_preserves_symlink_parent_traversal() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().expect("failed to create tempdir");
        let data = dir.path().join("data");
        let nested = data.join("nested");
        std::fs::create_dir_all(&nested).expect("failed to create nested dir");
        std::fs::write(dir.path().join("hello.txt"), "wrong").expect("failed to write wrong file");
        std::fs::write(data.join("hello.txt"), "right").expect("failed to write right file");
        symlink(&nested, dir.path().join("link")).expect("failed to create symlink");

        let base = EvaluationPath::from_local_path(dir.path().to_path_buf());
        let canonical_data = std::fs::canonicalize(&data).expect("failed to canonicalize data dir");

        let joined = base
            .join("link/../hello.txt")
            .expect("should join")
            .unwrap_local();
        assert_eq!(
            std::fs::read_to_string(&joined).expect("failed to read joined path"),
            "right"
        );

        // Missing suffix after `link/..` should still resolve `link/..` through
        // the symlink rather than collapsing lexically.
        let missing = base
            .join("link/../no_such_dir/hello.txt")
            .expect("should join")
            .unwrap_local();
        assert_eq!(missing, canonical_data.join("no_such_dir/hello.txt"));

        // Broken final symlink name should still resolve its parent through
        // `link/..`.
        symlink("missing", data.join("broken")).expect("failed to create broken symlink");
        let broken = base
            .join("link/../broken")
            .expect("should join")
            .unwrap_local();
        assert_eq!(broken, canonical_data.join("broken"));
    }
}
