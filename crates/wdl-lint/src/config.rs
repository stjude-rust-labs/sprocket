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
    /// List of keys to ignore in the [`SnakeCase`] lint.
    ///
    /// ## Example
    ///
    /// ```toml
    /// non_snake_case = ["Foo"]
    /// ```
    ///
    /// [`SnakeCase`]: crate::rules::SnakeCaseRule
    pub non_snake_case: HashSet<String>,
    /// List of keys to ignore in the [`DeclarationName`] lint.
    ///
    /// ## Example
    ///
    /// ```toml
    /// allowed_typed_identifiers = ["counter_int"]
    /// ```
    ///
    /// [`DeclarationName`]: crate::rules::DeclarationNameRule
    pub allowed_typed_identifiers: HashSet<String>,
}
