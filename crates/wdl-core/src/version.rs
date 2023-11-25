//! Workflow Description Language (WDL) grammar versions.

use clap::ValueEnum;
use serde::Deserialize;
use serde::Serialize;

/// A Workflow Description Language (WDL) grammar version.
#[derive(
    Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, ValueEnum,
)]
#[serde(rename_all = "lowercase")]
pub enum Version {
    /// Version 1.x of the WDL specification.
    #[default]
    V1,
}

impl Version {
    /// Gets a short, displayable name for this [`Version`].
    pub fn short_name(&self) -> &'static str {
        match self {
            Version::V1 => "v1",
        }
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Version::V1 => write!(f, "WDL v1.x"),
        }
    }
}
