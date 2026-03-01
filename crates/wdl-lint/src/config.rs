//! Linter config definition.

use std::collections::HashSet;

use serde::Deserialize;
use serde::Serialize;

/// The configuration for lint rules.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// List of keys to ignore in the [`ExpectedRuntimeKeys`] lint.
    ///
    /// ## Example
    ///
    /// ```toml
    /// allowed_runtime_keys = ["foo"]
    /// ```
    ///
    /// [`ExpectedRuntimeKeys`]: crate::rules::ExpectedRuntimeKeysRule
    pub allowed_runtime_keys: HashSet<String>,
    /// List of names to ignore in the [`SnakeCase`] and [`DeclarationName`]
    /// lints.
    ///
    /// ## Example
    ///
    /// ```toml
    /// allowed_names = ["Foo", "counter_int"]
    /// ```
    ///
    /// [`SnakeCase`]: crate::rules::SnakeCaseRule
    /// [`DeclarationName`]: crate::rules::DeclarationNameRule
    pub allowed_names: HashSet<String>,
}
