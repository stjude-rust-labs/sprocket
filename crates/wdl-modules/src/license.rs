//! SPDX license expression validation.

use std::fmt;
use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

/// An error parsing a [`LicenseExpression`].
#[derive(Debug, Error)]
pub enum LicenseError {
    /// The expression is empty.
    #[error("license expression cannot be empty")]
    Empty,

    /// The expression is not a valid SPDX license expression.
    #[error("invalid SPDX license expression: {0}")]
    Invalid(String),
}

/// A validated SPDX license expression.
///
/// Validates both the expression syntax and the license identifiers
/// against the SPDX license list (so typos like `MIT-2.0` are rejected
/// even though they would parse syntactically).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct LicenseExpression {
    // NOTE: we store the canonical string rather than the parsed
    // `spdx::Expression` because the upstream `Expression` only derives
    // `Clone`—not `Debug`, `PartialEq`, `Eq`, or `Hash`—and those four
    // derives are required for `LicenseExpression` to compose into
    // `Manifest`, `Tool`, and other types that themselves derive them.
    // Storing the canonical string also makes equality and serialization
    // unambiguous (the SPDX-renormalized form).
    /// The canonical (parsed and re-rendered) form of the expression.
    canonical: String,
}

impl LicenseExpression {
    /// Returns the canonical string form of the expression.
    pub fn as_str(&self) -> &str {
        &self.canonical
    }

    /// Re-parses the canonical string into a fresh [`spdx::Expression`].
    pub fn as_expression(&self) -> spdx::Expression {
        // SAFETY: `canonical` was produced by `spdx::Expression::parse`
        // during construction in `TryFrom<String>`, so it round-trips.
        spdx::Expression::parse(&self.canonical).unwrap()
    }
}

impl TryFrom<String> for LicenseExpression {
    type Error = LicenseError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(LicenseError::Empty);
        }
        let expr =
            spdx::Expression::parse(trimmed).map_err(|e| LicenseError::Invalid(format!("{e}")))?;
        Ok(Self {
            canonical: expr.to_string(),
        })
    }
}

impl fmt::Display for LicenseExpression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical)
    }
}

impl FromStr for LicenseExpression {
    type Err = LicenseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s.to_string())
    }
}

impl From<LicenseExpression> for String {
    fn from(expr: LicenseExpression) -> Self {
        expr.canonical
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_simple_licenses() {
        for s in ["MIT", "Apache-2.0", "BSD-3-Clause", "GPL-3.0-only"] {
            assert!(s.parse::<LicenseExpression>().is_ok(), "rejected `{s}`");
        }
    }

    #[test]
    fn accepts_compound_licenses() {
        for s in [
            "MIT OR Apache-2.0",
            "MIT AND Apache-2.0",
            "(MIT OR Apache-2.0) AND BSD-3-Clause",
            "Apache-2.0 WITH LLVM-exception",
        ] {
            assert!(s.parse::<LicenseExpression>().is_ok(), "rejected `{s}`");
        }
    }

    #[test]
    fn rejects_unknown_id() {
        // `MIT-2.0` is syntactically valid but not in the SPDX list.
        assert!("MIT-2.0".parse::<LicenseExpression>().is_err());
    }

    #[test]
    fn rejects_empty() {
        assert!("".parse::<LicenseExpression>().is_err());
        assert!("   ".parse::<LicenseExpression>().is_err());
    }

    #[test]
    fn round_trips_via_serde() {
        let license: LicenseExpression = "MIT OR Apache-2.0".parse().unwrap();
        let json = serde_json::to_string(&license).unwrap();
        let parsed: LicenseExpression = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, license);
    }
}
