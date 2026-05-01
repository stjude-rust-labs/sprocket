//! Version-requirement type wrapping a constrained subset of
//! [`semver::VersionReq`].

use std::fmt;
use std::str::FromStr;

use semver::Version;
use semver::VersionReq;
use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

/// An error parsing a [`VersionRequirement`].
#[derive(Debug, Error)]
#[error("version requirement `{0}` is not a valid `semver::VersionReq`")]
pub struct VersionRequirementError(String);

/// A version requirement, parsed by [`semver::VersionReq`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct VersionRequirement(VersionReq);

impl VersionRequirement {
    /// Tests whether a [`Version`] satisfies this requirement.
    pub fn matches(&self, version: &Version) -> bool {
        self.0.matches(version)
    }

    /// Returns a reference to the underlying [`VersionReq`].
    pub fn inner(&self) -> &VersionReq {
        &self.0
    }

    /// Consumes the [`VersionRequirement`] and returns the inner
    /// [`VersionReq`].
    pub fn into_inner(self) -> VersionReq {
        self.0
    }
}

impl fmt::Display for VersionRequirement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl TryFrom<String> for VersionRequirement {
    type Error = VersionRequirementError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(VersionRequirementError(s));
        }
        match VersionReq::parse(trimmed) {
            Ok(v) => Ok(Self(v)),
            Err(_) => Err(VersionRequirementError(s)),
        }
    }
}

impl FromStr for VersionRequirement {
    type Err = VersionRequirementError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s.to_string())
    }
}

impl From<VersionRequirement> for String {
    fn from(req: VersionRequirement) -> Self {
        req.0.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_spec_operators() {
        for s in [
            "^1.2.0",
            "~1.2.0",
            "=1.2.0",
            ">=1.0.0, <2.0.0",
            ">1.0.0",
            "<2.0.0",
            ">=1.0.0",
            "<=2.0.0",
            "*",
            "1.2.0",
        ] {
            assert!(s.parse::<VersionRequirement>().is_ok(), "rejected `{s}`");
        }
    }

    #[test]
    fn rejects_invalid_format() {
        for bad in ["", "   ", "not-a-req"] {
            assert!(
                bad.parse::<VersionRequirement>().is_err(),
                "accepted `{bad}`"
            );
        }
    }

    #[test]
    fn matches_versions_correctly() {
        let req: VersionRequirement = "^1.2.0".parse().unwrap();
        assert!(req.matches(&Version::parse("1.2.0").unwrap()));
        assert!(req.matches(&Version::parse("1.9.99").unwrap()));
        assert!(!req.matches(&Version::parse("2.0.0").unwrap()));
        assert!(!req.matches(&Version::parse("1.1.0").unwrap()));
    }

    #[test]
    fn round_trips_via_serde() {
        let req: VersionRequirement = "^1.2.0".parse().unwrap();
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(json, r#""^1.2.0""#);
        let parsed: VersionRequirement = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, req);
    }
}
