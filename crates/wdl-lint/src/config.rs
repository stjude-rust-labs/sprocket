//! Linter config definition.

use std::collections::HashSet;

use serde::Deserialize;
use serde::Serialize;

/// Define the lint rule config and doc generation utilities.
macro_rules! define_lint_rule_config {
    (
        $(#[$meta:meta])*
        pub struct $name:ident {
            $(
                $(#[doc = $doc:literal])+
                #[lints($($lints:ident),+ $(,)?)]
                $field:ident: $ty:ty = $default:expr,
            )+
        }
    ) => {
        /// The configuration for lint rules.
        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
        pub struct $name {
            $(
                $(#[doc = $doc])+
                pub $field: $ty,
            )+
        }

        impl Default for Config {
            fn default() -> Self {
                Self {
                    $(
                        $field: $default,
                    )+
                }
            }
        }

        impl $name {
            /// **(NOT A PUBLIC API)** Get the metadata for all config fields
            #[doc(hidden)]
            pub fn fields() -> &'static [ConfigField] {
                &[
                    $(
                        ConfigField {
                            name: stringify!($name),
                            applicable_lints: &[$(stringify!($lints)),+,]
                        }
                    ),+
                ]
            }
        }
    }
}

/// **(NOT A PUBLIC API)** A field in the `wdl-lint` [`Config`].
#[doc(hidden)]
#[derive(Debug)]
pub struct ConfigField {
    /// The name of the field.
    pub name: &'static str,
    /// All lints that this field applies to.
    pub applicable_lints: &'static [&'static str],
}

define_lint_rule_config! {
    /// The configuration for lint rules.
    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
        #[lints(ExpectedRuntimeKeysRule)]
        allowed_runtime_keys: HashSet<String> = HashSet::default(),
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
        #[lints(SnakeCase, DeclarationName)]
        allowed_names: HashSet<String> = HashSet::default(),
    }
}
