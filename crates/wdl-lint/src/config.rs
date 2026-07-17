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
use toml_spanner::Error as TomlError;
use toml_spanner::Failed;
use toml_spanner::FromToml;
use toml_spanner::Item;
use toml_spanner::Key;
use toml_spanner::Table;
use toml_spanner::TableStyle;
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

/// A naming convention case style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseStyle {
    /// `snake_case`.
    Snake,
    /// `SCREAMING_SNAKE_CASE`.
    ScreamingSnake,
    /// `camelCase`.
    Camel,
    /// `PascalCase`.
    Pascal,
}

impl CaseStyle {
    /// Returns the human-readable name of this style for diagnostics.
    pub fn diagnostic_name(self) -> &'static str {
        match self {
            CaseStyle::Snake => "snake case",
            CaseStyle::ScreamingSnake => "screaming snake case",
            CaseStyle::Camel => "camel case",
            CaseStyle::Pascal => "pascal case",
        }
    }

    /// Converts a name to this case style.
    ///
    /// Digit boundaries are preserved (for example `v1` is not split into
    /// `v_1`).
    pub fn convert(self, name: &str) -> String {
        use convert_case::Boundary;
        use convert_case::Case;
        use convert_case::Converter;

        let case = match self {
            CaseStyle::Snake => Case::Snake,
            CaseStyle::ScreamingSnake => Case::UpperSnake,
            CaseStyle::Camel => Case::Camel,
            CaseStyle::Pascal => Case::Pascal,
        };
        Converter::new()
            .remove_boundaries(&[Boundary::DigitLower, Boundary::LowerDigit])
            .to_case(case)
            .convert(name)
    }

    /// Returns whether a name already matches this case style.
    pub fn matches(self, name: &str) -> bool {
        self.convert(name) == name
    }
}

impl FromStr for CaseStyle {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "snake_case" => Ok(CaseStyle::Snake),
            "screaming_snake_case" => Ok(CaseStyle::ScreamingSnake),
            "camelCase" => Ok(CaseStyle::Camel),
            "PascalCase" => Ok(CaseStyle::Pascal),
            _ => Err(format!(
                "expected one of `snake_case`, `screaming_snake_case`, `camelCase`, or \
                 `PascalCase`, found `{s}`"
            )),
        }
    }
}

impl fmt::Display for CaseStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            CaseStyle::Snake => "snake_case",
            CaseStyle::ScreamingSnake => "screaming_snake_case",
            CaseStyle::Camel => "camelCase",
            CaseStyle::Pascal => "PascalCase",
        };
        f.write_str(s)
    }
}

impl serde::Serialize for CaseStyle {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> FromToml<'de> for CaseStyle {
    fn from_toml(ctx: &mut Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
        let Some(s) = item.as_str() else {
            return Err(ctx.report_expected_but_found(&"a string", item));
        };
        s.parse()
            .map_err(|e: String| ctx.report_custom_error(&e, item))
    }
}

impl ToToml for CaseStyle {
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
macro_rules! rule_param_name {
    ($field:ident) => {
        stringify!($field)
    };
    ($field:ident @ $rename:tt) => {
        $rename
    };
}

/// Defines configurable rule parameters and their generated behavior.
macro_rules! define_rule_params {
    (
        $(
            $(#[doc = $doc:literal])+
            #[rules($($rule:ident),+ $(,)?)]
            $field:ident $(@ $rename:tt)?: $ty:ty = $default:expr,
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
                #[toml(default = $default $(, rename = $rename)?)]
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

        impl RuleConfig {
            /// Validates values that depend on the configured rule.
            pub fn validate(&self, rule_id: &str) -> Result<(), String> {
                if rule_id == "TodoComment" {
                    for (index, keyword) in self.keywords.iter().enumerate() {
                        if keyword.is_empty() {
                            return Err(String::from(
                                "`keywords` entries for rule `TodoComment` cannot be empty",
                            ));
                        }

                        if self.keywords[..index].contains(keyword) {
                            return Err(format!(
                                "`keywords` for rule `TodoComment` contains duplicate entry \
                                 `{keyword}`"
                            ));
                        }
                    }
                }

                Ok(())
            }

            /// Serializes only parameters applicable to the configured rule.
            fn to_toml_for_rule<'a>(
                &'a self,
                rule_id: &str,
                arena: &'a Arena,
            ) -> Result<Item<'a>, ToTomlError> {
                let mut table = Table::new();
                table.set_style(TableStyle::Header);

                if let Some(severity) = &self.severity {
                    table.insert_unique(
                        Key::new("severity"),
                        severity.to_toml(arena)?,
                        arena,
                    );
                }

                $(
                    if [$(stringify!($rule)),+].contains(&rule_id) {
                        table.insert_unique(
                            Key::new(rule_param_name!($field $(@ $rename)?)),
                            self.$field.to_toml(arena)?,
                            arena,
                        );
                    }
                )+

                Ok(table.into_item())
            }
        }

        impl Config {
            /// Returns the metadata for every configurable rule parameter.
            pub fn params() -> Vec<ParamSpec> {
                vec![
                    $(
                        ParamSpec {
                            name: rule_param_name!($field $(@ $rename)?),
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

            /// Returns whether a parameter name is recognized.
            pub fn has_parameter(parameter: &str) -> bool {
                parameter == "severity"
                    $(
                        || parameter == rule_param_name!($field $(@ $rename)?)
                    )+
            }

            /// Returns whether a parameter applies to a rule.
            pub fn parameter_applies_to(rule_id: &str, parameter: &str) -> bool {
                if parameter == "severity" {
                    return true;
                }

                $(
                    if parameter == rule_param_name!($field $(@ $rename)?) {
                        return [$(stringify!($rule)),+].contains(&rule_id);
                    }
                )+

                false
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
    #[rules(NamingConvention, DeclarationName)]
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
    /// The reserved meta keys that are required to have string values.
    #[rules(DocMetaStrings)]
    reserved_keys: Vec<String> = ["description", "help", "external_help", "warning", "category", "group"]
        .into_iter()
        .map(String::from)
        .collect(),
    /// The case style required for task names.
    #[rules(NamingConvention)]
    task: CaseStyle = CaseStyle::Snake,
    /// The case style required for workflow names.
    #[rules(NamingConvention)]
    workflow: CaseStyle = CaseStyle::Snake,
    /// The case style required for variable names (inputs, outputs, and private
    /// declarations).
    #[rules(NamingConvention)]
    variable: CaseStyle = CaseStyle::Snake,
    /// The case style required for user-defined type names and enum choices.
    #[rules(NamingConvention)]
    r#type @ "type": CaseStyle = CaseStyle::Pascal,
    /// The case style required for struct members.
    #[rules(NamingConvention)]
    struct_member: CaseStyle = CaseStyle::Snake,
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

    /// Sets the severity override for a rule, creating an entry with default
    /// parameters if one does not already exist.
    pub fn set_severity(&mut self, rule_id: &str, severity: RuleSeverity) {
        self.0.entry(rule_id.to_string()).or_default().severity = Some(severity);
    }

    /// Iterates over the configured rules and their settings.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &RuleConfig)> {
        self.0.iter()
    }
}

impl<'de> FromToml<'de> for Config {
    fn from_toml(ctx: &mut Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
        let table = item.require_table(ctx)?;
        let mut map = BTreeMap::new();
        let mut failed = false;

        for (key, value) in table {
            let rule_id = key.name;
            if !crate::ALL_RULE_IDS.iter().any(|id| id == rule_id) {
                ctx.push_error(TomlError::custom(
                    format!("unknown lint rule `{rule_id}`"),
                    key.span,
                ));
                failed = true;
                continue;
            }

            if let Some(entry) = value.as_table() {
                for (parameter, _) in entry {
                    if Self::has_parameter(parameter.name)
                        && !Self::parameter_applies_to(rule_id, parameter.name)
                    {
                        ctx.push_error(TomlError::custom(
                            format!(
                                "`{param}` is not a configurable parameter for rule `{rule_id}`",
                                param = parameter.name
                            ),
                            parameter.span,
                        ));
                        failed = true;
                    }
                }
            }

            match RuleConfig::from_toml(ctx, value) {
                Ok(config) => {
                    if let Err(error) = config.validate(rule_id) {
                        ctx.push_error(TomlError::custom(error, key.span));
                        failed = true;
                    } else {
                        map.insert(rule_id.to_string(), config);
                    }
                }
                Err(_) => failed = true,
            }
        }

        if failed { Err(Failed) } else { Ok(Self(map)) }
    }
}

impl ToToml for Config {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        let mut table = Table::new();
        table.set_style(TableStyle::Implicit);

        for (rule_id, config) in &self.0 {
            table.insert_unique(
                Key::new(rule_id),
                config.to_toml_for_rule(rule_id, arena)?,
                arena,
            );
        }

        Ok(table.into_item())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parses_severity_and_params() {
        let config: Config = toml_spanner::from_str(
            "[NamingConvention]\nseverity = \"error\"\nallowed_names = [\"GATK\"]\n",
        )
        .unwrap();
        assert_eq!(
            config.severity_override("NamingConvention"),
            Some(RuleSeverity::Error)
        );
        assert_eq!(
            config.resolved("NamingConvention").allowed_names,
            vec![String::from("GATK")]
        );
    }

    #[test]
    fn parses_multiple_rules() {
        let config: Config = toml_spanner::from_str(
            "[NamingConvention]\nseverity = \"off\"\n\n[DescriptionLength]\nmax_length = 200\n",
        )
        .unwrap();
        assert_eq!(
            config.severity_override("NamingConvention"),
            Some(RuleSeverity::Off)
        );
        assert_eq!(config.resolved("DescriptionLength").max_length, 200);
    }

    #[test]
    fn unset_rule_uses_defaults() {
        let config = Config::default();
        assert_eq!(config.severity_override("NamingConvention"), None);
        assert_eq!(config.resolved("DescriptionLength").max_length, 140);
        assert_eq!(config.resolved("InputName").min_length, 3);
        assert!(config.resolved("InputName").check_prefixes);
        let naming = config.resolved("NamingConvention");
        assert_eq!(naming.r#type, CaseStyle::Pascal);
        assert_eq!(naming.struct_member, CaseStyle::Snake);
        assert_eq!(
            config.resolved("TodoComment").keywords,
            vec![String::from("TODO")]
        );
    }

    #[test]
    fn parses_struct_member_case_style() {
        // SAFETY: the test input is static and known to use valid rule parameters.
        let config: Config =
            toml_spanner::from_str("[NamingConvention]\nstruct_member = \"camelCase\"\n").unwrap();
        assert_eq!(
            config.resolved("NamingConvention").struct_member,
            CaseStyle::Camel
        );
    }

    #[test]
    fn case_style_names_separate_config_literals_from_diagnostics() {
        let cases = [
            (CaseStyle::Snake, "snake_case", "snake case"),
            (
                CaseStyle::ScreamingSnake,
                "screaming_snake_case",
                "screaming snake case",
            ),
            (CaseStyle::Camel, "camelCase", "camel case"),
            (CaseStyle::Pascal, "PascalCase", "pascal case"),
        ];

        for (style, config_literal, diagnostic_name) in cases {
            assert_eq!(style.to_string(), config_literal);
            assert_eq!(style.diagnostic_name(), diagnostic_name);
        }
    }

    #[test]
    fn rejects_unknown_parameter() {
        let err =
            toml_spanner::from_str::<Config>("[NamingConvention]\nnot_a_param = 1\n").unwrap_err();
        assert!(err.to_string().contains("not_a_param"), "{err}");
    }

    #[test]
    fn rejects_unknown_rule() {
        let err = toml_spanner::from_str::<Config>("[NotARule]\nseverity = \"off\"\n").unwrap_err();
        assert!(err.to_string().contains("unknown lint rule"), "{err}");
    }

    #[test]
    fn rejects_inapplicable_parameter() {
        let err =
            toml_spanner::from_str::<Config>("[ContainerUri]\nmax_length = 10\n").unwrap_err();
        assert!(
            err.to_string().contains("not a configurable parameter"),
            "{err}"
        );
    }

    #[test]
    fn rejects_invalid_todo_keywords() {
        let empty =
            toml_spanner::from_str::<Config>("[TodoComment]\nkeywords = [\"\"]\n").unwrap_err();
        assert!(empty.to_string().contains("cannot be empty"), "{empty}");

        let duplicate =
            toml_spanner::from_str::<Config>("[TodoComment]\nkeywords = [\"TODO\", \"TODO\"]\n")
                .unwrap_err();
        assert!(
            duplicate.to_string().contains("duplicate entry `TODO`"),
            "{duplicate}"
        );
    }

    #[test]
    fn rejects_invalid_severity() {
        let err = toml_spanner::from_str::<Config>("[NamingConvention]\nseverity = \"loud\"\n")
            .unwrap_err();
        assert!(err.to_string().contains("off"), "{err}");
    }

    #[test]
    fn severity_round_trips_through_toml() {
        let config: Config =
            toml_spanner::from_str("[ContainerUri]\nseverity = \"warning\"\n").unwrap();
        let serialized = toml_spanner::to_string(&config).unwrap();
        assert!(!serialized.contains("allowed_names"));
        let reparsed: Config = toml_spanner::from_str(&serialized).unwrap();
        assert_eq!(
            reparsed.severity_override("ContainerUri"),
            Some(RuleSeverity::Warning)
        );
    }

    #[test]
    fn params_describe_applicable_rules() {
        let params = Config::params();
        let allowed_names = params
            .iter()
            .find(|p| p.name == "allowed_names")
            .expect("allowed_names param should exist");
        assert!(allowed_names.applicable_rules.contains(&"NamingConvention"));
        assert!(allowed_names.applicable_rules.contains(&"DeclarationName"));
    }
}
