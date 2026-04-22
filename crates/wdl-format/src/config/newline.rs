//! Newline style within formatting configuration.

use std::str::FromStr;

/// Unix-style newline.
const UNIX_NEWLINE: &str = "\n";

/// Windows-style newline.
const WINDOWS_NEWLINE: &str = "\r\n";

/// The newline style to use when formatting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NewlineStyle {
    /// Use the native newline style of the platform.
    #[default]
    Auto,
    /// Use Unix-style newlines (`\n`).
    Unix,
    /// Use Windows-style newlines (`\r\n`).
    Windows,
}

impl FromStr for NewlineStyle {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match &*s.to_ascii_lowercase() {
            "auto" => Ok(Self::Auto),
            "unix" => Ok(Self::Unix),
            "windows" => Ok(Self::Windows),
            _ => Err(()),
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
}
