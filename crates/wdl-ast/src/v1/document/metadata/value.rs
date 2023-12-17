//! Metadata values.

use grammar::v1::Rule;
use pest::iterators::Pair;
use wdl_grammar as grammar;
use wdl_macros::check_node;
use wdl_macros::dive_one;
use wdl_macros::unwrap_one;

mod array;
mod object;

pub use array::Array;
pub use object::Object;

/// An error related to a [`Value`].
#[derive(Debug)]
pub enum Error {
    /// An array error.
    Array(Box<array::Error>),

    /// An object error.
    Object(Box<object::Error>),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Array(err) => write!(f, "array error: {err}"),
            Error::Object(err) => write!(f, "object error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
type Result<T> = std::result::Result<T, Error>;

/// A metadata value.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Value {
    /// A string.
    String(String),

    /// An integer.
    Integer(String),

    /// A float.
    Float(String),

    /// A boolean.
    Boolean(bool),

    /// Null.
    Null,

    /// An object.
    Object(Object),

    /// An array.
    Array(Array),
}

impl TryFrom<Pair<'_, grammar::v1::Rule>> for Value {
    type Error = Error;

    fn try_from(node: Pair<'_, grammar::v1::Rule>) -> Result<Self> {
        check_node!(node, metadata_value);
        let node = unwrap_one!(node, metadata_value);

        match node.as_rule() {
            Rule::string => {
                let inner = dive_one!(node, string_inner, string);
                Ok(Value::String(inner.as_str().to_owned()))
            }
            Rule::integer => Ok(Value::Integer(node.as_str().to_owned())),
            Rule::float => Ok(Value::Float(node.as_str().to_owned())),
            Rule::boolean => match node.as_str() {
                "true" => Ok(Value::Boolean(true)),
                "false" => Ok(Value::Boolean(false)),
                value => {
                    unreachable!("unknown boolean literal value: {}", value)
                }
            },
            Rule::null => Ok(Value::Null),
            Rule::metadata_object => {
                let object = Object::try_from(node)
                    .map_err(Box::new)
                    .map_err(Error::Object)?;
                Ok(Value::Object(object))
            }
            Rule::metadata_array => {
                let array = Array::try_from(node)
                    .map_err(Box::new)
                    .map_err(Error::Array)?;
                Ok(Value::Array(array))
            }
            rule => unreachable!("workflow metadata value should not contain {:?}", rule),
        }
    }
}
