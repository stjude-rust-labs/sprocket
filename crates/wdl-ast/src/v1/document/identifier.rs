//! Identifiers.
//!
//! Identifiers come in two flavors:
//!
//! * [Singular](singular::Identifier), which represent a non-namespaced
//!   identifier that matches the pattern `^[a-zA-Z][a-zA-Z0-9_]*$`, and
//! * [Qualified](qualified::Identifier), which represent multiple singular
//!   identifiers concatenated by a [seperator](qualified::SEPARATOR)
//!   (effectively, namespaced identifiers).

use grammar::v1::Rule;
use pest::iterators::Pair;
use wdl_grammar as grammar;

pub mod qualified;
pub mod singular;

/// An error related to an [`Identifier`].
#[derive(Debug)]
pub enum Error {
    /// A qualified identifier error.
    Qualified(qualified::Error),

    /// A singular identifier error.
    Singular(singular::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Qualified(err) => {
                write!(f, "qualified identifier error: {err}")
            }
            Error::Singular(err) => {
                write!(f, "singular identifier error: {err}")
            }
        }
    }
}

impl std::error::Error for Error {}

/// An identifier.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Identifier {
    /// A singular identifier.
    Singular(singular::Identifier),

    /// A qualified identifier.
    Qualified(qualified::Identifier),
}

impl Identifier {
    /// Returns a reference to the [singular identifier](singular::Identifier)
    /// wrapped in [`Some`] if the [`Identifier`] is an
    /// [`Identifier::Singular`]. Else, [`None`] is returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::identifier::singular;
    /// use ast::v1::document::Identifier;
    /// use wdl_ast as ast;
    ///
    /// let identifier = Identifier::Singular(singular::Identifier::try_from("hello_world")?);
    ///
    /// assert_eq!(
    ///     identifier
    ///         .as_singular()
    ///         .map(|identifier| identifier.as_str()),
    ///     Some("hello_world")
    /// );
    /// assert_eq!(identifier.as_qualified(), None);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn as_singular(&self) -> Option<&singular::Identifier> {
        match self {
            Identifier::Singular(ref identifier) => Some(identifier),
            _ => None,
        }
    }

    /// Consumes `self` and returns the [singular
    /// identifier](singular::Identifier) wrapped in [`Some`] if the
    /// [`Identifier`] is an [`Identifier::Singular`]. Else, [`None`] is
    /// returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::identifier::singular;
    /// use ast::v1::document::Identifier;
    /// use wdl_ast as ast;
    ///
    /// let identifier = Identifier::Singular(singular::Identifier::try_from("hello_world")?);
    ///
    /// assert_eq!(
    ///     identifier.clone().into_singular(),
    ///     Some(singular::Identifier::try_from("hello_world")?)
    /// );
    /// assert_eq!(identifier.into_qualified(), None);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn into_singular(self) -> Option<singular::Identifier> {
        match self {
            Identifier::Singular(identifier) => Some(identifier),
            _ => None,
        }
    }

    /// Returns a reference to the [qualified identifier](qualified::Identifier)
    /// wrapped in [`Some`] if the [`Identifier`] is an
    /// [`Identifier::Qualified`]. Else, [`None`] is returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::identifier::qualified;
    /// use ast::v1::document::Identifier;
    /// use wdl_ast as ast;
    ///
    /// let identifier = Identifier::Qualified(qualified::Identifier::try_from("hello.there.world")?);
    ///
    /// assert_eq!(identifier.as_singular(), None);
    /// assert_eq!(
    ///     identifier
    ///         .as_qualified()
    ///         .map(|identifier| identifier.to_string()),
    ///     Some(String::from("hello.there.world"))
    /// );
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn as_qualified(&self) -> Option<&qualified::Identifier> {
        match self {
            Identifier::Qualified(ref identifier) => Some(identifier),
            _ => None,
        }
    }

    /// Consumes `self` and returns the [qualified
    /// identifier](qualified::Identifier) wrapped in [`Some`] if the
    /// [`Identifier`] is an [`Identifier::Qualified`]. Else, [`None`] is
    /// returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::identifier::qualified;
    /// use ast::v1::document::Identifier;
    /// use wdl_ast as ast;
    ///
    /// let identifier =
    ///     Identifier::Qualified(qualified::Identifier::try_from("hello.there.world").unwrap());
    ///
    /// assert_eq!(
    ///     identifier.clone().into_qualified(),
    ///     Some(qualified::Identifier::try_from("hello.there.world").unwrap())
    /// );
    /// assert_eq!(identifier.into_singular(), None);
    /// ```
    pub fn into_qualified(self) -> Option<qualified::Identifier> {
        match self {
            Identifier::Qualified(identifier) => Some(identifier),
            _ => None,
        }
    }
}

impl std::fmt::Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Identifier::Singular(identifier) => write!(f, "{identifier}"),
            Identifier::Qualified(identifier) => write!(f, "{identifier}"),
        }
    }
}

impl From<singular::Identifier> for Identifier {
    fn from(value: singular::Identifier) -> Self {
        Identifier::Singular(value)
    }
}

impl From<qualified::Identifier> for Identifier {
    fn from(value: qualified::Identifier) -> Self {
        Identifier::Qualified(value)
    }
}

impl TryFrom<Pair<'_, grammar::v1::Rule>> for Identifier {
    type Error = Error;

    fn try_from(node: Pair<'_, grammar::v1::Rule>) -> Result<Self, Self::Error> {
        match node.as_rule() {
            Rule::singular_identifier => {
                let identifier = singular::Identifier::try_from(node).map_err(Error::Singular)?;
                Ok(Identifier::Singular(identifier))
            }
            Rule::qualified_identifier => {
                let identifier = qualified::Identifier::try_from(node).map_err(Error::Qualified)?;
                Ok(Identifier::Qualified(identifier))
            }
            node => {
                panic!("identifier cannot be parsed from node type {:?}", node)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_parses_from_a_supported_node_type() {
        let identifier =
            wdl_macros::test::valid_node!("hello_world", singular_identifier, Identifier);
        assert_eq!(identifier.into_singular().unwrap().as_str(), "hello_world");

        wdl_macros::test::valid_node!("hello.there.world", qualified_identifier, Identifier);
    }

    wdl_macros::test::create_invalid_node_test!(
        "version 1.1\n\ntask hello { command <<<>>> }",
        document,
        identifier,
        Identifier,
        it_fails_to_parse_from_an_unsupported_node_type
    );
}
