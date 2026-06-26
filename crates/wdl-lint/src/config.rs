//! Linter configuration.
//!
//! Configuration is expressed as a table of per-rule settings keyed by rule ID.
//! Each rule may set an optional `severity` override and any parameters that
//! apply to it. See [`RuleConfig`] for the available parameters and
//! [`RuleSeverity`] for the severity values.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use toml_spanner::Arena;
use toml_spanner::Context;
use toml_spanner::Failed;
use toml_spanner::FromToml;
use toml_spanner::Item;
use toml_spanner::ToToml;
use toml_spanner::ToTomlError;
use toml_spanner::Toml;
use wdl_ast::Severity;

/// An overridden severity for a rule.
///
/// This is distinct from the absence of an override, which leaves a rule
/// emitting at its built-in default severity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleSeverity {
    /// Disable the rule entirely.
    Off,
    /// Emit the rule's diagnostics at note severity.
    Note,
    /// Emit the rule's diagnostics at warning severity.
    Warning,
    /// Emit the rule's diagnostics at error severity.
    Error,
}

impl RuleSeverity {
    /// Converts the override to a concrete [`Severity`].
    ///
    /// Returns `None` when the rule is disabled.
    pub fn as_severity(self) -> Option<Severity> {
        match self {
            RuleSeverity::Off => None,
            RuleSeverity::Note => Some(Severity::Note),
            RuleSeverity::Warning => Some(Severity::Warning),
            RuleSeverity::Error => Some(Severity::Error),
        }
    }
}

impl FromStr for RuleSeverity {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "off" => Ok(RuleSeverity::Off),
            "note" => Ok(RuleSeverity::Note),
            "warning" => Ok(RuleSeverity::Warning),
            "error" => Ok(RuleSeverity::Error),
            _ => Err(format!(
                "expected one of `off`, `note`, `warning`, or `error`, found `{s}`"
            )),
        }
    }
}

impl fmt::Display for RuleSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            RuleSeverity::Off => "off",
            RuleSeverity::Note => "note",
            RuleSeverity::Warning => "warning",
            RuleSeverity::Error => "error",
        };
        f.write_str(s)
    }
}

impl<'de> FromToml<'de> for RuleSeverity {
    fn from_toml(ctx: &mut Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
        let Some(s) = item.as_str() else {
            return Err(ctx.report_expected_but_found(&"a string", item));
        };
        s.parse()
            .map_err(|e: String| ctx.report_custom_error(&e, item))
    }
}

impl ToToml for RuleSeverity {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        Ok(Item::string(arena.alloc_str(&self.to_string())))
    }
}

/// Describes a single configurable rule parameter.
///
/// This powers `sprocket explain` and the generated documentation, and is used
/// to validate that a parameter is applicable to the rule it is configured on.
#[derive(Debug)]
pub struct ParamSpec {
    /// The name of the parameter as it appears in the configuration file.
    pub name: &'static str,
    /// A description of the parameter.
    pub description: &'static str,
    /// The default value of the parameter serialized as JSON.
    pub default: String,
    /// The rules this parameter applies to.
    pub applicable_rules: &'static [&'static str],
}

/// Defines the [`RuleConfig`] parameters along with their defaults, the rules
/// they apply to, and the introspection used by tooling.
macro_rules! define_rule_params {
    (
        $(
            $(#[doc = $doc:literal])+
            #[rules($($rule:ident),+ $(,)?)]
            $field:ident: $ty:ty = $default:expr,
        )+
    ) => {
        /// Per-rule configuration.
        ///
        /// A rule's table may set `severity` to override its severity (or `off`
        /// to disable it) along with any parameters applicable to that rule.
        #[derive(Clone, Debug, PartialEq, Eq, Toml)]
        #[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
        pub struct RuleConfig {
            /// The severity override for the rule.
            ///
            /// When absent, the rule emits at its built-in default severity.
            #[toml(default)]
            pub severity: Option<RuleSeverity>,
            $(
                $(#[doc = $doc])+
                #[toml(default = $default)]
                pub $field: $ty,
            )+
        }

        impl Default for RuleConfig {
            fn default() -> Self {
                Self {
                    severity: None,
                    $(
                        $field: $default,
                    )+
                }
            }
        }

        impl Config {
            /// Returns the metadata for every configurable rule parameter.
            pub fn params() -> Vec<ParamSpec> {
                vec![
                    $(
                        ParamSpec {
                            name: stringify!($field),
                            description: concat!($($doc, '\n',)*).trim(),
                            default: {
                                let default: $ty = $default;
                                serde_json::to_string(&default)
                                    .expect("parameter default should serialize")
                            },
                            applicable_rules: &[$(stringify!($rule)),+],
                        },
                    )+
                ]
            }
        }
    };
}

define_rule_params! {
    /// Names to ignore in the rule.
    ///
    /// ```toml
    /// allowed_names = ["GATK", "counter_int"]
    /// ```
    #[rules(SnakeCase, DeclarationName)]
    allowed_names: Vec<String> = Vec::new(),
    /// Runtime keys to allow in addition to the keys defined by the spec.
    ///
    /// ```toml
    /// allowed_runtime_keys = ["foo"]
    /// ```
    #[rules(ExpectedRuntimeKeys)]
    allowed_runtime_keys: Vec<String> = Vec::new(),
    /// The maximum allowed length, in characters, of a description.
    #[rules(DescriptionLength)]
    max_length: u64 = 140,
    /// The minimum length, in characters, below which a name is flagged as too
    /// short.
    #[rules(InputName, OutputName)]
    min_length: u64 = 3,
    /// Whether to flag disallowed name prefixes (such as `in_` or `out_`).
    #[rules(InputName, OutputName)]
    check_prefixes: bool = true,
    /// The comment keywords that trigger the rule.
    ///
    /// ```toml
    /// keywords = ["TODO", "FIXME"]
    /// ```
    #[rules(TodoComment)]
    keywords: Vec<String> = vec![String::from("TODO")],
}

/// The configuration for lint rules.
///
/// This is a table of per-rule settings keyed by rule ID. Rules without an
/// entry use their defaults.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Config(BTreeMap<String, RuleConfig>);

impl Config {
    /// Creates a configuration from a map of rule IDs to their settings.
    pub fn from_map(map: BTreeMap<String, RuleConfig>) -> Self {
        Self(map)
    }

    /// Gets the configuration for a rule, if one was provided.
    pub fn get(&self, rule_id: &str) -> Option<&RuleConfig> {
        self.0.get(rule_id)
    }

    /// Gets the resolved configuration for a rule.
    ///
    /// Returns the rule's provided configuration or the defaults when none was
    /// provided.
    pub fn resolved(&self, rule_id: &str) -> RuleConfig {
        self.0.get(rule_id).cloned().unwrap_or_default()
    }

    /// Gets the severity override for a rule, if any.
    pub fn severity_override(&self, rule_id: &str) -> Option<RuleSeverity> {
        self.0.get(rule_id).and_then(|c| c.severity)
    }

    /// Iterates over the configured rules and their settings.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &RuleConfig)> {
        self.0.iter()
    }
}

impl<'de> FromToml<'de> for Config {
    fn from_toml(ctx: &mut Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
        Ok(Self(BTreeMap::from_toml(ctx, item)?))
    }
}

impl ToToml for Config {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        self.0.to_toml(arena)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parses_severity_and_params() {
        let config: Config = toml_spanner::from_str(
            "[SnakeCase]\nseverity = \"error\"\nallowed_names = [\"GATK\"]\n",
        )
        .unwrap();
        assert_eq!(
            config.severity_override("SnakeCase"),
            Some(RuleSeverity::Error)
        );
        assert_eq!(
            config.resolved("SnakeCase").allowed_names,
            vec![String::from("GATK")]
        );
    }

    #[test]
    fn parses_multiple_rules() {
        let config: Config = toml_spanner::from_str(
            "[SnakeCase]\nseverity = \"off\"\n\n[DescriptionLength]\nmax_length = 200\n",
        )
        .unwrap();
        assert_eq!(
            config.severity_override("SnakeCase"),
            Some(RuleSeverity::Off)
        );
        assert_eq!(config.resolved("DescriptionLength").max_length, 200);
    }

    #[test]
    fn unset_rule_uses_defaults() {
        let config = Config::default();
        assert_eq!(config.severity_override("SnakeCase"), None);
        assert_eq!(config.resolved("DescriptionLength").max_length, 140);
        assert_eq!(config.resolved("InputName").min_length, 3);
        assert!(config.resolved("InputName").check_prefixes);
        assert_eq!(
            config.resolved("TodoComment").keywords,
            vec![String::from("TODO")]
        );
    }

    #[test]
    fn rejects_unknown_parameter() {
        let err = toml_spanner::from_str::<Config>("[SnakeCase]\nnot_a_param = 1\n").unwrap_err();
        assert!(err.to_string().contains("not_a_param"), "{err}");
    }

    #[test]
    fn rejects_invalid_severity() {
        let err =
            toml_spanner::from_str::<Config>("[SnakeCase]\nseverity = \"loud\"\n").unwrap_err();
        assert!(err.to_string().contains("off"), "{err}");
    }

    #[test]
    fn params_describe_applicable_rules() {
        let params = Config::params();
        let allowed_names = params
            .iter()
            .find(|p| p.name == "allowed_names")
            .expect("allowed_names param should exist");
        assert!(allowed_names.applicable_rules.contains(&"SnakeCase"));
        assert!(allowed_names.applicable_rules.contains(&"DeclarationName"));
    }
}
