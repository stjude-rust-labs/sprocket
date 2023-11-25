//! A unary sign.

use crate::v1::document::Expression;

/// A unary sign.
#[derive(Clone, Debug, Hash, Eq, Ord, PartialEq, PartialOrd)]
pub enum UnarySigned {
    /// Positive (`+`).
    Positive(Box<Expression>),

    /// Negative (`-`).
    Negative(Box<Expression>),
}
