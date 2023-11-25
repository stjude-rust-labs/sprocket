//! Metadata object values.

use std::collections::BTreeMap;

use grammar::v1::Rule;
use pest::iterators::Pair;
use wdl_grammar as grammar;
use wdl_macros::check_node;
use wdl_macros::extract_one;
use wdl_macros::unwrap_one;

use crate::v1::document::identifier::singular;
use crate::v1::document::identifier::singular::Identifier;
use crate::v1::document::metadata::value;
use crate::v1::document::metadata::Value;

/// An error related to an [`Object`].
#[derive(Debug)]
pub enum Error {
    /// An identifier error.
    Identifier(singular::Error),

    /// A value error.
    Value(value::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Identifier(err) => write!(f, "identifier error: {err}"),
            Error::Value(err) => write!(f, "value error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
type Result<T> = std::result::Result<T, Error>;

/// A metadata object value.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Object(BTreeMap<Identifier, Value>);

impl TryFrom<Pair<'_, grammar::v1::Rule>> for Object {
    type Error = Error;

    fn try_from(node: Pair<'_, grammar::v1::Rule>) -> Result<Self> {
        check_node!(node, metadata_object);
        let mut inner = BTreeMap::new();

        for node in node.into_inner() {
            match node.as_rule() {
                Rule::metadata_kv => {
                    // TODO: a clone is required here because Pest's `FlatPairs`
                    // type does not support creating an iterator without taking
                    // ownership (at the time of writing). This can be made
                    // better with a PR to Pest.
                    let key_node = extract_one!(node.clone(), metadata_key, metadata_kv)?;
                    let key = Identifier::try_from(unwrap_one!(key_node, metadata_key).as_str())
                        .map_err(Error::Identifier)?;

                    let value_node = extract_one!(node, metadata_value, metadata_kv)?;
                    let value = Value::try_from(value_node).map_err(Error::Value)?;

                    inner.insert(key, value);
                }
                Rule::COMMA => {}
                Rule::WHITESPACE => {}
                Rule::COMMENT => {}
                rule => unreachable!("parameter metadata should not contain {:?}", rule),
            }
        }

        Ok(Object(inner))
    }
}
