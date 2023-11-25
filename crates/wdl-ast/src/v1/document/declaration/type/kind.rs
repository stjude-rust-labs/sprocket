//! Kinds of WDL [`Type`](super::Type)s.

use crate::v1::document::identifier::singular::Identifier;

/// A kind of WDL [`Type`](super::Type).
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Kind {
    /// A map.
    Map,

    /// An array.
    Array,

    /// A pair.
    Pair,

    /// A string.
    String,

    /// A file.
    File,

    /// A boolean.
    Boolean,

    /// An integer.
    Integer,

    /// A float.
    Float,

    /// An object.
    Object,

    /// A struct.
    Struct(Identifier),
}
