//! Linter config definition.

use schemars::JsonSchema;
use toml_spanner::Toml;

use crate::rules::BashSetOption;

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
        $(#[$meta])*
        #[toml(Toml)]
        pub struct $name {
            $(
                $(#[doc = $doc])+
                #[toml(default)]
                #[schemars(default)]
                pub $field: $ty,
            )+
        }

        impl Default for $name {
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
            pub fn fields() -> Vec<ConfigField> {
                vec![
                    $(
                        ConfigField {
                            name: stringify!($field),
                            description: concat!($($doc, '\n',)*).trim(),
                            default: {
                                let default: $ty = $default;
                                serde_json::to_string(&default).unwrap()
                            },
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
    /// The description of the config field.
    pub description: &'static str,
    /// The default value of the field as a TOML string.
    pub default: String,
    /// All lints that this field applies to.
    pub applicable_lints: &'static [&'static str],
}

define_lint_rule_config! {
    /// The configuration for lint rules.
    #[derive(Clone, Debug, PartialEq, Eq, Toml, JsonSchema)]
    #[schemars(rename = "WdlLintConfig")]
    pub struct Config {
        /// List of keys to ignore in the [`ExpectedRuntimeKeys`] lint.
        ///
        /// ##### Example
        ///
        /// ```toml
        /// allowed_runtime_keys = ["foo"]
        /// ```
        ///
        /// [`ExpectedRuntimeKeys`]: crate::rules::ExpectedRuntimeKeysRule
        #[lints(ExpectedRuntimeKeys)]
        allowed_runtime_keys: Vec<String> = Vec::default(),
        /// List of names to ignore in the [`SnakeCase`] and [`DeclarationName`]
        /// lints.
        ///
        /// ##### Example
        ///
        /// ```toml
        /// allowed_names = ["Foo", "counter_int"]
        /// ```
        ///
        /// [`SnakeCase`]: crate::rules::SnakeCaseRule
        /// [`DeclarationName`]: crate::rules::DeclarationNameRule
        #[lints(SnakeCase, DeclarationName)]
        allowed_names: Vec<String> = Vec::default(),
        /// List of options to enforce in the bash `set` builtin for every
        /// `command` section.
        ///
        /// ##### Example
        ///
        /// ```toml
        /// bash_set_options = ["errexit", "nounset", "pipefail"]
        /// ```
        #[lints(BashSetSyntax)]
        bash_set_options: Vec<BashSetOption> = vec![BashSetOption::ErrExit, BashSetOption::NoUnset, BashSetOption::Pipefail],
    }
}
