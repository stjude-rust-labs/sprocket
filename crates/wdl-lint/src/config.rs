//! Linter config definition.

use std::collections::HashSet;

use serde::Deserialize;
use serde::Serialize;

/// The configuration for lint rules.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// List of keys to ignore in the [`ExpectedRuntimeKeys`] lint.
    ///
    /// ## Example
    ///
    /// ```toml
    /// allowed_runtime_keys = ["foo"]
    /// ```
    ///
    /// [`ExpectedRuntimeKeys`]: crate::rules::ExpectedRuntimeKeysRule.
    pub allowed_runtime_keys: HashSet<String>,
}
