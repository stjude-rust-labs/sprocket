//! A lint rule for deprecated `runtime` keys.

use wdl_analysis::Diagnostics;
use wdl_analysis::Document;
use wdl_analysis::Example;
use wdl_analysis::LabeledSnippet;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Ident;
use wdl_ast::Severity;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::RuntimeItem;
use wdl_ast::v1::RuntimeSection;
use wdl_ast::version::V1;

use crate::Config;
use crate::Rule;
use crate::Tag;
use crate::TagSet;
use crate::rules::KeyKind;
use crate::rules::keys_v1_1;

/// The identifier for the deprecated runtime key rule.
const ID: &str = "DeprecatedRuntimeKey";

/// Creates a "deprecated runtime key" diagnostic.
fn deprecated_runtime_key(severity: Severity, key: &Ident, replacement: &str) -> Diagnostic {
    Diagnostic::new(
        severity,
        format!(
            "the `{key}` runtime key has been deprecated in favor of `{replacement}`",
            key = key.text()
        ),
    )
    .with_rule(ID)
    .with_highlight(key.span())
    .with_fix(format!(
        "replace the `{key}` key with `{replacement}`",
        key = key.text()
    ))
}

/// Detects deprecated runtime keys.
#[derive(Debug, Clone, Copy)]
pub struct DeprecatedRuntimeKeyRule {
    /// The severity of the rule's diagnostics.
    severity: Severity,
    /// The detected version of the current document.
    version: Option<SupportedVersion>,
    /// Whether or not we've already processed a `runtime` section within the
    /// current task.
    runtime_processed_for_task: bool,
}

impl DeprecatedRuntimeKeyRule {
    /// Creates a new instance of the rule.
    pub fn new(config: &Config) -> Self {
        Self {
            severity: config.deprecated_runtime_key.diagnostic_severity(),
            version: None,
            runtime_processed_for_task: false,
        }
    }
}

impl Rule for DeprecatedRuntimeKeyRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Detects WDL 1.1 `runtime` keys that have preferred replacements."
    }

    fn explanation(&self) -> &'static str {
        "Some WDL 1.1 `runtime` keys are deprecated in favor of renamed keys. Using the current \
         key names makes runtime requirements clearer and easier for execution engines to \
         interpret."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.1

task old_container_key {
    runtime {
        docker: "ubuntu:latest"
    }
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: None,
                snippet: r#"version 1.1

task old_container_key {
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
        &["UnknownRuntimeKeys", "RecommendedRuntimeKeys"]
    }
}

impl Visitor for DeprecatedRuntimeKeyRule {
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
        if self.runtime_processed_for_task || reason == VisitReason::Exit {
            return;
        }

        if let SupportedVersion::V1(V1::One) = self.version.expect("should have version") {
            let key_name = item.name();
            if let Some(KeyKind::Deprecated(replacement)) = keys_v1_1().get(key_name.text()) {
                diagnostics.exceptable_add(
                    deprecated_runtime_key(self.severity, &key_name, replacement),
                    item.inner(),
                    &self.exceptable_nodes(),
                );
            }
        }
    }
}
