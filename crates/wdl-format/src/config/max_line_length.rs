//! Configuration for max line length formatting.

use schemars::JsonSchema;
use toml_spanner::Arena;
use toml_spanner::Context;
use toml_spanner::Failed;
use toml_spanner::FromToml;
use toml_spanner::Item;
use toml_spanner::ToToml;
use toml_spanner::ToTomlError;

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
#[derive(JsonSchema)]
#[schemars(inline)]
#[expect(dead_code, reason = "Only used for schema generation.")]
enum MaxLineLengthSchema {
    /// No maximum.
    #[schemars(rename = "none")]
    None,
    /// Maximum line length in characters.
    #[schemars(untagged)]
    Value(#[schemars(range(min = MIN_MAX_LINE_LENGTH, max = MAX_MAX_LINE_LENGTH))] usize),
}

/// The maximum line length.
#[derive(Clone, Copy, Debug, Eq, PartialEq, JsonSchema)]
#[schemars(with = "MaxLineLengthSchema")]
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

impl<'de> FromToml<'de> for MaxLineLength {
    fn from_toml(ctx: &mut Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
        if let Some(SENTINEL) = item.as_str() {
            return Ok(Self(None));
        }

        if let Some(n) = item.as_u64().and_then(|n| usize::try_from(n).ok())
            && (MIN_MAX_LINE_LENGTH..=MAX_MAX_LINE_LENGTH).contains(&n)
        {
            return Ok(Self(Some(n)));
        }

        Err(ctx.report_custom_error(
            format!(
                "expected a positive integer between {MIN_MAX_LINE_LENGTH} and \
                 {MAX_MAX_LINE_LENGTH} or `{SENTINEL}` for max line length value"
            ),
            item,
        ))
    }
}

impl ToToml for MaxLineLength {
    fn to_toml<'a>(&'a self, _: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        match &self.0 {
            Some(n) => Ok(i64::try_from(*n)
                .map_err(|e| ToTomlError {
                    message: format!("invalid max line length: {e}").into(),
                })?
                .into()),
            None => Ok(Item::string(SENTINEL)),
        }
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn serialization() {
        let map: HashMap<&str, MaxLineLength> =
            HashMap::from_iter([("value", MaxLineLength(None))]);
        assert_eq!(
            toml_spanner::to_string(&map).unwrap(),
            format!("value = \"{SENTINEL}\"\n")
        );

        let map: HashMap<&str, MaxLineLength> =
            HashMap::from_iter([("value", MaxLineLength(Some(123)))]);
        assert_eq!(toml_spanner::to_string(&map).unwrap(), "value = 123\n");
    }

    #[test]
    fn deserialization() {
        let map: HashMap<String, MaxLineLength> =
            toml_spanner::from_str(&format!("value = '{SENTINEL}'")).unwrap();
        assert_eq!(map["value"], MaxLineLength(None));

        let map: HashMap<String, MaxLineLength> = toml_spanner::from_str("value = 80").unwrap();
        assert_eq!(map["value"], MaxLineLength(Some(80)));

        let expected_error = format!(
            "expected a positive integer between {MIN_MAX_LINE_LENGTH} and {MAX_MAX_LINE_LENGTH} \
             or `{SENTINEL}` for max line length value at `value`"
        );

        let error = toml_spanner::from_str::<HashMap<String, MaxLineLength>>("value = 'wrong'")
            .unwrap_err();
        assert_eq!(error.to_string(), expected_error);

        let error =
            toml_spanner::from_str::<HashMap<String, MaxLineLength>>("value = 1234").unwrap_err();
        assert_eq!(error.to_string(), expected_error);

        let error =
            toml_spanner::from_str::<HashMap<String, MaxLineLength>>("value = -10").unwrap_err();
        assert_eq!(error.to_string(), expected_error);
    }
}
