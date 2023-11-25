//! Metadata array values.

use grammar::v1::Rule;
use pest::iterators::Pair;
use wdl_grammar as grammar;
use wdl_macros::check_node;

use crate::v1::document::metadata::value;
use crate::v1::document::metadata::Value;

/// An error related to an [`Array`].
#[derive(Debug)]
pub enum Error {
    /// A value error.
    Value(value::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Value(err) => write!(f, "value error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
type Result<T> = std::result::Result<T, Error>;

/// A metadata array value.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Array(Vec<Value>);

impl TryFrom<Pair<'_, grammar::v1::Rule>> for Array {
    type Error = Error;

    fn try_from(node: Pair<'_, grammar::v1::Rule>) -> Result<Self> {
        check_node!(node, metadata_array);

        node.into_inner()
            .filter(|node| {
                !matches!(
                    node.as_rule(),
                    Rule::WHITESPACE | Rule::COMMENT | Rule::COMMA
                )
            })
            .map(|node| Value::try_from(node).map_err(Error::Value))
            .collect::<Result<Vec<_>>>()
            .map(Array)
    }
}
