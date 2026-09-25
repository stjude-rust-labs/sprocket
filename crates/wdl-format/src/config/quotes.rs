//! Quote style configuration.

use std::fmt;
use std::str::FromStr;

use schemars::JsonSchema;
use thiserror::Error;

/// Double quote literal.
const DOUBLE_QUOTE: &str = "\"";
/// Single quote literal.
const SINGLE_QUOTE: &str = "'";

/// The quote style to use when formatting.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, JsonSchema)]
#[schemars(rename_all = "lowercase")]
pub enum QuoteStyle {
    /// Use double quotes for all literal strings.
    Double,
    /// Use single quotes for all literal strings.
    Single,
    /// Preserve the quote style found in the input.
    #[default]
    Preserve,
}

impl fmt::Display for QuoteStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Double => f.write_str("double"),
            Self::Single => f.write_str("single"),
            Self::Preserve => f.write_str("preserve"),
        }
    }
}

/// An error returned when parsing an invalid [`QuoteStyle`] string.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("invalid quote style `{0}`; expected one of: `double`, `single`, `preserve`")]
pub struct ParseQuoteStyleError(String);

impl FromStr for QuoteStyle {
    type Err = ParseQuoteStyleError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "double" => Ok(Self::Double),
            "single" => Ok(Self::Single),
            "preserve" => Ok(Self::Preserve),
            _ => Err(ParseQuoteStyleError(s.to_string())),
        }
    }
}

impl QuoteStyle {
    /// Gets the quote string for this style. Returns `None` if style is
    /// [`QuoteStyle::Preserve`].
    pub fn as_str(&self) -> Option<&str> {
        match self {
            QuoteStyle::Preserve => None,
            QuoteStyle::Double => Some(DOUBLE_QUOTE),
            QuoteStyle::Single => Some(SINGLE_QUOTE),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_double_quote() {
        assert_eq!(QuoteStyle::Double.as_str(), Some("\""));
    }

    #[test]
    fn test_single_quote() {
        assert_eq!(QuoteStyle::Single.as_str(), Some("'"));
    }

    #[test]
    fn test_preserve_quote() {
        assert_eq!(QuoteStyle::Preserve.as_str(), None);
    }

    #[test]
    fn test_default_is_preserve() {
        assert!(matches!(QuoteStyle::default(), QuoteStyle::Preserve));
    }

    #[test]
    fn from_str_accepts_valid_values() {
        assert_eq!(
            "preserve".parse::<QuoteStyle>().unwrap(),
            QuoteStyle::Preserve
        );
        assert_eq!("double".parse::<QuoteStyle>().unwrap(), QuoteStyle::Double);
        assert_eq!("single".parse::<QuoteStyle>().unwrap(), QuoteStyle::Single);
    }

    #[test]
    fn from_str_rejects_invalid_value() {
        let err = "bad".parse::<QuoteStyle>().unwrap_err();
        assert_eq!(
            err.to_string(),
            "invalid quote style `bad`; expected one of: `double`, `single`, `preserve`"
        );
    }

    #[test]
    fn display_round_trips_through_from_str() {
        for style in [QuoteStyle::Preserve, QuoteStyle::Double, QuoteStyle::Single] {
            assert_eq!(style.to_string().parse::<QuoteStyle>().unwrap(), style);
        }
    }
}
