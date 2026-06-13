//! Identifiers for documents.

use std::path::MAIN_SEPARATOR;

use anyhow::Context;

use crate::repository;

/// The character that separates the repository from the path in the identifier.
const SEPARATOR: char = ':';

/// A document identifier.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Identifier {
    /// The repository identifier.
    repository: repository::Identifier,

    /// The path within the repository.
    path: String,
}

impl Identifier {
    /// Creates a new [`Identifier`].
    pub fn new(repository: repository::Identifier, relative_path: impl AsRef<str>) -> Self {
        // NOTE: the tests are stored using UNIX filepath conventions (namely,
        // with `/` as the delimiter), so we need to replace the separators on
        // Windows with this.
        let path = relative_path.as_ref().replace(MAIN_SEPARATOR, "/");
        let path = path.strip_prefix("/").unwrap_or(&path);

        Self {
            repository,
            // Ensure the path always starts with `/`
            path: format!("/{path}"),
        }
    }

    /// Gets the [`repository::Identifier`] from this [`Identifier`] by
    /// reference.
    pub fn repository(&self) -> &repository::Identifier {
        &self.repository
    }

    /// Gets the path of the document.
    pub fn path(&self) -> &str {
        &self.path
    }
}

impl std::fmt::Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{repo}{SEPARATOR}{path}",
            repo = self.repository,
            path = self.path
        )
    }
}

impl std::str::FromStr for Identifier {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (repository, path) = s.split_once(SEPARATOR).with_context(|| {
            format!("invalid document identifier `{s}`: expected format `<repo>:<path>")
        })?;

        Ok(Self {
            repository: repository.parse()?,
            path: path.to_string(),
        })
    }
}
