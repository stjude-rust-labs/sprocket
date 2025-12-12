//! Shared helpers for validating numeric runtime fields.

use std::fmt;

use anyhow::Result;
use anyhow::bail;

/// Identifies the source of a numeric task setting used for diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SettingSource {
    /// The source is a requirement.
    Requirement,
    /// The source is a hint.
    Hint,
}

impl fmt::Display for SettingSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Requirement => write!(f, "requirement"),
            Self::Hint => write!(f, "hint"),
        }
    }
}

/// Ensures a numeric value is not negative.
pub(crate) fn ensure_non_negative_i64(source: SettingSource, key: &str, value: i64) -> Result<i64> {
    if value < 0 {
        bail!("task {source} `{key}` cannot be less than zero (got {value})",);
    }

    Ok(value)
}

/// Formats a shared error message for invalid numeric literals.
pub(crate) fn invalid_numeric_value_message(source: SettingSource, key: &str, raw: &str) -> String {
    format!("task specifies an invalid `{key}` {source} `{raw}`")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_non_negative_allows_zero() {
        assert_eq!(
            ensure_non_negative_i64(SettingSource::Requirement, "cpu", 0).unwrap(),
            0
        );
    }

    #[test]
    fn ensure_non_negative_rejects_negatives() {
        let err = ensure_non_negative_i64(SettingSource::Hint, "preemptible", -2).unwrap_err();
        assert!(
            err.to_string()
                .contains("task hint `preemptible` cannot be less than zero")
        );
    }

    #[test]
    fn invalid_message_mentions_kind() {
        let message = invalid_numeric_value_message(SettingSource::Requirement, "memory", "-1 GiB");
        assert_eq!(
            message,
            "task specifies an invalid `memory` requirement `-1 GiB`"
        );
    }
}
