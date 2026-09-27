//! A lint rule for recommended `runtime` keys.

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
use wdl_ast::version::V1;

use crate::Config;
use crate::Rule;
use crate::Tag;
use crate::TagSet;
use crate::rules::keys_v1_0;
use crate::rules::keys_v1_1;
use crate::rules::recommended_keys;
use crate::util::diagnostic;
use crate::util::serialize_oxford_comma;

/// The identifier for the recommended runtime keys rule.
const ID: &str = "RecommendedRuntimeKeys";

/// Creates a "missing recommended runtime key" diagnostic.
fn report_missing_recommended_keys(
    severity: Severity,
    mut keys: Vec<&str>,
    runtime_span: Span,
    specification: &str,
) -> Diagnostic {
    assert!(!keys.is_empty());
    keys.sort();

    let (message, fix) = if keys.len() == 1 {
        let key = keys.first().expect("should have exactly one key");

        (
            format!("the following runtime key is recommended by {specification}: `{key}`"),
            format!("include an entry for the `{key}` key in the `runtime` section"),
        )
    } else {
        let keys = serialize_oxford_comma(
            &keys
                .iter()
                .map(|key| format!("`{key}`"))
                .collect::<Vec<_>>(),
        )
        .expect("should have keys");

        (
            format!("the following runtime keys are recommended by {specification}: {keys}"),
            format!("include entries for the {keys} keys in the `runtime` section"),
        )
    };

    diagnostic(severity, message)
        .with_rule(ID)
        .with_highlight(runtime_span)
        .with_fix(fix)
}

/// Detects missing recommended runtime keys.
#[derive(Debug, Clone)]
pub struct RecommendedRuntimeKeysRule {
    /// The severity of the rule's diagnostics.
    severity: Severity,
    /// The detected version of the current document.
    version: Option<SupportedVersion>,
    /// Whether or not we've already processed a `runtime` section within the
    /// current task.
    runtime_processed_for_task: bool,
    /// All keys encountered in the current runtime section.
    encountered_keys: Vec<String>,
}

impl RecommendedRuntimeKeysRule {
    /// Creates a new instance of the rule.
    pub fn new(config: &Config) -> Self {
        Self {
            severity: config.recommended_runtime_keys.diagnostic_severity(),
            version: None,
            runtime_processed_for_task: false,
            encountered_keys: Vec::new(),
        }
    }
}

impl Default for RecommendedRuntimeKeysRule {
    fn default() -> Self {
        Self::new(&Config::default())
    }
}

impl Rule for RecommendedRuntimeKeysRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that WDL 1.0 and 1.1 `runtime` sections include recommended keys."
    }

    fn explanation(&self) -> &'static str {
        "WDL 1.0 and 1.1 recommend runtime keys that improve task portability. In WDL 1.0, \
         `docker` and `memory` are recommended. In WDL 1.1, `container` is recommended."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.1

task missing_container_key {
    runtime {
    }
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: None,
                snippet: r#"version 1.1

task missing_container_key {
    runtime {
        container: "ubuntu:latest"
    }
}
"#,
            }),
        }]
    }

    fn tags(&self) -> TagSet {
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
        &["UnknownRuntimeKeys", "DeprecatedRuntimeKey"]
    }
}

impl Visitor for RecommendedRuntimeKeysRule {
    fn reset(&mut self) {
        self.version = None;
        self.runtime_processed_for_task = false;
        self.encountered_keys.clear();
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
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &RuntimeSection,
    ) {
        if self.runtime_processed_for_task {
            return;
        }

        if reason == VisitReason::Enter {
            return;
        }

        if let SupportedVersion::V1(minor_version) = self.version.expect("should have version") {
            let specification = format!("the WDL {minor_version} specification");
            let recommended_keys = match minor_version {
                V1::Zero => recommended_keys(keys_v1_0()),
                V1::One => recommended_keys(keys_v1_1()),
                _ => return,
            };

            let missing_keys = recommended_keys
                .filter(|(key, _)| !self.encountered_keys.iter().any(|s| s == *key))
                .map(|(key, _)| key)
                .collect::<Vec<_>>();

            if !missing_keys.is_empty() {
                diagnostics.exceptable_add(
                    report_missing_recommended_keys(
                        self.severity,
                        missing_keys,
                        section
                            .inner()
                            .first_token()
                            .expect("runtime section should have tokens")
                            .text_range()
                            .into(),
                        &specification,
                    ),
                    section.inner(),
                    &self.exceptable_nodes(),
                );
            }

            self.encountered_keys.clear();
            self.runtime_processed_for_task = true;
        }
    }

    fn runtime_item(
        &mut self,
        _diagnostics: &mut Diagnostics,
        reason: VisitReason,
        item: &RuntimeItem,
    ) {
        if self.runtime_processed_for_task || reason == VisitReason::Exit {
            return;
        }

        self.encountered_keys.push(item.name().text().to_string());
    }
}
