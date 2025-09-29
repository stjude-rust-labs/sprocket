//! Configuration for max line length formatting.

use thiserror::Error;

/// Error while creating a max line length configuration.
#[derive(Error, Debug)]
pub enum MaxLineLengthError {
    /// Suppplied value outside allowed range.
    #[error(
        "`{0}` is outside the allowed range for the max line length: `{min}-{max}`",
        min = MIN_MAX_LINE_LENGTH,
        max = MAX_MAX_LINE_LENGTH
    )]
    OutsideAllowedRange(usize),
}

/// The default maximum line length.
pub const DEFAULT_MAX_LINE_LENGTH: usize = 90;
/// The minimum maximum line length.
pub const MIN_MAX_LINE_LENGTH: usize = 60;
/// The maximum maximum line length.
pub const MAX_MAX_LINE_LENGTH: usize = 240;

/// The maximum line length.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MaxLineLength(Option<usize>);

impl MaxLineLength {
    /// Attempts to create a new `MaxLineLength` with the provided value.
    ///
    /// A value of `0` indicates no maximum.
    pub fn try_new(value: usize) -> Result<Self, MaxLineLengthError> {
        let val = match value {
            0 => Self(None),
            MIN_MAX_LINE_LENGTH..=MAX_MAX_LINE_LENGTH => Self(Some(value)),
            _ => {
                return Err(MaxLineLengthError::OutsideAllowedRange(value));
            }
        };
        Ok(val)
    }

    /// Gets the maximum line length. A value of `None` indicates no maximum.
    pub fn get(&self) -> Option<usize> {
        self.0
    }
}

impl Default for MaxLineLength {
    fn default() -> Self {
        Self(Some(DEFAULT_MAX_LINE_LENGTH))
    }
}
