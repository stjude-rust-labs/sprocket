//! Qualified identifiers.

use pest::iterators::Pair;
use wdl_grammar as grammar;
use wdl_macros::check_node;

use crate::v1::document::identifier::singular;

/// The separator between the [`singular::Identifier`]s when serialized.
pub const SEPARATOR: &str = ".";

/// An error related to an [`Identifier`].
#[derive(Debug)]
pub enum Error {
    /// Attempted to create an empty identifier.
    Empty,

    /// Attempted to create a qualified identifier with an invalid format.
    InvalidFormat(String, String),

    /// A singular identifier error.
    ///
    /// Generally speaking, this error will be returned if there is any issue
    /// parsing the singular identifiers that comprise the qualified identifier.
    Singular(singular::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Empty => write!(f, "cannot create an empty identifier"),
            Error::InvalidFormat(value, reason) => {
                write!(f, "invalid format for \"{value}\": {reason}")
            }
            Error::Singular(err) => write!(f, "singular identifier error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
type Result<T> = std::result::Result<T, Error>;

/// A qualified identifier.
///
/// Qualified [`Identifier`]s are comprised of one or more singular
/// [`Identifier`](singular::Identifier)s that are joined together by
/// [`SEPARATOR`] (in the [`Identifier`]'s serialized form). These identifiers
/// are effectively used to enable namespacing of identifiers.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Identifier(Vec<singular::Identifier>);

impl std::fmt::Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            self.0
                .iter()
                .map(|identifier| identifier.as_str())
                .collect::<Vec<_>>()
                .join(SEPARATOR)
        )
    }
}

// Note: when displaying an [`Identifier`] via the [`std::fmt::Debug`] trait,
// it's more clear to simply serialize the [`Identifier`] as is done in
// [`std::fmt::Display`] rather than to print each element in the inner [`Vec`].
impl std::fmt::Debug for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "\"{}\"", self)
    }
}

impl TryFrom<&str> for Identifier {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        if value.is_empty() {
            return Err(Error::Empty);
        }

        if !value.contains(SEPARATOR) {
            return Err(Error::InvalidFormat(
                value.to_owned(),
                String::from("cannot create qualified identifier with no scope"),
            ));
        }

        value
            .split(SEPARATOR)
            .map(|identifier| singular::Identifier::try_from(identifier).map_err(Error::Singular))
            .collect::<Result<Identifier>>()
    }
}

// Note: this is implemented to facilitate collection via `collect()` on a set
// of singular [`Identifier`](singular::Identifier)s.
impl FromIterator<singular::Identifier> for Identifier {
    fn from_iter<T: IntoIterator<Item = singular::Identifier>>(iter: T) -> Self {
        Identifier(iter.into_iter().collect())
    }
}

impl TryFrom<Pair<'_, grammar::v1::Rule>> for Identifier {
    type Error = Error;

    fn try_from(node: Pair<'_, grammar::v1::Rule>) -> Result<Self> {
        check_node!(node, qualified_identifier);

        let mut nodes = node.into_inner().collect::<Vec<_>>();

        if nodes.is_empty() {
            return Err(Error::Empty);
        } else if nodes.len() == 1 {
            return Err(Error::InvalidFormat(
                // SAFTEY: we just ensured that exactly one node exists.
                nodes.pop().unwrap().as_str().to_owned(),
                String::from("cannot create qualified identifier with no scope"),
            ));
        }

        nodes
            .into_iter()
            .map(singular::Identifier::try_from)
            .collect::<std::result::Result<Identifier, _>>()
            .map_err(Error::Singular)
    }
}

#[cfg(test)]
mod tests {
    use wdl_macros::test::create_invalid_node_test;
    use wdl_macros::test::valid_node;

    use super::*;

    #[test]
    fn it_parses_from_a_supported_node_type() {
        valid_node!("hello.there.world", qualified_identifier, Identifier);
    }

    create_invalid_node_test!(
        "version 1.1\n\ntask hello { command <<<>>> }",
        document,
        qualified_identifier,
        Identifier,
        it_fails_to_parse_from_an_unsupported_node_type
    );

    #[test]
    fn it_collects_identifiers_into_a_qualified_identifier() {
        let identifiers = vec![
            String::from("hello"),
            String::from("there"),
            String::from("world"),
        ];

        let qualified = identifiers
            .into_iter()
            .map(crate::v1::document::identifier::singular::Identifier::try_from)
            .collect::<std::result::Result<Identifier, _>>()
            .unwrap();

        assert_eq!(qualified.to_string(), "hello.there.world");
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
    fn it_fails_to_create_a_qualified_identifier_from_a_singular_identifier() -> Result<()> {
        let err = Identifier::try_from("hello_world").unwrap_err();
        assert_eq!(
            err.to_string(),
            String::from("invalid format for \"hello_world\": cannot create qualified identifier with no scope")
        );

        Ok(())
    }
}
