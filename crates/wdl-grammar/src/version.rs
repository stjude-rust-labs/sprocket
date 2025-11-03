//! Representation for version definitions.

use std::str::FromStr;

use strum::IntoEnumIterator;

/// Represents a supported V1 WDL version.
// NOTE: it is expected that this enumeration is in increasing order of 1.x versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, strum::EnumIter)]
#[non_exhaustive]
pub enum V1 {
    /// The document version is 1.0.
    Zero,
    /// The document version is 1.1.
    One,
    /// The document version is 1.2.
    Two,
    /// The document version is 1.3.
    Three,
}

impl std::fmt::Display for V1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            V1::Zero => write!(f, "1.0"),
            V1::One => write!(f, "1.1"),
            V1::Two => write!(f, "1.2"),
            V1::Three => write!(f, "1.3"),
        }
    }
}

/// Represents a supported WDL version.
///
/// The `Default` implementation of this type returns the most recent
/// fully-supported ratified version of WDL.
// NOTE: it is expected that this enumeration is in increasing order of WDL versions.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    serde_with::DeserializeFromStr,
    serde_with::SerializeDisplay,
)]
#[non_exhaustive]
pub enum SupportedVersion {
    /// The document version is 1.x.
    V1(V1),
}

impl SupportedVersion {
    /// Returns `true` if the other version has the same major version as this
    /// one.
    ///
    /// ```
    /// # use wdl_grammar::SupportedVersion;
    /// # use wdl_grammar::version::V1;
    /// assert!(SupportedVersion::V1(V1::Zero).has_same_major_version(SupportedVersion::V1(V1::Two)));
    /// ```
    pub fn has_same_major_version(self, other: SupportedVersion) -> bool {
        match (self, other) {
            (SupportedVersion::V1(_), SupportedVersion::V1(_)) => true,
        }
    }

    /// Returns an iterator over all supported WDL versions.
    pub fn all() -> impl Iterator<Item = Self> {
        V1::iter().map(Self::V1)
    }
}

impl Default for SupportedVersion {
    fn default() -> Self {
        Self::V1(V1::Two)
    }
}

impl std::fmt::Display for SupportedVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SupportedVersion::V1(version) => write!(f, "{version}"),
        }
    }
}

impl FromStr for SupportedVersion {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "1.0" => Ok(Self::V1(V1::Zero)),
            "1.1" => Ok(Self::V1(V1::One)),
            "1.2" => Ok(Self::V1(V1::Two)),
            "1.3" => Ok(Self::V1(V1::Three)),
            _ => Err(s.to_string()),
        }
    }
}
