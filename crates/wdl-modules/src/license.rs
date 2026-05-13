//! SPDX license expression validation.

use std::fmt;
use std::hash::Hash;
use std::hash::Hasher;
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
#[derive(Clone, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct LicenseExpression(spdx::Expression);

impl LicenseExpression {
    /// Returns a reference to the inner [`spdx::Expression`].
    pub fn as_expression(&self) -> &spdx::Expression {
        &self.0
    }

    /// Returns the canonical string form of the expression.
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

impl fmt::Debug for LicenseExpression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("LicenseExpression")
            .field(&self.as_str())
            .finish()
    }
}

impl fmt::Display for LicenseExpression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialEq for LicenseExpression {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for LicenseExpression {}

impl Hash for LicenseExpression {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
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
        Ok(Self(expr))
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
        expr.as_str().to_string()
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
