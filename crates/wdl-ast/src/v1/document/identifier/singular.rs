//! Singular identifiers.

use std::borrow::Borrow;

use lazy_static::lazy_static;
use pest::iterators::Pair;
use regex::Regex;
use wdl_grammar as grammar;
use wdl_macros::check_node;

lazy_static! {
    static ref PATTERN: Regex = Regex::new(r"^[a-zA-Z][a-zA-Z0-9_]*$").unwrap();
}

/// An error related to an [`Identifier`].
#[derive(Debug)]
pub enum Error {
    /// Attempted to create an empty identifier.
    Empty,

    /// Attempted to create an identifier with an invalid format.
    InvalidFormat(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Empty => write!(f, "cannot create an empty identifier"),
            Error::InvalidFormat(format) => {
                write!(f, "invalid format for identifier: \"{format}\"")
            }
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
type Result<T> = std::result::Result<T, Error>;

/// A singular identifier.
///
/// An [`Identifier`] must match the pattern `^[a-zA-Z][a-zA-Z0-9_]*$`. If an ones
/// attempts to create an [`Identifier`] that does not match this pattern, an
/// [`Error::InvalidFormat`] is returned.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Identifier(String);

// Note: this is included because we want to allow looking up an [`Identifier`]
// using a `&str` in things like [`HashMap`]s (and similar).
impl Borrow<str> for Identifier {
    fn borrow(&self) -> &str {
        self.0.as_str()
    }
}

impl std::ops::Deref for Identifier {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// Note: when displaying an [`Identifier`] via the [`std::fmt::Debug`] trait,
// it's more clear to simply serialize the [`Identifier`] as is done in
// [`std::fmt::Display`] rather than to print it within the tuple struct.
impl std::fmt::Debug for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "\"{}\"", self)
    }
}

impl TryFrom<&str> for Identifier {
    type Error = Error;

    fn try_from(value: &str) -> std::prelude::v1::Result<Self, Self::Error> {
        ensure_valid(value)?;
        Ok(Identifier(value.to_owned()))
    }
}

impl TryFrom<String> for Identifier {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        ensure_valid(&value)?;
        Ok(Identifier(value))
    }
}

/// Ensures that a provided [`&str`](str) is a valid identifier. If it is not,
/// the appropriate error is returned.
fn ensure_valid(value: impl AsRef<str>) -> Result<()> {
    let value = value.as_ref();

    if value.is_empty() {
        return Err(Error::Empty);
    }

    if !PATTERN.is_match(value) {
        return Err(Error::InvalidFormat(value.to_string()));
    }

    Ok(())
}

impl TryFrom<Pair<'_, grammar::v1::Rule>> for Identifier {
    type Error = Error;

    fn try_from(node: Pair<'_, grammar::v1::Rule>) -> Result<Self> {
        check_node!(node, singular_identifier);
        Identifier::try_from(node.as_str().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_creates_a_identifier_with_a_valid_format() -> Result<()> {
        Identifier::try_from("hello_world")?;
        Identifier::try_from("a1b2c3")?;

        Ok(())
    }

    #[test]
    fn it_fails_to_create_an_empty_identifier() -> Result<()> {
        let err = Identifier::try_from("").unwrap_err();
        assert_eq!(
            err.to_string(),
            String::from("cannot create an empty identifier")
        );

        Ok(())
    }

    #[test]
    fn it_fails_to_create_invalid_identifiers() -> Result<()> {
        let err = Identifier::try_from("0123").unwrap_err();
        assert_eq!(
            err.to_string(),
            String::from("invalid format for identifier: \"0123\"")
        );

        let err = Identifier::try_from("_").unwrap_err();
        assert_eq!(
            err.to_string(),
            String::from("invalid format for identifier: \"_\"")
        );

        let err = Identifier::try_from("$hello").unwrap_err();
        assert_eq!(
            err.to_string(),
            String::from("invalid format for identifier: \"$hello\"")
        );

        Ok(())
    }

    #[test]
    fn it_derefences_as_a_string_and_as_str() {
        let identifier = Identifier::try_from("hello_world").unwrap();
        assert_eq!(identifier.as_str(), "hello_world");
    }
}
