//! Kinds of concern codes.

use serde::Deserialize;
use serde::Serialize;

/// A kind of concern code.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Kind {
    /// An error concern code.
    Error,

    /// A warning concern code.
    Warning,
}

impl Kind {
    /// Gets the prefix for this [`Kind`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::code::Kind;
    ///
    /// assert_eq!(Kind::Error.prefix(), 'E');
    /// assert_eq!(Kind::Warning.prefix(), 'W');
    /// ```
    pub fn prefix(&self) -> char {
        match self {
            Kind::Error => 'E',
            Kind::Warning => 'W',
        }
    }
}
