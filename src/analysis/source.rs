//! Sources for a WDL documents used in analysis.

use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use path_clean::PathClean;
use url::Url;
use wdl::analysis::Analyzer;

/// The supported source schemes for both WDL source files and JSON input files.
const SUPPORTED_SOURCE_SCHEMES: &[&str] = &["https://", "http://", "file://"];

/// Helper to check if a given string starts with the given prefix, ignoring
/// ASCII case.
fn starts_with_ignore_ascii_case(s: &str, prefix: &str) -> bool {
    s.get(0..prefix.len())
        .map(|s| s.eq_ignore_ascii_case(prefix))
        .unwrap_or(false)
}

/// Determines if the given string is prefixed with a supported URL scheme for
/// source files.
pub(crate) fn is_supported_source_url(s: &str) -> bool {
    SUPPORTED_SOURCE_SCHEMES
        .iter()
        .any(|scheme| starts_with_ignore_ascii_case(s, scheme))
}

/// A source for an analysis.
#[derive(Clone, Debug)]
pub enum Source {
    /// The source is a local or remote file.
    File(Url),

    /// The source is a local directory.
    Directory(PathBuf),
}

impl Source {
    /// Attempts to reference the source as a URL.
    pub fn as_url(&self) -> Option<&Url> {
        match self {
            Source::File(url) => Some(url),
            Source::Directory(_) => None,
        }
    }

    /// Converts the source to a URL.
    ///
    /// For [`Source::File`], this clones the URL. For [`Source::Directory`],
    /// this converts the path to a `file://` URL.
    pub fn to_url(&self) -> Url {
        match self {
            Source::File(url) => url.clone(),
            Source::Directory(path) => {
                Url::from_directory_path(path).expect("directory path should convert to URL")
            }
        }
    }

    /// Registers the source within an [`Analyzer`].
    pub async fn register<T: Send + Clone + 'static>(
        self,
        analyzer: &mut Analyzer<T>,
    ) -> Result<()> {
        match self {
            Source::File(url) => analyzer.add_document(url).await,
            Source::Directory(path) => analyzer.add_directory(path).await,
        }
    }
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::File(url) => write!(f, "{url}"),
            Source::Directory(path) => write!(f, "{path}", path = path.display()),
        }
    }
}

impl std::str::FromStr for Source {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Only HTTP(S) and local files are supported for analysis.
        if is_supported_source_url(s) {
            return Ok(Self::File(
                s.parse().with_context(|| format!("invalid URL `{s}`"))?,
            ));
        }

        let path = Path::new(s);

        let path = std::path::absolute(path)
            .map_err(|_| anyhow!("failed to convert `{path}` to a URI", path = path.display()))
            .map(|path| path.clean())?;

        if !path.exists() {
            bail!("source file `{s}` does not exist");
        }

        if path.is_dir() {
            return Ok(Source::Directory(path));
        } else if path.is_file()
            && let Ok(url) = Url::from_file_path(&path)
        {
            return Ok(Source::File(url));
        }

        bail!("failed to convert `{s}` to a URI")
    }
}

impl Default for Source {
    fn default() -> Self {
        // Default to the current directory.
        Source::Directory(
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from(std::path::Component::CurDir.as_os_str())),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let path = std::path::absolute(file.path()).unwrap();

        let source = path.to_str().unwrap().parse::<Source>().unwrap();
        assert!(matches!(source, Source::File(_)));
        let url = source.as_url().unwrap();
        assert_eq!(url.scheme(), "file");
        assert_eq!(url.to_file_path().unwrap(), path);
    }

    #[test]
    fn directory() {
        let dir = tempfile::TempDir::new().unwrap();
        let name = dir.path().as_os_str().to_str().unwrap();

        assert!(matches!(name.parse().unwrap(),
            Source::Directory(path)
            if path.as_os_str().to_str().unwrap() == name));
    }

    #[test]
    fn url() {
        const EXAMPLE: &str = "https://example.com/";
        assert!(matches!(EXAMPLE.parse().unwrap(),
            Source::File(url)
            if url.as_str()
                == EXAMPLE
        ));
    }

    #[test]
    fn missing_file() {
        let err = "a-random-file-that-doesnt-exist.txt"
            .parse::<Source>()
            .unwrap_err();

        assert_eq!(
            err.to_string(),
            "source file `a-random-file-that-doesnt-exist.txt` does not exist"
        );
    }

    #[test]
    fn invalid_source() {
        let err = "".parse::<Source>().unwrap_err();

        assert_eq!(err.to_string(), "failed to convert `` to a URI");
    }
}
