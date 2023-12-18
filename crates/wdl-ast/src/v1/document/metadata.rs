//! Metadata.

use std::collections::BTreeMap;

use grammar::v1::Rule;
use pest::iterators::Pair;
use wdl_core::file::location::Located;
use wdl_core::file::Location;
use wdl_grammar as grammar;
use wdl_macros::extract_one;
use wdl_macros::unwrap_one;

use crate::v1::document::identifier::singular;
use crate::v1::document::identifier::singular::Identifier;

pub mod value;

pub use value::Value;

/// An error related to a [`Metadata`].
#[derive(Debug)]
pub enum Error {
    /// An identifier error.
    Identifier(singular::Error),

    /// A location error.
    Location(wdl_core::file::location::Error),

    /// A value error.
    Value(value::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Identifier(err) => write!(f, "identifier error: {err}"),
            Error::Location(err) => write!(f, "location error: {err}"),
            Error::Value(err) => write!(f, "value error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
type Result<T> = std::result::Result<T, Error>;

/// The inner map for [`Metadata`].
type Map = BTreeMap<Located<Identifier>, Located<Value>>;

/// A metadata.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Metadata(Map);

impl From<Map> for Metadata {
    fn from(metadata: Map) -> Self {
        Metadata(metadata)
    }
}

impl Metadata {
    /// Returns the inner value of the [`Metadata`] by reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    ///
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::metadata::Value;
    /// use ast::v1::document::Metadata;
    /// use wdl_ast as ast;
    /// use wdl_core::file::location::Located;
    ///
    /// let mut map = BTreeMap::new();
    /// map.insert(
    ///     Located::unplaced(Identifier::try_from("hello").unwrap()),
    ///     Located::unplaced(Value::String(String::from("world"))),
    /// );
    /// map.insert(
    ///     Located::unplaced(Identifier::try_from("foo").unwrap()),
    ///     Located::unplaced(Value::Null),
    /// );
    ///
    /// let metadata = Metadata::from(map);
    ///
    /// let inner = metadata.inner();
    ///
    /// assert_eq!(
    ///     inner
    ///         .get(&Identifier::try_from("hello").unwrap())
    ///         .unwrap()
    ///         .inner(),
    ///     &Value::String(String::from("world"))
    /// );
    /// assert_eq!(
    ///     inner
    ///         .get(&Identifier::try_from("foo").unwrap())
    ///         .unwrap()
    ///         .inner(),
    ///     &Value::Null
    /// );
    /// assert_eq!(inner.get(&Identifier::try_from("baz").unwrap()), None);
    /// ```
    pub fn inner(&self) -> &Map {
        &self.0
    }

    /// Consumes `self` and returns the inner value of the [`Metadata`].
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    ///
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::metadata::Value;
    /// use ast::v1::document::Metadata;
    /// use wdl_ast as ast;
    /// use wdl_core::file::location::Located;
    ///
    /// let mut map = BTreeMap::new();
    /// map.insert(
    ///     Located::unplaced(Identifier::try_from("hello").unwrap()),
    ///     Located::unplaced(Value::String(String::from("world"))),
    /// );
    /// map.insert(
    ///     Located::unplaced(Identifier::try_from("foo").unwrap()),
    ///     Located::unplaced(Value::Null),
    /// );
    ///
    /// let metadata = Metadata::from(map.clone());
    ///
    /// assert_eq!(metadata.into_inner(), map);
    /// ```
    pub fn into_inner(self) -> Map {
        self.0
    }
}

impl TryFrom<Pair<'_, grammar::v1::Rule>> for Metadata {
    type Error = Error;

    fn try_from(node: Pair<'_, grammar::v1::Rule>) -> Result<Self> {
        match node.as_rule() {
            Rule::metadata => {}
            Rule::parameter_metadata => {}
            rule => panic!(
                "{} cannot be parsed from node type {:?}",
                stringify!($type_),
                rule
            ),
        }

        let mut metadata = Map::default();

        for node in node.into_inner() {
            match node.as_rule() {
                Rule::metadata_kv => {
                    //================//
                    // Key extraction //
                    //================//

                    // TODO: a clone is required here because Pest's `FlatPairs`
                    // type does not support creating an iterator without taking
                    // ownership (at the time of writing). This can be made
                    // better with a PR to Pest.
                    let key_node = extract_one!(node.clone(), metadata_key, metadata_kv)?;
                    let location =
                        Location::try_from(key_node.as_span()).map_err(Error::Location)?;
                    let identifier =
                        Identifier::try_from(unwrap_one!(key_node, metadata_key).as_str())
                            .map_err(Error::Identifier)?;
                    let key = Located::new(identifier, location);

                    //==================//
                    // Value extraction //
                    //==================//

                    let value_node = extract_one!(node, metadata_value, metadata_kv)?;
                    let location =
                        Location::try_from(value_node.as_span()).map_err(Error::Location)?;
                    let value = Value::try_from(value_node).map_err(Error::Value)?;
                    let value = Located::new(value, location);

                    metadata.insert(key, value);
                }
                Rule::WHITESPACE => {}
                Rule::COMMENT => {}
                rule => unreachable!("parameter metadata should not contain {:?}", rule),
            }
        }

        Ok(Metadata(metadata))
    }
}
