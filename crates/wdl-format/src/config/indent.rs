//! Indentation within formatting configuration.

use std::fmt;

use schemars::JsonSchema;
use toml_spanner::Arena;
use toml_spanner::Context;
use toml_spanner::Failed;
use toml_spanner::FromToml;
use toml_spanner::Item;
use toml_spanner::ToToml;
use toml_spanner::ToTomlError;

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
#[derive(Clone, Copy, Debug, PartialEq, Eq, JsonSchema)]
#[schemars(rename_all = "lowercase")]
pub enum Indent {
    /// Tabs.
    Tabs,
    /// Spaces.
    #[schemars(untagged)]
    Spaces(#[schemars(range(max = "MAX_SPACE_INDENT"))] usize),
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

impl fmt::Display for Indent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for _ in 0..self.num() {
            self.character().fmt(f)?;
        }

        Ok(())
    }
}

impl<'de> FromToml<'de> for Indent {
    fn from_toml(ctx: &mut Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
        if let Some("tabs") = item.as_str() {
            return Ok(Self::Tabs);
        }

        if let Some(n) = item.as_u64().and_then(|n| usize::try_from(n).ok())
            && n <= MAX_SPACE_INDENT
        {
            return Ok(Self::Spaces(n));
        }

        Err(ctx.report_custom_error(
            format!(
                "expected an integer less than or equal to {MAX_SPACE_INDENT} or `tabs` for \
                 indentation value"
            ),
            item,
        ))
    }
}

impl ToToml for Indent {
    fn to_toml<'a>(&'a self, _: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        match self {
            Self::Tabs => Ok(Item::string("tabs")),
            Self::Spaces(n) => Ok(i64::try_from(*n)
                .map_err(|e| ToTomlError {
                    message: format!("invalid number of spaces: {e}").into(),
                })?
                .into()),
        }
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn serialization() {
        let map: HashMap<&str, Indent> = HashMap::from_iter([("value", Indent::Tabs)]);
        assert_eq!(toml_spanner::to_string(&map).unwrap(), "value = \"tabs\"\n");

        let map: HashMap<&str, Indent> = HashMap::from_iter([("value", Indent::Spaces(10))]);
        assert_eq!(toml_spanner::to_string(&map).unwrap(), "value = 10\n");
    }

    #[test]
    fn deserialization() {
        let map: HashMap<String, Indent> = toml_spanner::from_str("value = 'tabs'").unwrap();
        assert_eq!(map["value"], Indent::Tabs);

        let map: HashMap<String, Indent> = toml_spanner::from_str("value = 10").unwrap();
        assert_eq!(map["value"], Indent::Spaces(10));

        let map: HashMap<String, Indent> = toml_spanner::from_str("value = 0").unwrap();
        assert_eq!(map["value"], Indent::Spaces(0));

        let expected_error = format!(
            "expected an integer less than or equal to {MAX_SPACE_INDENT} or `tabs` for \
             indentation value at `value`"
        );

        let error =
            toml_spanner::from_str::<HashMap<String, Indent>>("value = 'wrong'").unwrap_err();
        assert_eq!(error.to_string(), expected_error);

        let error = toml_spanner::from_str::<HashMap<String, Indent>>("value = -10").unwrap_err();
        assert_eq!(error.to_string(), expected_error);

        let error =
            toml_spanner::from_str::<HashMap<String, Indent>>("value = 100000").unwrap_err();
        assert_eq!(error.to_string(), expected_error);
    }
}
