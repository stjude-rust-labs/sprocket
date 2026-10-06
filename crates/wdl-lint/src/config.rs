//! Rule configuration.
//!
//! Every analysis and lint rule has its own table in [`Config`], keyed by the
//! rule's ID. Each table has a `severity` and any parameters specific to that
//! rule.

use schemars::JsonSchema;
use serde::Serialize;
use toml_spanner::Toml;
use wdl_analysis::DiagnosticsConfig;
use wdl_analysis::Rule as _;
use wdl_ast::Severity;

use crate::rules::BashSetOption;

/// The severity of a rule's diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Toml, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[toml(Toml, rename_all = "lowercase")]
#[schemars(rename_all = "lowercase")]
pub enum RuleSeverity {
    /// The rule is disabled.
    Off,
    /// The rule's diagnostics are notes.
    Note,
    /// The rule's diagnostics are warnings.
    Warning,
}

impl RuleSeverity {
    /// Gets the diagnostic severity, or `None` if the rule is `off`.
    pub fn severity(self) -> Option<Severity> {
        match self {
            Self::Off => None,
            Self::Note => Some(Severity::Note),
            Self::Warning => Some(Severity::Warning),
        }
    }

    /// Converts the built-in severity of an analysis rule.
    ///
    /// # Panics
    ///
    /// Panics if the severity is [`Severity::Error`]; configurable rules never
    /// report errors.
    fn from_builtin(severity: Severity) -> Self {
        match severity {
            Severity::Note => Self::Note,
            Severity::Warning => Self::Warning,
            Severity::Error => panic!("configurable rules cannot default to `error`"),
        }
    }
}

/// **(NOT A PUBLIC API)** A field of a rule's configuration table.
#[doc(hidden)]
#[derive(Debug)]
pub struct ConfigField {
    /// The name of the field.
    pub name: &'static str,
    /// The description of the field.
    pub description: &'static str,
    /// The default value of the field as a JSON string.
    pub default: String,
}

/// Defines the rule configuration.
///
/// Each rule gets its own configuration struct with a `severity` field and any
/// parameters, and [`Config`] gets one field per rule.
macro_rules! define_rules_config {
    (
        analysis {
            $(
                $a_id:tt => $a_field:ident: $a_ty:ident = $a_rule:ty;
            )+
        }
        lint {
            $(
                $l_id:tt => $l_field:ident: $l_ty:ident {
                    severity = $l_severity:expr;
                    $(
                        $(#[doc = $p_doc:literal])+
                        $p_field:ident: $p_ty:ty = $p_default:expr;
                    )*
                }
            )+
        }
    ) => {
        $(
            #[doc = concat!("Configuration for the `", $a_id, "` analysis rule.")]
            #[derive(Clone, Debug, PartialEq, Eq, Serialize, Toml, JsonSchema)]
            #[serde(rename_all = "snake_case")]
            #[toml(Toml, rename_all = "snake_case", warn_unknown_fields)]
            #[schemars(rename_all = "snake_case", default, deny_unknown_fields)]
            pub struct $a_ty {
                /// The severity of the rule's diagnostics: `off`, `note` or `warning`.
                #[toml(default = Self::default().severity)]
                pub severity: RuleSeverity,
            }

            impl Default for $a_ty {
                fn default() -> Self {
                    Self {
                        severity: RuleSeverity::from_builtin(<$a_rule>::default().severity()),
                    }
                }
            }
        )+

        $(
            #[doc = concat!("Configuration for the `", $l_id, "` lint rule.")]
            #[derive(Clone, Debug, PartialEq, Eq, Serialize, Toml, JsonSchema)]
            #[serde(rename_all = "snake_case")]
            #[toml(Toml, rename_all = "snake_case", warn_unknown_fields)]
            #[schemars(rename_all = "snake_case", default, deny_unknown_fields)]
            pub struct $l_ty {
                /// The severity of the rule's diagnostics: `off`, `note` or `warning`.
                #[toml(default = $l_severity)]
                pub severity: RuleSeverity,
                $(
                    $(#[doc = $p_doc])+
                    #[toml(default = $p_default)]
                    pub $p_field: $p_ty,
                )*
            }

            impl Default for $l_ty {
                fn default() -> Self {
                    Self {
                        severity: $l_severity,
                        $($p_field: $p_default,)*
                    }
                }
            }

            impl $l_ty {
                /// Gets the severity for the rule's diagnostics.
                ///
                /// A rule that is `off` is never run, so `note` is returned
                /// for it.
                #[allow(dead_code)]
                pub(crate) fn diagnostic_severity(&self) -> Severity {
                    self.severity.severity().unwrap_or(Severity::Note)
                }

                /// Gets the configuration fields of the rule.
                fn fields() -> Vec<ConfigField> {
                    let default = Self::default();
                    vec![
                        ConfigField {
                            name: "severity",
                            description: "The severity of the rule's diagnostics: `off`, `note` or `warning`.",
                            default: serde_json::to_string(&default.severity)
                                .expect("should serialize"),
                        },
                        $(
                            ConfigField {
                                name: stringify!($p_field),
                                description: concat!($($p_doc, '\n',)*).trim(),
                                default: serde_json::to_string(&default.$p_field)
                                    .expect("should serialize"),
                            },
                        )*
                    ]
                }
            }
        )+

        /// The configuration for analysis and lint rules.
        ///
        /// Each rule has its own table, keyed by the rule's ID, with a
        /// `severity` and any parameters specific to the rule. Rules that are
        /// not configured use their defaults.
        #[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Toml, JsonSchema)]
        #[toml(Toml, warn_unknown_fields)]
        #[schemars(rename = "WdlRulesConfig", default, deny_unknown_fields)]
        pub struct Config {
            $(
                #[doc = concat!("The `", $a_id, "` rule.")]
                #[serde(rename = $a_id)]
                #[toml(default, rename = $a_id, style = Header)]
                #[schemars(rename = $a_id)]
                pub $a_field: $a_ty,
            )+
            $(
                #[doc = concat!("The `", $l_id, "` rule.")]
                #[serde(rename = $l_id)]
                #[toml(default, rename = $l_id, style = Header)]
                #[schemars(rename = $l_id)]
                pub $l_field: $l_ty,
            )+
        }

        impl Config {
            /// Gets the configured severity of a rule.
            ///
            /// The rule ID is matched case-insensitively. Returns `None` if the
            /// rule is unknown.
            pub fn severity(&self, id: &str) -> Option<RuleSeverity> {
                $(
                    if id.eq_ignore_ascii_case($a_id) {
                        return Some(self.$a_field.severity);
                    }
                )+
                $(
                    if id.eq_ignore_ascii_case($l_id) {
                        return Some(self.$l_field.severity);
                    }
                )+
                None
            }

            /// Sets the severity of a rule.
            ///
            /// The rule ID is matched case-insensitively. Returns `false` if
            /// the rule is unknown.
            pub fn set_severity(&mut self, id: &str, severity: RuleSeverity) -> bool {
                $(
                    if id.eq_ignore_ascii_case($a_id) {
                        self.$a_field.severity = severity;
                        return true;
                    }
                )+
                $(
                    if id.eq_ignore_ascii_case($l_id) {
                        self.$l_field.severity = severity;
                        return true;
                    }
                )+
                false
            }

            /// Gets the analysis diagnostics configuration.
            ///
            /// Each analysis rule reports at its configured severity, and rules
            /// that are `off` are disabled.
            pub fn diagnostics_config(&self) -> DiagnosticsConfig {
                DiagnosticsConfig {
                    $($a_field: self.$a_field.severity.severity(),)+
                }
            }

            /// **(NOT A PUBLIC API)** Gets the configuration fields of a rule.
            ///
            /// Returns `None` if the rule is unknown.
            #[doc(hidden)]
            pub fn fields(id: &str) -> Option<Vec<ConfigField>> {
                $(
                    if id == $a_id {
                        let default = <$a_ty>::default();
                        return Some(vec![ConfigField {
                            name: "severity",
                            description: "The severity of the rule's diagnostics: `off`, `note` or `warning`.",
                            default: serde_json::to_string(&default.severity)
                                .expect("should serialize"),
                        }]);
                    }
                )+
                $(
                    if id == $l_id {
                        return Some(<$l_ty>::fields());
                    }
                )+
                None
            }
        }
    };
}

define_rules_config! {
    analysis {
        "UnusedImport" => unused_import: UnusedImportConfig
            = wdl_analysis::UnusedImportRule;
        "UnusedInput" => unused_input: UnusedInputConfig
            = wdl_analysis::UnusedInputRule;
        "UnusedDeclaration" => unused_declaration: UnusedDeclarationConfig
            = wdl_analysis::UnusedDeclarationRule;
        "UnusedCall" => unused_call: UnusedCallConfig
            = wdl_analysis::UnusedCallRule;
        "UnnecessaryFunctionCall" => unnecessary_function_call: UnnecessaryFunctionCallConfig
            = wdl_analysis::UnnecessaryFunctionCall;
        "UsingFallbackVersion" => using_fallback_version: UsingFallbackVersionConfig
            = wdl_analysis::UsingFallbackVersion;
        "MisleadingDeclarationOrder" => misleading_declaration_order: MisleadingDeclarationOrderConfig
            = wdl_analysis::MisleadingDeclarationOrderRule;
        "MeaninglessLintDirective" => meaningless_lint_directive: MeaninglessLintDirectiveConfig
            = wdl_analysis::MeaninglessLintDirective;
        "KnownRules" => known_rules: KnownRulesConfig
            = wdl_analysis::KnownRulesRule;
        "ExceptDirectiveValid" => except_directive_valid: ExceptDirectiveValidConfig
            = wdl_analysis::ExceptDirectiveValidRule;
        "CommandSectionIndentation" => command_section_indentation: CommandSectionIndentationConfig
            = wdl_analysis::CommandSectionIndentationRule;
        "DeprecatedObject" => deprecated_object: DeprecatedObjectConfig
            = wdl_analysis::DeprecatedObjectRule;
        "DeprecatedPlaceholder" => deprecated_placeholder: DeprecatedPlaceholderConfig
            = wdl_analysis::DeprecatedPlaceholderRule;
        "DeprecatedRuntimeSection" => deprecated_runtime_section: DeprecatedRuntimeSectionConfig
            = wdl_analysis::DeprecatedRuntimeSectionRule;
    }
    lint {
        "BashSetSyntax" => bash_set_syntax: BashSetSyntaxConfig {
            severity = RuleSeverity::Warning;
            /// List of options to enforce in the bash `set` builtin for every
            /// `command` section.
            ///
            /// ##### Example
            ///
            /// ```toml
            /// [check.rules.BashSetSyntax]
            /// bash_set_options = ["errexit", "nounset", "pipefail"]
            /// ```
            bash_set_options: Vec<BashSetOption> = vec![
                BashSetOption::ErrExit,
                BashSetOption::NoUnset,
                BashSetOption::Pipefail,
            ];
        }
        "CallInputKeyword" => call_input_keyword: CallInputKeywordConfig {
            severity = RuleSeverity::Note;
        }
        "ConciseInput" => concise_input: ConciseInputConfig {
            severity = RuleSeverity::Note;
        }
        "ContainerUri" => container_uri: ContainerUriConfig {
            severity = RuleSeverity::Warning;
        }
        "DeprecatedRuntimeKey" => deprecated_runtime_key: DeprecatedRuntimeKeyConfig {
            severity = RuleSeverity::Note;
        }
        "DeclarationName" => declaration_name: DeclarationNameConfig {
            severity = RuleSeverity::Note;
            /// List of declaration names to ignore.
            ///
            /// ##### Example
            ///
            /// ```toml
            /// [check.rules.DeclarationName]
            /// allowed_names = ["counter_int"]
            /// ```
            allowed_names: Vec<String> = Vec::new();
        }
        "ExtraneousParameterMeta" => extraneous_parameter_meta: ExtraneousParameterMetaConfig {
            severity = RuleSeverity::Note;
        }
        "DenyGlobStar" => deny_glob_star: DenyGlobStarConfig {
            severity = RuleSeverity::Warning;
        }
        "DescriptionLength" => description_length: DescriptionLengthConfig {
            severity = RuleSeverity::Off;
        }
        "DocCommentTabs" => doc_comment_tabs: DocCommentTabsConfig {
            severity = RuleSeverity::Warning;
        }
        "DocMetaStrings" => doc_meta_strings: DocMetaStringsConfig {
            severity = RuleSeverity::Off;
        }
        "EmptyDocComment" => empty_doc_comment: EmptyDocCommentConfig {
            severity = RuleSeverity::Note;
        }
        "EmptyOutputs" => empty_outputs: EmptyOutputsConfig {
            severity = RuleSeverity::Note;
        }
        "UnknownRuntimeKeys" => unknown_runtime_keys: UnknownRuntimeKeysConfig {
            severity = RuleSeverity::Warning;
            /// List of `runtime` keys to ignore.
            ///
            /// ##### Example
            ///
            /// ```toml
            /// [check.rules.UnknownRuntimeKeys]
            /// allowed_runtime_keys = ["foo"]
            /// ```
            allowed_runtime_keys: Vec<String> = Vec::new();
        }
        "HereDocCommands" => heredoc_commands: HereDocCommandsConfig {
            severity = RuleSeverity::Warning;
        }
        "HostPathLiterals" => host_path_literals: HostPathLiteralsConfig {
            severity = RuleSeverity::Warning;
        }
        "ImportPlacement" => import_placement: ImportPlacementConfig {
            severity = RuleSeverity::Warning;
        }
        "InlineInstall" => inline_install: InlineInstallConfig {
            severity = RuleSeverity::Warning;
        }
        "InputName" => input_name: InputNameConfig {
            severity = RuleSeverity::Note;
            /// The minimum length of input names; shorter names are flagged.
            ///
            /// ##### Example
            ///
            /// ```toml
            /// [check.rules.InputName]
            /// min_length = 5
            /// ```
            min_length: u8 = 3;
            /// Whether to flag input names that start with a disallowed
            /// prefix (`in`/`In` followed by an uppercase letter or underscore, or `input`).
            ///
            /// ##### Example
            ///
            /// ```toml
            /// [check.rules.InputName]
            /// check_prefixes = false
            /// ```
            check_prefixes: bool = true;
        }
        "MatchingOutputMeta" => matching_output_meta: MatchingOutputMetaConfig {
            severity = RuleSeverity::Warning;
        }
        "MetaDescription" => meta_description: MetaDescriptionConfig {
            severity = RuleSeverity::Note;
        }
        "MetaSections" => meta_sections: MetaSectionsConfig {
            severity = RuleSeverity::Note;
        }
        "MutableContainerTag" => mutable_container_tag: MutableContainerTagConfig {
            severity = RuleSeverity::Note;
        }
        "OutputName" => output_name: OutputNameConfig {
            severity = RuleSeverity::Note;
            /// The minimum length of output names; shorter names are flagged.
            ///
            /// ##### Example
            ///
            /// ```toml
            /// [check.rules.OutputName]
            /// min_length = 5
            /// ```
            min_length: u8 = 3;
            /// Whether to flag output names that start with a disallowed
            /// prefix (`out`/`Out` followed by an uppercase letter or underscore, or `output`).
            ///
            /// ##### Example
            ///
            /// ```toml
            /// [check.rules.OutputName]
            /// check_prefixes = false
            /// ```
            check_prefixes: bool = true;
        }
        "OutputMetaOrder" => output_meta_order: OutputMetaOrderConfig {
            severity = RuleSeverity::Note;
        }
        "ParameterDescription" => parameter_description: ParameterDescriptionConfig {
            severity = RuleSeverity::Note;
        }
        "MissingParameterMeta" => missing_parameter_meta: MissingParameterMetaConfig {
            severity = RuleSeverity::Warning;
        }
        "ParameterMetaOrder" => parameter_meta_order: ParameterMetaOrderConfig {
            severity = RuleSeverity::Note;
        }
        "PascalCase" => pascal_case: PascalCaseConfig {
            severity = RuleSeverity::Warning;
        }
        "RedundantNone" => redundant_none: RedundantNoneConfig {
            severity = RuleSeverity::Note;
        }
        "RecommendedRuntimeKeys" => recommended_runtime_keys: RecommendedRuntimeKeysConfig {
            severity = RuleSeverity::Note;
        }
        "RequirementsSection" => requirements_section: RequirementsSectionConfig {
            severity = RuleSeverity::Warning;
        }
        "RedundantContainerArray" => redundant_container_array: RedundantContainerArrayConfig {
            severity = RuleSeverity::Note;
        }
        "RuntimeSection" => runtime_section: RuntimeSectionConfig {
            severity = RuleSeverity::Warning;
        }
        "ShellSplitting" => shell_splitting: ShellSplittingConfig {
            severity = RuleSeverity::Warning;
        }
        "ShellCheck" => shellcheck: ShellCheckConfig {
            severity = RuleSeverity::Warning;
        }
        "SnakeCase" => snake_case: SnakeCaseConfig {
            severity = RuleSeverity::Warning;
            /// List of names to ignore.
            ///
            /// ##### Example
            ///
            /// ```toml
            /// [check.rules.SnakeCase]
            /// allowed_names = ["Foo"]
            /// ```
            allowed_names: Vec<String> = Vec::new();
        }
        "FlaggedComment" => flagged_comment: FlaggedCommentConfig {
            severity = RuleSeverity::Note;
            /// List of keywords to flag in comments.
            ///
            /// Keywords are matched as case-sensitive substrings of the comment
            /// text, and cannot be empty. When keywords overlap (for example,
            /// `FIX` and `FIXME`), the longest keyword is reported.
            ///
            /// ##### Example
            ///
            /// ```toml
            /// [check.rules.FlaggedComment]
            /// keywords = ["TODO", "FIXME", "XXX"]
            /// ```
            keywords: Vec<String> = vec![String::from("TODO")];
        }
        "UnusedDocComments" => unused_doc_comments: UnusedDocCommentsConfig {
            severity = RuleSeverity::Note;
        }
    }
}
