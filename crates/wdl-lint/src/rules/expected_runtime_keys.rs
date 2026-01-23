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
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Ident;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::TokenText;
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
use wdl_ast::v1::TaskDefinition;
use wdl_ast::version::V1;

use crate::Config;
use crate::Rule;
use crate::Tag;
use crate::TagSet;
use crate::util::serialize_oxford_comma;

/// The identifier for the runtime section rule.
const ID: &str = "ExpectedRuntimeKeys";

/// A kind of runtime key.
///
/// These are intended to be assigned at a per-version level of granularity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum KeyKind {
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
    pub fn is_recommended(&self) -> bool {
        *self == KeyKind::Recommended
    }
}

/// The mapping between `runtime` keys and their kind for WDL v1.0.
///
/// Link: https://github.com/openwdl/wdl/blob/main/versions/1.0/SPEC.md#runtime-section
fn keys_v1_0() -> &'static HashMap<&'static str, KeyKind> {
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
fn keys_v1_1() -> &'static HashMap<&'static str, KeyKind> {
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

/// Creates a "deprecated runtime key" diagnostic.
fn deprecated_runtime_key(key: &Ident, replacement: &str) -> Diagnostic {
    Diagnostic::note(format!(
        "the `{key}` runtime key has been deprecated in favor of `{replacement}`",
        key = key.text()
    ))
    .with_rule(ID)
    .with_highlight(key.span())
    .with_fix(format!(
        "replace the `{key}` key with `{replacement}`",
        key = key.text()
    ))
}

/// Creates a "non-reserved runtime key" diagnostic.
///
/// Returns `None` if every given key is allowed.
fn report_non_reserved_runtime_keys(
    keys: &HashSet<TokenText>,
    allowed_runtime_keys: &HashSet<String>,
    runtime_span: Span,
    specification: &str,
) -> Option<Diagnostic> {
    assert!(!keys.is_empty());

    let mut key_names = keys
        .iter()
        .map(|key| key.text())
        .filter(|text| !allowed_runtime_keys.contains(*text))
        .collect::<Vec<_>>();
    if key_names.is_empty() {
        return None;
    }

    key_names.sort();

    let (message, fix) = if key_names.len() == 1 {
        // SAFETY: we just checked to make sure there is exactly one element in
        // `keys`, so this will always unwrap.
        let key = key_names.into_iter().next().unwrap();

        (
            format!(
                "the following runtime key is not reserved in {specification}: `{key}`; \
                 therefore, its inclusion in the `runtime` section is deprecated"
            ),
            format!(
                "if a reserved key name was intended, correct the spelling; otherwise, remove the \
                 `{key}` key"
            ),
        )
    } else {
        // SAFETY: we know that this has more than one element because we
        // asserted the input `Vec` not be empty above. As such, this will
        // always produce a result.
        let keys = serialize_oxford_comma(
            &key_names
                .into_iter()
                .map(|key| format!("`{key}`"))
                .collect::<Vec<_>>(),
        )
        .unwrap();

        (
            format!(
                "the following runtime keys are not reserved in {specification}: {keys}; \
                 therefore, their inclusion in the `runtime` section is deprecated"
            ),
            format!(
                "if reserved key names were intended, correct the spelling of each key; \
                 otherwise, remove the {keys} keys"
            ),
        )
    };

    let mut diagnostic = Diagnostic::warning(message)
        .with_rule(ID)
        .with_highlight(runtime_span)
        .with_fix(fix);

    for key in keys.iter() {
        diagnostic = diagnostic.with_label(
            format!("the `{key}` key should be removed", key = key.text()),
            key.span(),
        );
    }

    Some(diagnostic)
}

/// Creates a "missing recommended runtime key" diagnostic.
fn report_missing_recommended_keys(
    mut keys: Vec<&str>,
    runtime_span: Span,
    specification: &str,
) -> Diagnostic {
    assert!(!keys.is_empty());
    keys.sort();

    let (message, fix) = if keys.len() == 1 {
        // SAFETY: we just checked to make sure there is exactly one element in
        // `keys`, so this will always unwrap.
        let key = keys.first().unwrap();

        (
            format!("the following runtime key is recommended by {specification}: `{key}`"),
            format!("include an entry for the `{key}` key in the `runtime` section"),
        )
    } else {
        // SAFETY: we know that this has more than one element because we
        // asserted the input `Vec` not be empty above. As such, this will
        // always produce a result.
        let keys = serialize_oxford_comma(
            &keys
                .iter()
                .map(|key| format!("`{key}`"))
                .collect::<Vec<_>>(),
        )
        .unwrap();

        (
            format!("the following runtime keys are recommended by {specification}: {keys}"),
            format!("include entries for the {keys} keys in the `runtime` section"),
        )
    };

    Diagnostic::note(message)
        .with_rule(ID)
        .with_highlight(runtime_span)
        .with_fix(fix)
}

/// Detects the use of deprecated, unknown, or missing runtime keys.
#[derive(Debug, Clone)]
pub struct ExpectedRuntimeKeysRule {
    /// The detected version of the current document.
    version: Option<SupportedVersion>,
    /// The span of the first `runtime` section encountered within the current
    /// task.
    runtime_span: Option<Span>,
    /// Whether or not we've already processed a `runtime` section within the
    /// current task.
    runtime_processed_for_task: bool,
    /// All keys encountered in the current runtime section.
    encountered_keys: Vec<Ident>,
    /// All non-reserved keys encountered in the current runtime section.
    non_reserved_keys: HashSet<TokenText>,
    /// Allowed keys from the config.
    allowed_runtime_keys: HashSet<String>,
}

impl ExpectedRuntimeKeysRule {
    /// Create a new instance of `ExpectedRuntimeKeysRule`
    pub fn new(config: &Config) -> Self {
        Self {
            version: None,
            runtime_span: None,
            runtime_processed_for_task: false,
            encountered_keys: Vec::new(),
            non_reserved_keys: HashSet::new(),
            allowed_runtime_keys: config.allowed_runtime_keys.clone(),
        }
    }
}

impl Rule for ExpectedRuntimeKeysRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that `runtime` sections have the appropriate keys."
    }

    fn explanation(&self) -> &'static str {
        "The behavior of this rule is different depending on the WDL version:

        For WDL v1.0 documents, the `docker` and `memory` keys are recommended, but the inclusion \
         of any number of other keys is permitted.

        For WDL v1.1 documents,

        - A list of mandatory, reserved keywords will be recommended for inclusion if they are not \
         present. Here, 'mandatory' refers to the requirement that all execution engines support \
         this key—not that the key must be present in the `runtime` section.
        - Optional, reserved \"hint\" keys are also permitted but not flagged when they are \
         missing (as their support in execution engines is not guaranteed).
        - The WDL v1.1 specification deprecates the inclusion of non-reserved keys in a  `runtime` \
         section. As such, any non-reserved keys will be flagged for removal.

         For WDL v1.2 documents and later, this rule does not evaluate because `runtime` sections \
         were deprecated in this version."
    }

    fn tags(&self) -> crate::TagSet {
        TagSet::new(&[Tag::Completeness, Tag::Deprecated])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::RuntimeSectionNode,
        ])
    }

    fn related_rules(&self) -> &[&'static str] {
        &["DeprecatedObject", "DeprecatedPlaceholder"]
    }
}

/// A utility method to parse the recommended keys from a static set of runtime
/// keys from either WDL v1.0 or WDL v1.1.
fn recommended_keys<'a, 'k>(
    keys: &'a HashMap<&'k str, KeyKind>,
) -> impl Iterator<Item = (&'k str, &'a KeyKind)> {
    keys.iter()
        .filter(|(_, kind)| kind.is_recommended())
        .map(|(key, kind)| (*key, kind))
}

impl Visitor for ExpectedRuntimeKeysRule {
    fn reset(&mut self) {
        self.version = None;
        self.runtime_span = None;
        self.runtime_processed_for_task = false;
        self.encountered_keys.clear();
        self.non_reserved_keys.clear();
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
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        def: &TaskDefinition,
    ) {
        match reason {
            VisitReason::Enter => {
                self.runtime_processed_for_task = false;
                self.runtime_span = None;
                self.encountered_keys.clear();
                self.non_reserved_keys.clear();
            }
            VisitReason::Exit => {
                // If a runtime section span has not been encountered, then
                // there won't be any keys to report and we can return early.
                let runtime_span = match self.runtime_span {
                    Some(span) => span,
                    None => return,
                };
                let runtime_node = def
                    .runtime()
                    .expect("runtime section should exist")
                    .inner()
                    .clone();

                // SAFETY: the version must always be set before we get to this
                // point, as document is the root node of the tree.
                if let SupportedVersion::V1(minor_version) = self.version.unwrap() {
                    let specification = format!("the WDL {minor_version} specification");

                    if !self.non_reserved_keys.is_empty()
                        && let Some(diagnostic) = report_non_reserved_runtime_keys(
                            &self.non_reserved_keys,
                            &self.allowed_runtime_keys,
                            runtime_span,
                            &specification,
                        )
                    {
                        diagnostics.exceptable_add(
                            diagnostic,
                            SyntaxElement::from(runtime_node.clone()),
                            &self.exceptable_nodes(),
                        );
                    }

                    let recommended_keys = match minor_version {
                        V1::Zero => recommended_keys(keys_v1_0()),
                        V1::One => recommended_keys(keys_v1_1()),
                        _ => return,
                    };

                    let missing_keys = recommended_keys
                        .filter(|(key, _)| !self.encountered_keys.iter().any(|s| s.text() == *key))
                        .map(|(key, _)| key)
                        .collect::<Vec<_>>();

                    if !missing_keys.is_empty() {
                        diagnostics.exceptable_add(
                            report_missing_recommended_keys(
                                missing_keys,
                                runtime_span,
                                &specification,
                            ),
                            SyntaxElement::from(runtime_node),
                            &self.exceptable_nodes(),
                        );
                    }
                }
            }
        }
    }

    fn runtime_section(
        &mut self,
        _: &mut Diagnostics,
        reason: VisitReason,
        section: &RuntimeSection,
    ) {
        // NOTE: if we've already processed a `runtime` section for this task
        // and we hit this again, that means there are multiple `runtime`
        // sections in the task. In that case, validation should report that
        // this cannot occur, and the runtime section should be ignored.
        if self.runtime_processed_for_task {
            return;
        }

        match reason {
            VisitReason::Enter => {
                self.runtime_span = match self.runtime_span {
                    // SAFETY: we should never encounter a case where a
                    // `runtime` section is entered before a previous `runtime`
                    // section is exited.
                    Some(_) => unreachable!(),
                    None => Some(
                        section
                            .inner()
                            .first_token()
                            .expect("runtime section should have tokens")
                            .text_range()
                            .into(),
                    ),
                };
            }
            VisitReason::Exit => {
                self.runtime_processed_for_task = true;
            }
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
            // - WDL v1.0 contains no deprecated keys: the only issue that can occur is when
            //   one of the two recommended key is omitted, and that is handled at the end
            //   of the `document()` method.
            // - WDL v1.2 deprecates the `runtime` section, so any WDL document with a
            //   version of 1.2 or later should ignore the keys and report the section as
            //   deprecated (in another rule).
            if minor_version == V1::One {
                match keys_v1_1().get(key_name.text()) {
                    Some(kind) => {
                        // If the key was found in the map, the only potential
                        // problem that can be encountered is if the key is
                        // deprecated.
                        if let KeyKind::Deprecated(replacement) = kind {
                            diagnostics.exceptable_add(
                                deprecated_runtime_key(&key_name, replacement),
                                SyntaxElement::from(item.inner().clone()),
                                &self.exceptable_nodes(),
                            );
                        }
                    }
                    None => {
                        // If the key was _not_ found in the map, that means the
                        // key was not one of the permitted values for WDL v1.1.
                        self.non_reserved_keys.insert(key_name.hashable());
                    }
                }
            }
        }

        self.encountered_keys.push(key_name);
    }
}
