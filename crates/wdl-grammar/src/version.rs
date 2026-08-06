//! Representation for version definitions.

use std::mem;
use std::str::FromStr;

use strum::IntoEnumIterator;

/// Represents a supported V1 WDL version.
// NOTE: it is expected that this enumeration is in increasing order of 1.x versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, strum::EnumIter)]
#[non_exhaustive]
#[cfg_attr(
    feature = "unstable-python",
    pyo3::pyclass(
        module = "sprocket_bio.grammar.version",
        frozen,
        from_py_object,
        rename_all = "SCREAMING_SNAKE_CASE",
        eq,
        ord,
        str
    )
)]
pub enum V1 {
    /// The document version is 1.0.
    Zero,
    /// The document version is 1.1.
    One,
    /// The document version is 1.2.
    Two,
    /// The document version is 1.3.
    Three,
    /// The document version is 1.4.
    Four,
}

impl std::fmt::Display for V1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            V1::Zero => write!(f, "1.0"),
            V1::One => write!(f, "1.1"),
            V1::Two => write!(f, "1.2"),
            V1::Three => write!(f, "1.3"),
            V1::Four => write!(f, "1.4"),
        }
    }
}

impl FromStr for V1 {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "1.0" => Ok(Self::Zero),
            "1.1" => Ok(Self::One),
            "1.2" => Ok(Self::Two),
            "1.3" => Ok(Self::Three),
            "1.4" => Ok(Self::Four),
            _ => Err(format!("unsupported version `{s}`")),
        }
    }
}

/// Represents a supported WDL version.
///
/// The `Default` implementation of this type returns the most recent
/// fully-supported ratified version of WDL.
// NOTE: it is expected that this enumeration is in increasing order of WDL versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
#[cfg_attr(
    feature = "unstable-python",
    pyo3::pyclass(
        module = "sprocket_bio.grammar.version",
        frozen,
        from_py_object,
        rename_all = "SCREAMING_SNAKE_CASE",
        eq,
        ord,
        str
    )
)]
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
        // Check only that the discriminants are equal, ignoring the value contained by
        // each.
        mem::discriminant(&self) == mem::discriminant(&other)
    }

    /// Returns an iterator over all supported WDL versions.
    pub fn all() -> impl Iterator<Item = Self> {
        V1::iter().map(Self::V1)
    }
}

impl Default for SupportedVersion {
    fn default() -> Self {
        Self::V1(V1::Three)
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
        if let Ok(v) = V1::from_str(s) {
            return Ok(Self::V1(v));
        }

        Err(format!("unsupported version `{s}`"))
    }
}

/// Python-specific APIs.
#[cfg(feature = "unstable-python")]
mod python {
    use pyo3::prelude::*;

    use super::*;

    #[pymethods]
    impl SupportedVersion {
        /// Returns `true` if the other version has the same major version as
        /// this one.
        ///
        /// ```python
        /// >>> from sprocket_bio.grammar.version import SupportedVersion, V1
        /// >>> SupportedVersion.V1(V1.ZERO).has_same_major_version(SupportedVersion.V1(V1.TWO))
        /// True
        /// ```
        #[pyo3(name = "has_same_major_version")]
        fn py_has_same_major_version(&self, other: Bound<'_, SupportedVersion>) -> bool {
            self.has_same_major_version(*other.get())
        }

        /// Returns a printable representation of this object.
        fn __repr__(&self) -> String {
            match self {
                Self::V1(v1) => format!("SupportedVersion.V1({})", v1.__pyo3__repr__()),
            }
        }
    }
}
