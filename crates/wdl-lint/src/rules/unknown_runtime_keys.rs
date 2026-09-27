//! A lint rule for the `runtime` section keys.
//!
//! Note that this lint rule will only emit diagnostics for WDL documents that
//! have a major version of 1 but a minor version of less than 2, as the
//! `runtime` section was deprecated in WDL v1.2.

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::OnceLock;

use wdl_analysis::Diagnostics;
use wdl_analysis::Document;
use wdl_analysis::Example;
use wdl_analysis::LabeledSnippet;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Severity;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::RuntimeItem;
use wdl_ast::v1::RuntimeSection;
use wdl_ast::v1::TASK_HINT_INPUTS;
use wdl_ast::v1::TASK_HINT_LOCALIZATION_OPTIONAL_ALIAS;
use wdl_ast::v1::TASK_HINT_MAX_CPU_ALIAS;
use wdl_ast::v1::TASK_HINT_MAX_MEMORY_ALIAS;
use wdl_ast::v1::TASK_HINT_OUTPUTS;
use wdl_ast::v1::TASK_HINT_SHORT_TASK_ALIAS;
use wdl_ast::v1::TASK_REQUIREMENT_CONTAINER;
use wdl_ast::v1::TASK_REQUIREMENT_CONTAINER_ALIAS;
use wdl_ast::v1::TASK_REQUIREMENT_CPU;
use wdl_ast::v1::TASK_REQUIREMENT_DISKS;
use wdl_ast::v1::TASK_REQUIREMENT_GPU;
use wdl_ast::v1::TASK_REQUIREMENT_MAX_RETRIES_ALIAS;
use wdl_ast::v1::TASK_REQUIREMENT_MEMORY;
use wdl_ast::v1::TASK_REQUIREMENT_RETURN_CODES_ALIAS;
use wdl_ast::version::V1;

use crate::Config;
use crate::Rule;
use crate::Tag;
use crate::TagSet;
use crate::util::diagnostic;

/// The identifier for the runtime section rule.
const ID: &str = "UnknownRuntimeKeys";

/// A kind of runtime key.
///
/// These are intended to be assigned at a per-version level of granularity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum KeyKind {
    /// A key that is deprecated in favor of another key.
    Deprecated(
        /// The equivalent key that should be used instead.
        &'static str,
    ),
    /// A runtime key that is recommended to be included.
    Recommended,
    /// A runtime key that has a reserved meaning in the specification but which
    /// execution engines are _not_ required to support. These are also called
    /// "hints" in WDL parlance.
    ReservedHint,
    /// A runtime key that has a reserved meaning in the specification and which
    /// execution engines are required to support (but don't necessarily have to
    /// be present in WDL documents).
    ReservedMandatory,
}

impl KeyKind {
    /// Returns whether a key is recommended to be included.
    pub(crate) fn is_recommended(&self) -> bool {
        *self == KeyKind::Recommended
    }
}

/// The mapping between `runtime` keys and their kind for WDL v1.0.
///
/// Link: https://github.com/openwdl/wdl/blob/main/versions/1.0/SPEC.md#runtime-section
pub(crate) fn keys_v1_0() -> &'static HashMap<&'static str, KeyKind> {
    /// Keys and their kind for WDL v1.0.
    static KEYS_V1_0: OnceLock<HashMap<&'static str, KeyKind>> = OnceLock::new();

    KEYS_V1_0.get_or_init(|| {
        let mut keys = HashMap::new();
        keys.insert(TASK_REQUIREMENT_CONTAINER_ALIAS, KeyKind::Recommended);
        keys.insert(TASK_REQUIREMENT_MEMORY, KeyKind::Recommended);
        keys
    })
}

/// The mapping between `runtime` keys and their kind for WDL v1.1.
///
/// Link: https://github.com/openwdl/wdl/blob/wdl-1.1/SPEC.md#runtime-section
pub(crate) fn keys_v1_1() -> &'static HashMap<&'static str, KeyKind> {
    /// Keys and their kind for WDL v1.1.
    static KEYS_V1_1: OnceLock<HashMap<&'static str, KeyKind>> = OnceLock::new();

    KEYS_V1_1.get_or_init(|| {
        let mut keys = HashMap::new();
        keys.insert(TASK_REQUIREMENT_CONTAINER, KeyKind::Recommended);
        keys.insert(
            TASK_REQUIREMENT_CONTAINER_ALIAS,
            KeyKind::Deprecated(TASK_REQUIREMENT_CONTAINER),
        );
        keys.insert(TASK_REQUIREMENT_CPU, KeyKind::ReservedMandatory);
        keys.insert(TASK_REQUIREMENT_MEMORY, KeyKind::ReservedMandatory);
        keys.insert(TASK_REQUIREMENT_GPU, KeyKind::ReservedMandatory);
        keys.insert(TASK_REQUIREMENT_DISKS, KeyKind::ReservedMandatory);
        keys.insert(
            TASK_REQUIREMENT_MAX_RETRIES_ALIAS,
            KeyKind::ReservedMandatory,
        );
        keys.insert(
            TASK_REQUIREMENT_RETURN_CODES_ALIAS,
            KeyKind::ReservedMandatory,
        );
        keys.insert(TASK_HINT_MAX_CPU_ALIAS, KeyKind::ReservedHint);
        keys.insert(TASK_HINT_MAX_MEMORY_ALIAS, KeyKind::ReservedHint);
        keys.insert(TASK_HINT_SHORT_TASK_ALIAS, KeyKind::ReservedHint);
        keys.insert(TASK_HINT_LOCALIZATION_OPTIONAL_ALIAS, KeyKind::ReservedHint);
        keys.insert(TASK_HINT_INPUTS, KeyKind::ReservedHint);
        keys.insert(TASK_HINT_OUTPUTS, KeyKind::ReservedHint);
        keys
    })
}

/// Creates a "non-reserved runtime key" diagnostic for a specific `key`
fn report_non_reserved_runtime_key(
    severity: Severity,
    key: &str,
    span: Span,
    specification: &str,
) -> Diagnostic {
    diagnostic(
        severity,
        format!(
            "the runtime key `{key}` is not reserved in {specification}; arbitrary runtime keys \
             are deprecated"
        ),
    )
    .with_rule(ID)
    .with_highlight(span)
    .with_fix(format!("remove the `{key}` key"))
}

/// Detects the use of unknown runtime keys.
#[derive(Debug, Clone)]
pub struct UnknownRuntimeKeysRule {
    /// The severity of the rule's diagnostics.
    severity: Severity,
    /// The detected version of the current document.
    version: Option<SupportedVersion>,
    /// Whether or not we've already processed a `runtime` section within the
    /// current task.
    runtime_processed_for_task: bool,
    /// Allowed keys from the config.
    allowed_runtime_keys: HashSet<String>,
}

impl UnknownRuntimeKeysRule {
    /// Create a new instance of `UnknownRuntimeKeysRule`
    pub fn new(config: &Config) -> Self {
        Self {
            severity: config.unknown_runtime_keys.diagnostic_severity(),
            version: None,
            runtime_processed_for_task: false,
            allowed_runtime_keys: HashSet::from_iter(
                config
                    .unknown_runtime_keys
                    .allowed_runtime_keys
                    .iter()
                    .cloned(),
            ),
        }
    }
}

impl Default for UnknownRuntimeKeysRule {
    fn default() -> Self {
        Self::new(&Config::default())
    }
}

impl Rule for UnknownRuntimeKeysRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that WDL 1.1 `runtime` sections use reserved keys."
    }

    fn explanation(&self) -> &'static str {
        "The behavior of this rule is different depending on the WDL version:

For WDL v1.0 documents, this rule does not emit diagnostics.

For WDL v1.1 documents, the specification deprecates the inclusion of non-reserved keys in a \
         `runtime` section. As such, any non-reserved keys will be flagged for removal.

For WDL v1.2 documents and later, this rule does not evaluate because `runtime` sections were \
         deprecated in this version."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: Some("The following has an unexpected key"),
                snippet: r#"version 1.1

task unexpected_runtime_key {
    runtime {
        container: "ubuntu"
        foo: "bar"
    }
}
"#,
            },
            revised: None,
        }]
    }

    fn tags(&self) -> crate::TagSet {
        TagSet::new(&[Tag::Completeness, Tag::Deprecated])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::RuntimeSectionNode,
            SyntaxKind::RuntimeItemNode,
        ])
    }

    fn related_rules(&self) -> &'static [&'static str] {
        &[
            "DeprecatedObject",
            "DeprecatedPlaceholder",
            "DeprecatedRuntimeKey",
            "RecommendedRuntimeKeys",
        ]
    }
}

/// A utility method to parse the recommended keys from a static set of runtime
/// keys from either WDL v1.0 or WDL v1.1.
pub(crate) fn recommended_keys<'a, 'k>(
    keys: &'a HashMap<&'k str, KeyKind>,
) -> impl Iterator<Item = (&'k str, &'a KeyKind)> {
    keys.iter()
        .filter(|(_, kind)| kind.is_recommended())
        .map(|(key, kind)| (*key, kind))
}

impl Visitor for UnknownRuntimeKeysRule {
    fn reset(&mut self) {
        self.version = None;
        self.runtime_processed_for_task = false;
    }

    fn document(
        &mut self,
        _: &mut Diagnostics,
        reason: VisitReason,
        _: &Document,
        version: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        self.version = Some(version);
    }

    fn task_definition(
        &mut self,
        _: &mut Diagnostics,
        reason: VisitReason,
        _: &wdl_ast::v1::TaskDefinition,
    ) {
        if reason == VisitReason::Exit {
            self.runtime_processed_for_task = false;
        }
    }

    fn runtime_section(
        &mut self,
        _diagnostics: &mut Diagnostics,
        reason: VisitReason,
        _section: &RuntimeSection,
    ) {
        if reason == VisitReason::Exit {
            self.runtime_processed_for_task = true;
        }
    }

    fn runtime_item(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        item: &RuntimeItem,
    ) {
        // NOTE: if we've already processed a `runtime` section for this task
        // and we hit this again, that means there are multiple `runtime`
        // sections in the task. In that case, validation should report that
        // this cannot occur, and the runtime items should be ignored.
        if self.runtime_processed_for_task || reason == VisitReason::Exit {
            return;
        }

        let key_name = item.name();

        // SAFETY: the version must always be set before we get to this point,
        // as document is the root node of the tree.
        if let SupportedVersion::V1(minor_version) = self.version.unwrap() {
            // The only keys that need to be individually inspected are WDL v1.1
            // keys because,
            //
            // - WDL v1.0 contains no deprecated keys: the only issue that can
            //   occur is when one of the two recommended key is omitted, and
            //   that is handled at the end of the `document()` method.
            // - WDL v1.2 deprecates the `runtime` section, so any WDL document
            //   with a version of 1.2 or later should ignore the keys and
            //   report the section as deprecated (in another rule).
            if minor_version == V1::One {
                match keys_v1_1().get(key_name.text()) {
                    Some(_) => {}
                    None => {
                        let specification = format!("the WDL {minor_version} specification");
                        let key_text = key_name.text();
                        // If the key was _not_ found in the map, that means the
                        // key was not one of the permitted values for WDL v1.1.
                        //
                        // If it's also not in the explicitly allowed
                        // configuration,
                        // add a diagnostic for it.
                        if !self.allowed_runtime_keys.contains(key_text) {
                            // Note we don't use `item.span()` here to avoid
                            // highlighting the whole
                            // runtime object.
                            let text_for_key_span = item
                                .inner()
                                .first_token()
                                .expect("RuntimeItem must have text in first token")
                                .text_range()
                                .into();
                            diagnostics.exceptable_add(
                                report_non_reserved_runtime_key(
                                    self.severity,
                                    key_text,
                                    text_for_key_span,
                                    &specification,
                                ),
                                item.inner(),
                                &self.exceptable_nodes(),
                            );
                        }
                    }
                }
            }
        }
    }
}
