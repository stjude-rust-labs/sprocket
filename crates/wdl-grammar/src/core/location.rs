//! Locations within a [`File`](std::fs::File).

use std::num::NonZeroUsize;

/// A location within a [`File`](std::fs::File).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Location {
    /// The entire file.
    ///
    /// In other words, associated with all lines and columns in a file.
    File,

    /// A line within a file.
    ///
    /// In other words, a particular line and all columns.
    Line(NonZeroUsize),

    /// A line and column within a file.
    LineCol {
        /// The line.
        line_no: NonZeroUsize,

        /// The column.
        col_no: NonZeroUsize,
    },
}

impl Location {
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
    /// use wdl_grammar as grammar;
    ///
    /// use std::num::NonZeroUsize;
    ///
    /// use grammar::core::Location;
    ///
    /// assert_eq!(Location::File.to_string(), None);
    /// assert_eq!(
    ///     Location::Line(NonZeroUsize::try_from(1).unwrap()).to_string(),
    ///     Some(String::from("1:*"))
    /// );
    /// assert_eq!(
    ///     Location::LineCol {
    ///         line_no: NonZeroUsize::try_from(1).unwrap(),
    ///         col_no: NonZeroUsize::try_from(1).unwrap(),
    ///     }
    ///     .to_string(),
    ///     Some(String::from("1:1"))
    /// );
    /// ```
    pub fn to_string(&self) -> Option<String> {
        match self {
            Location::File => None,
            Location::Line(line_no) => Some(format!("{}:*", line_no)),
            Location::LineCol { line_no, col_no } => Some(format!("{}:{}", line_no, col_no)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_file() {
        assert_eq!(Location::File.to_string(), None);
    }

    #[test]
    fn display_line() {
        let result = Location::Line(NonZeroUsize::try_from(1).unwrap()).to_string();
        assert_eq!(result, Some(String::from("1:*")));
    }

    #[test]
    fn display_line_col() {
        let result = Location::LineCol {
            line_no: NonZeroUsize::try_from(1).unwrap(),
            col_no: NonZeroUsize::try_from(1).unwrap(),
        }
        .to_string();
        assert_eq!(result, Some(String::from("1:1")));
    }
}
