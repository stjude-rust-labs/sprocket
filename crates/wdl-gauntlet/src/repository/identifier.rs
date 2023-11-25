//! Identifiers for repositories.

use serde::Deserialize;
use serde::Serialize;

/// The character that separates the organization from the repository name.
const SEPARATOR: char = '/';

/// A parse error related to an [`Identifier`].
#[derive(Debug)]
pub enum ParseError {
    /// Attempted to parse a [`Identifier`] from an invalid format.
    InvalidFormat(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::InvalidFormat(value) => write!(f, "invalid format: {value}"),
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

/// A repository identifier.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Identifier {
    /// The organization of the repository identifier.
    organization: String,

    /// The name of the repository identifier.
    name: String,
}

impl Identifier {
    /// Gets the repository name of this [`Identifier`] by reference.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Gets the organization name of this [`Identifier`] by reference.
    pub fn organization(&self) -> &str {
        self.organization.as_str()
    }
}

impl std::fmt::Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}{}", self.organization, SEPARATOR, self.name)
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
        let organization = parts.next().unwrap().to_string();
        let name = parts.next().unwrap().to_string();

        Ok(Self { organization, name })
    }
}
