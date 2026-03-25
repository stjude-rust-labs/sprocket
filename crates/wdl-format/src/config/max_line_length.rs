//! Configuration for max line length formatting.

use serde::Deserialize;
use serde::Serialize;

/// Error while creating a max line length configuration.
#[derive(thiserror::Error, Debug)]
pub enum MaxLineLengthError {
    /// Supplied number outside allowed range.
    #[error(
        "`{0}` is outside the allowed range for the max line length ({min}-{max})",
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
/// The max line length sentinel value meaning "no maximum".
const SENTINEL: &str = "none";

/// The maximum line length.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MaxLineLength(Option<usize>);

impl MaxLineLength {
    /// Attempts to create a new `MaxLineLength` with the provided value.
    pub fn try_new(value: Option<usize>) -> Result<Self, MaxLineLengthError> {
        match value {
            None => Ok(Self(None)),
            Some(value) if (MIN_MAX_LINE_LENGTH..=MAX_MAX_LINE_LENGTH).contains(&value) => {
                Ok(Self(Some(value)))
            }
            Some(value) => Err(MaxLineLengthError::OutsideAllowedRange(value)),
        }
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

impl Serialize for MaxLineLength {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            MaxLineLength(None) => SENTINEL.serialize(serializer),
            MaxLineLength(Some(n)) => n.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for MaxLineLength {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Value {
            Num(usize),
            Str(String),
            Null,
        }

        match Value::deserialize(deserializer)? {
            Value::Num(n) => MaxLineLength::try_new(Some(n)).map_err(serde::de::Error::custom),
            Value::Str(s) if s == SENTINEL => Ok(MaxLineLength(None)),
            Value::Str(s) => Err(serde::de::Error::custom(format!(
                "expected a number or \"{SENTINEL}\", got \"{s}\""
            ))),
            Value::Null => Ok(MaxLineLength(None)),
        }
    }
}
