use std::fmt;
use std::path::Path;

use anyhow::Context as _;
use anyhow::anyhow;

use crate::path::EvaluationPath;

/// Represents an absolute path to a file or directory on the host file system
/// or a URL to a remote file or directory.
///
/// The host in this context is where the WDL evaluation is taking place, as
/// opposed to a containerized and/or remote execution environment managed by a
/// backend.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HostPath {
    /// The path representation is kept private so that this module's
    /// constructors can ensure that shell expansion and path validation has
    /// been performed.
    path: EvaluationPath,
}

impl HostPath {
    /// Perform shell expansion on a string, and attempt to construct a
    /// [`HostPath`] from the result.
    ///
    /// Returns an error if the string is not a supported URL or an absolute
    /// path, or if shell expansion fails.
    pub fn new(path: &str) -> Result<Self, anyhow::Error> {
        let shell_expanded = shellexpand::full(path)
            .with_context(|| format!("failed to shell-expand path `{path}`"))?;
        if let Ok(url) = shell_expanded.parse() {
            Ok(Self {
                path: EvaluationPath::Remote(url),
            })
        } else {
            let path = Path::new(path);
            if path.is_absolute() {
                Ok(Self {
                    path: EvaluationPath::Local(path.to_path_buf()),
                })
            } else {
                Err(anyhow!(
                    "host path was not a URL or absolute path: {path:?}"
                ))
            }
        }
    }

    /// Gets the string representation of the host path.
    pub fn as_str(&self) -> &str {
        // As long as we only create this type from a valid string, it will remain a
        // valid string
        self.path.to_str().expect("HostPath was not a valid string")
    }
}

impl fmt::Display for HostPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.path.fmt(f)
    }
}
