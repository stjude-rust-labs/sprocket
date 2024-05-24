//! Identifiers for documents.

use serde::Deserialize;
use serde::Serialize;
use serde_with::serde_as;
use serde_with::DisplayFromStr;

use crate::repository;

/// The character that separates the repository from the path in the identifier.
const SEPARATOR: char = ':';

/// A parse error related to an [`Identifier`].
#[derive(Debug)]
pub enum ParseError {
    /// Attempted to parse a [`Identifier`] from an invalid format.
    InvalidFormat(String),

    /// An invalid repository identifier was provided.
    RepositoryIdentifier(repository::identifier::Error),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::InvalidFormat(value) => {
                write!(f, "invalid format: {value}")
            }
            ParseError::RepositoryIdentifier(err) => {
                write!(f, "repository identifier error: {err}")
            }
        }
    }
}

impl std::error::Error for ParseError {}

/// An error related to an [`Identifier`].
#[derive(Debug)]
pub enum Error {
    /// A parse error.
    Parse(ParseError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Parse(err) => write!(f, "parse error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A document identifier.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Identifier {
    /// The repository identifier.
    #[serde_as(as = "DisplayFromStr")]
    repository: repository::Identifier,

    /// The path within the repository.
    path: String,
}

impl Identifier {
    /// Creates a new [`Identifier`].
    pub fn new(repository: repository::Identifier, relative_path: String) -> Self {
        Self {
            repository,
            path: relative_path,
        }
    }

    /// Gets the [`repository::Identifier`] from this [`Identifier`] by
    /// reference.
    pub fn repository(&self) -> &repository::Identifier {
        &self.repository
    }
}

impl std::fmt::Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}{}", self.repository, SEPARATOR, self.path)
    }
}

impl std::str::FromStr for Identifier {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts = s.split(SEPARATOR).collect::<Vec<_>>();

        if parts.len() != 2 {
            return Err(Error::Parse(ParseError::InvalidFormat(s.to_string())));
        }

        let mut parts = parts.into_iter();

        // SAFETY: we just checked above that two elements exist, so this will
        // always unwrap.
        let repository = parts
            .next()
            .unwrap()
            .to_string()
            .parse::<repository::Identifier>()
            .map_err(|err| Error::Parse(ParseError::RepositoryIdentifier(err)))?;

        // SAFETY: we just checked above that two elements exist, so this will
        // always unwrap.
        let path = parts.next().unwrap().to_string();

        Ok(Self { repository, path })
    }
}
