//! A lint rule for missing `runtime` sections.

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
use wdl_ast::v1::TaskDefinition;
use wdl_ast::version::V1;

use crate::Config;
use crate::Rule;
use crate::Tag;
use crate::TagSet;
use crate::util::diagnostic;

/// The identifier for the missing runtime rule.
const ID: &str = "RuntimeSection";

/// Creates a "missing runtime section" diagnostic.
fn missing_runtime_section(severity: Severity, task: &str, span: Span) -> Diagnostic {
    diagnostic(
        severity,
        format!("task `{task}` is missing a `runtime` section"),
    )
    .with_rule(ID)
    .with_label("this task is missing a `runtime` section", span)
    .with_fix("add a `runtime` section")
}

/// Detects missing `runtime` section for tasks.
#[derive(Debug, Clone, Copy)]
pub struct RuntimeSectionRule {
    /// The severity of the rule's diagnostics.
    severity: Severity,
    /// The WDL version of the document being linted.
    version: Option<SupportedVersion>,
}

impl RuntimeSectionRule {
    /// Creates a new instance of the rule.
    pub fn new(config: &Config) -> Self {
        Self {
            severity: config.runtime_section.diagnostic_severity(),
            version: None,
        }
    }
}

impl Default for RuntimeSectionRule {
    fn default() -> Self {
        Self::new(&Config::default())
    }
}

impl Rule for RuntimeSectionRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that tasks have a `runtime` section (for WDL v1.1 and prior)."
    }

    fn explanation(&self) -> &'static str {
        "Tasks that don't declare `runtime` sections are unlikely to be portable."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.1

task say_hello {
    input {
        String name
    }

    command <<<
        echo "Hello, ~{name}!"
    >>>
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: None,
                snippet: r#"version 1.1

task say_hello {
    input {
        String name
    }

    command <<<
        echo "Hello, ~{name}!"
    >>>

    runtime {
        container: "ubuntu:latest"
    }
}
"#,
            }),
        }]
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Completeness, Tag::Portability])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::TaskDefinitionNode,
        ])
    }

    fn related_rules(&self) -> &'static [&'static str] {
        &[
            "MetaDescription",
            "ParameterMetaMatched",
            "MetaSections",
            "OutputSection",
            "MatchingOutputMeta",
        ]
    }
}

impl Visitor for RuntimeSectionRule {
    fn reset(&mut self) {
        self.version = None;
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
        task: &TaskDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // This rule should only be present for WDL v1.1 or earlier, as the
        // `requirements` section replaces it in WDL v1.2.
        if let SupportedVersion::V1(minor_version) =
            self.version.expect("version should exist here")
            && minor_version <= V1::One
            && task.runtime().is_none()
        {
            let name = task.name();
            diagnostics.exceptable_add(
                missing_runtime_section(self.severity, name.text(), name.span()),
                task.inner(),
                &self.exceptable_nodes(),
            );
        }
    }
}
