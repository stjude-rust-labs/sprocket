use anyhow::Result;
use anyhow::bail;

/// Identifies the source of a numeric task setting used for diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResourceKind {
    /// The source is a requirement.
    Requirement,
    /// Hint values that tweak backend behavior.
    Hint,
}

impl ResourceKind {
    /// Provides a short noun that reads well inside error messages.
    const fn noun(self) -> &'static str {
        match self {
            Self::Requirement => "requirement",
            Self::Hint => "hint",
        }
    }
}

/// Ensures a numeric value is not negative.
pub(crate) fn ensure_non_negative_i64(kind: ResourceKind, key: &str, value: i64) -> Result<i64> {
    if value < 0 {
        bail!(
            "task {kind} `{key}` cannot be less than zero (got {value})",
            kind = kind.noun(),
        );
    }

    Ok(value)
}

/// Formats a shared error message for invalid numeric literals.
pub(crate) fn invalid_numeric_value_message(kind: ResourceKind, key: &str, raw: &str) -> String {
    format!(
        "task specifies an invalid `{key}` {kind} `{raw}`",
        kind = kind.noun(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_non_negative_allows_zero() {
        assert_eq!(
            ensure_non_negative_i64(ResourceKind::Requirement, "cpu", 0).unwrap(),
            0
        );
    }

    #[test]
    fn ensure_non_negative_rejects_negatives() {
        let err = ensure_non_negative_i64(ResourceKind::Hint, "preemptible", -2).unwrap_err();
        assert!(
            err.to_string()
                .contains("task hint `preemptible` cannot be less than zero")
        );
    }

    #[test]
    fn invalid_message_mentions_kind() {
        let message = invalid_numeric_value_message(ResourceKind::Requirement, "memory", "-1 GiB");
        assert_eq!(
            message,
            "task specifies an invalid `memory` requirement `-1 GiB`"
        );
    }
}
