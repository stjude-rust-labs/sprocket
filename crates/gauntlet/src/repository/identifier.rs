//! Identifiers for repositories.

use anyhow::Context;

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
            ParseError::InvalidFormat(value) => {
                write!(
                    f,
                    "expected a repository identifier in the format `<organization>/<name>`, \
                     found `{value}`"
                )
            }
        }
    }
}

/// A repository identifier.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
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
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (organization, name) = s.split_once(SEPARATOR).with_context(|| {
            format!("invalid repository identifier `{s}`, expected format `<org>/<repo>`")
        })?;

        Ok(Self {
            organization: organization.into(),
            name: name.into(),
        })
    }
}
