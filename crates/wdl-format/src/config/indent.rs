//! Indentation within formatting configuration.

use serde::Deserialize;
use serde::Serialize;

use crate::SPACE;
use crate::TAB;

/// Error while creating indentation configuration.
#[derive(thiserror::Error, Debug)]
pub enum IndentError {
    /// Invalid space options
    #[error("indentation with spaces must have a number of spaces")]
    InvalidSpaceConfiguration,
    /// Invalid tab options
    #[error("indentation with tabs cannot have a number of spaces")]
    InvalidTabConfiguration,
    /// Too many spaces
    #[error("`{0}` is more than the maximum allowed number of spaces ({max})", max = MAX_SPACE_INDENT)]
    TooManySpaces(usize),
}

/// The default number of spaces to represent one indentation level.
const DEFAULT_SPACE_INDENT: usize = 4;
/// The default indentation.
pub const DEFAULT_INDENT: Indent = Indent::Spaces(DEFAULT_SPACE_INDENT);
/// The maximum number of spaces to represent one indentation level.
pub const MAX_SPACE_INDENT: usize = 16;

/// An indentation level.
#[derive(Clone, Copy, Debug)]
pub enum Indent {
    /// Tabs.
    Tabs,
    /// Spaces.
    Spaces(usize),
}

impl Default for Indent {
    fn default() -> Self {
        DEFAULT_INDENT
    }
}

impl Indent {
    /// Attempts to create a new indentation level configuration.
    pub fn try_new(tab: bool, num_spaces: Option<usize>) -> Result<Self, IndentError> {
        match (tab, num_spaces) {
            (true, None) => Ok(Indent::Tabs),
            (true, Some(_)) => Err(IndentError::InvalidTabConfiguration),
            (false, Some(n)) => {
                if n > MAX_SPACE_INDENT {
                    Err(IndentError::TooManySpaces(n))
                } else {
                    Ok(Indent::Spaces(n))
                }
            }
            (false, None) => Err(IndentError::InvalidSpaceConfiguration),
        }
    }

    /// Gets the number of characters to indent.
    pub fn num(&self) -> usize {
        match self {
            Indent::Tabs => 1,
            Indent::Spaces(n) => *n,
        }
    }

    /// Gets the character used for indentation.
    pub fn character(&self) -> &str {
        match self {
            Indent::Tabs => TAB,
            Indent::Spaces(_) => SPACE,
        }
    }

    /// Gets the string representation of the indentation.
    pub fn string(&self) -> String {
        match self {
            Indent::Tabs => self.character().to_string(),
            Indent::Spaces(n) => self.character().repeat(*n),
        }
    }
}

impl Serialize for Indent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Indent::Tabs => "tabs".serialize(serializer),
            Indent::Spaces(n) => n.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Indent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Value {
            Num(usize),
            Str(String),
        }

        match Value::deserialize(deserializer)? {
            Value::Num(n) => Indent::try_new(false, Some(n)).map_err(serde::de::Error::custom),
            Value::Str(s) if s == "tabs" => Ok(Indent::Tabs),
            Value::Str(s) => Err(serde::de::Error::custom(format!(
                "expected a number or \"tabs\", got \"{s}\""
            ))),
        }
    }
}
