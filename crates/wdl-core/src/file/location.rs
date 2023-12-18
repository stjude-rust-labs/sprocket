//! Locations.
//!
//! ## [`Position`]
//!
//! A [`Position`] is a row and a column within a file. [`Positions`](Position)
//! are the foundation of many of the broader [`Location`] types.
//!
//! ## [`Location`]
//!
//! A [`Location`] refers to coordinate within a file where an element
//! originated (or lack thereof). [`Locations`](Location) can be one of the
//! following:
//!
//! * [`Location::Unplaced`], meaning the entity associated with the location
//!   did not originate from any location within a file. This is generally
//!   useful when you'd like to represent the location of an element generated
//!   by code rather than parsed from a file.
//! * [`Location::Position`], meaning an entity originated at a single position
//!   within a file.
//! * [`Location::Span`], meaning an entity is represented by a range between a
//!   start and end position within a file.
//!
//! Within `wdl-core`, [`Locations`](Location) are generally used in conjunction
//! with the [`Located<E>`] type.
//!
//! ## [`Located<E>`]
//!
//! This module introduces [`Located<E>`]—a wrapper type that pairs entities
//! (`E`) with a [`Location`]. The [`Located`] type provides direct access to
//! the `E` value via dereferencing and exposes the associated [`Location`]
//! through the [`Located::location()`] method. Notably, trait implementations
//! (excluding [`Clone`]) focus solely on the inner `E` value, meaning
//! operations like comparison, hashing, and ordering do not consider the
//! [`Location`]. This ensures that the type is generally treated as the inner
//! `E` while also providing the context of the [`Location`] when desired.

mod located;
pub mod position;

pub use located::Located;
pub use position::Position;
use serde::Deserialize;
use serde::Serialize;

/// An error related to a [`Location`].
#[derive(Debug)]
pub enum Error {
    /// A position error.
    Position(position::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Position(err) => write!(f, "position error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A 1-based location.
///
/// See the [module documentation](crate::file::Location) for more information.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Location {
    /// No location.
    ///
    /// This is generally the case when an element was programmatically
    /// generated instead of parsed from an existing document.
    Unplaced,

    /// A single position.
    Position(Position),

    /// Spanning from a start location to an end location (inclusive).
    Span {
        /// The start position.
        start: Position,

        /// The end position (inclusive).
        end: Position,
    },
}

impl Location {
    /// Gets the byte range for the [`Location`].
    ///
    /// # Examples
    ///
    /// ```
    /// use std::num::NonZeroUsize;
    ///
    /// use wdl_core::file::location::Position;
    /// use wdl_core::file::Location;
    ///
    /// let location = Location::Unplaced;
    /// assert!(location.byte_range().is_none());
    ///
    /// let location = Location::Position(Position::new(
    ///     NonZeroUsize::try_from(1).unwrap(),
    ///     NonZeroUsize::try_from(1).unwrap(),
    ///     0,
    /// ));
    /// assert_eq!(location.byte_range(), Some(0..0));
    ///
    /// let location = Location::Span {
    ///     start: Position::new(
    ///         NonZeroUsize::try_from(1).unwrap(),
    ///         NonZeroUsize::try_from(1).unwrap(),
    ///         0,
    ///     ),
    ///     end: Position::new(
    ///         NonZeroUsize::try_from(3).unwrap(),
    ///         NonZeroUsize::try_from(4).unwrap(),
    ///         6,
    ///     ),
    /// };
    /// assert_eq!(location.byte_range(), Some(0..6));
    /// ```
    pub fn byte_range(&self) -> Option<std::ops::Range<usize>> {
        match self {
            Location::Unplaced => None,
            Location::Position(position) => Some(position.byte_no()..position.byte_no()),
            Location::Span { start, end } => Some(start.byte_no()..end.byte_no()),
        }
    }

    /// Converts a [`Location`] to a [`String`] (if it can be converted).
    ///
    /// Notably, this method conflicts with and does not implement
    /// [`std::string::ToString`]. This was an intentional decision, as that
    /// trait assumes that the struct may _always_ be able to be converted into
    /// a [`String`].
    ///
    /// # Examples
    ///
    /// ```
    /// use std::num::NonZeroUsize;
    ///
    /// use wdl_core::file::location::Position;
    /// use wdl_core::file::Location;
    ///
    /// assert_eq!(Location::Unplaced.to_string(), None);
    /// assert_eq!(
    ///     Location::Position(Position::new(
    ///         NonZeroUsize::try_from(1).unwrap(),
    ///         NonZeroUsize::try_from(2).unwrap(),
    ///         1
    ///     ))
    ///     .to_string(),
    ///     Some(String::from("1:2"))
    /// );
    /// assert_eq!(
    ///     Location::Span {
    ///         start: Position::new(
    ///             NonZeroUsize::try_from(1).unwrap(),
    ///             NonZeroUsize::try_from(2).unwrap(),
    ///             1
    ///         ),
    ///         end: Position::new(
    ///             NonZeroUsize::try_from(3).unwrap(),
    ///             NonZeroUsize::try_from(4).unwrap(),
    ///             6
    ///         )
    ///     }
    ///     .to_string(),
    ///     Some(String::from("1:2-3:4"))
    /// );
    /// ```
    pub fn to_string(&self) -> Option<String> {
        match self {
            Location::Unplaced => None,
            Location::Position(position) => Some(format!("{}", position)),
            Location::Span { start, end } => Some(format!("{}-{}", start, end)),
        }
    }
}

impl TryFrom<pest::Span<'_>> for Location {
    type Error = Error;

    fn try_from(span: pest::Span<'_>) -> Result<Self, Self::Error> {
        let start = Position::try_from(span.start_pos()).map_err(Error::Position)?;
        let end = Position::try_from(span.end_pos()).map_err(Error::Position)?;

        Ok(Location::Span { start, end })
    }
}

impl std::fmt::Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Location::Unplaced => write!(f, ""),
            Location::Position(position) => write!(f, "{}", position),
            Location::Span { start, end } => write!(f, "{}-{}", start, end),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use super::*;

    #[test]
    fn display_file() {
        assert_eq!(Location::Unplaced.to_string(), None);
    }

    #[test]
    fn display_position() {
        let result = Location::Position(Position::new(
            NonZeroUsize::try_from(1).unwrap(),
            NonZeroUsize::try_from(1).unwrap(),
            0,
        ))
        .to_string();
        assert_eq!(result, Some(String::from("1:1")));
    }

    #[test]
    fn display_span() {
        let result = Location::Span {
            start: Position::new(
                NonZeroUsize::try_from(1).unwrap(),
                NonZeroUsize::try_from(1).unwrap(),
                0,
            ),
            end: Position::new(
                NonZeroUsize::try_from(5).unwrap(),
                NonZeroUsize::try_from(5).unwrap(),
                24,
            ),
        }
        .to_string();
        assert_eq!(result, Some(String::from("1:1-5:5")));
    }
}
