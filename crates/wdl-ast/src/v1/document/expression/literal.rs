//! A literal.

use ordered_float::OrderedFloat;

use crate::v1::document::identifier::singular::Identifier;

/// An literal value within an [`Expression`](super::Expression).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Literal {
    /// A boolean.
    Boolean(bool),

    /// An integer.
    Integer(i64),

    /// A float.
    Float(OrderedFloat<f64>),

    /// A string.
    String(String),

    /// None.
    None,

    /// An identifier.
    Identifier(Identifier),
}
