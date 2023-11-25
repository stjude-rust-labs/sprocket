//! Values within a call body.

use crate::v1::document::Expression;

/// A value within a call [`Body`](super::Body).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Value {
    /// An expression.
    Expression(Expression),

    /// An implicit binding.
    ///
    /// In an implicit binding, the value is inferred to be the same identifier
    /// as the key to which the value belongs in the [`Body`](super::Body).
    ImplicitBinding,
}
