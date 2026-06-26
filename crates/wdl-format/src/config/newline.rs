//! Newline style within formatting configuration.

use std::fmt;
use std::str::FromStr;

use schemars::JsonSchema;
use thiserror::Error;

/// Unix-style newline.
const UNIX_NEWLINE: &str = "\n";

/// Windows-style newline.
const WINDOWS_NEWLINE: &str = "\r\n";

/// The newline style to use when formatting.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, JsonSchema)]
pub enum NewlineStyle {
    /// Use the native newline style of the platform.
    #[default]
    Auto,
    /// Use Unix-style newlines (`\n`).
    Unix,
    /// Use Windows-style newlines (`\r\n`).
    Windows,
}

impl fmt::Display for NewlineStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auto => f.write_str("auto"),
            Self::Unix => f.write_str("unix"),
            Self::Windows => f.write_str("windows"),
        }
    }
}

/// An error returned when parsing an invalid [`NewlineStyle`] string.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("invalid newline style `{0}`; expected one of: `auto`, `unix`, `windows`")]
pub struct ParseNewlineStyleError(String);

impl FromStr for NewlineStyle {
    type Err = ParseNewlineStyleError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "unix" => Ok(Self::Unix),
            "windows" => Ok(Self::Windows),
            _ => Err(ParseNewlineStyleError(s.to_string())),
        }
    }
}

impl NewlineStyle {
    /// Gets the newline string for this style.
    pub fn as_str(&self) -> &str {
        match self {
            NewlineStyle::Auto => {
                if cfg!(windows) {
                    WINDOWS_NEWLINE
                } else {
                    UNIX_NEWLINE
                }
            }
            NewlineStyle::Unix => UNIX_NEWLINE,
            NewlineStyle::Windows => WINDOWS_NEWLINE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unix_newline() {
        assert_eq!(NewlineStyle::Unix.as_str(), "\n");
    }

    #[test]
    fn test_windows_newline() {
        assert_eq!(NewlineStyle::Windows.as_str(), "\r\n");
    }

    #[test]
    fn test_auto_newline() {
        let newline = NewlineStyle::Auto.as_str();
        assert!(newline == "\n" || newline == "\r\n");
    }

    #[test]
    fn test_default_is_auto() {
        assert!(matches!(NewlineStyle::default(), NewlineStyle::Auto));
    }

    #[test]
    fn from_str_accepts_valid_values() {
        assert_eq!("auto".parse::<NewlineStyle>().unwrap(), NewlineStyle::Auto);
        assert_eq!("unix".parse::<NewlineStyle>().unwrap(), NewlineStyle::Unix);
        assert_eq!(
            "windows".parse::<NewlineStyle>().unwrap(),
            NewlineStyle::Windows
        );
        assert_eq!("AUTO".parse::<NewlineStyle>().unwrap(), NewlineStyle::Auto);
    }

    #[test]
    fn from_str_rejects_invalid_value() {
        let err = "bad".parse::<NewlineStyle>().unwrap_err();
        assert_eq!(
            err.to_string(),
            "invalid newline style `bad`; expected one of: `auto`, `unix`, `windows`"
        );
    }

    #[test]
    fn display_round_trips_through_from_str() {
        for style in [
            NewlineStyle::Auto,
            NewlineStyle::Unix,
            NewlineStyle::Windows,
        ] {
            assert_eq!(style.to_string().parse::<NewlineStyle>().unwrap(), style);
        }
    }
}
