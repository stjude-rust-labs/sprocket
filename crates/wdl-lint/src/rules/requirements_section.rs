//! A lint rule for missing `requirements` sections.

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

/// The identifier for the missing requirements rule.
const ID: &str = "RequirementsSection";

/// Creates a "missing requirements section" diagnostic.
fn missing_requirements_section(severity: Severity, task: &str, span: Span) -> Diagnostic {
    diagnostic(
        severity,
        format!("task `{task}` is missing a `requirements` section"),
    )
    .with_rule(ID)
    .with_label("this task is missing a `requirements` section", span)
    .with_fix("add a `requirements` section")
}

/// Detects missing `requirements` section for tasks.
#[derive(Debug, Clone, Copy)]
pub struct RequirementsSectionRule {
    /// The severity of the rule's diagnostics.
    severity: Severity,
    /// The WDL version of the document being linted.
    version: Option<SupportedVersion>,
}

impl RequirementsSectionRule {
    /// Creates a new instance of the rule.
    pub fn new(config: &Config) -> Self {
        Self {
            severity: config.requirements_section.diagnostic_severity(),
            version: None,
        }
    }
}

impl Default for RequirementsSectionRule {
    fn default() -> Self {
        Self::new(&Config::default())
    }
}

impl Rule for RequirementsSectionRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that tasks have a `requirements` section (for WDL v1.2 and beyond)."
    }

    fn explanation(&self) -> &'static str {
        "Tasks that don't declare `requirements` sections are unlikely to be portable."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.2

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
                snippet: r#"version 1.2

task say_hello {
    input {
        String name
    }

    command <<<
        echo "Hello, ~{name}!"
    >>>

    requirements {
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
            "DeprecatedRuntimeSection",
            "UnknownRuntimeKeys",
            "MetaDescription",
            "MissingParameterMeta",
            "MetaSections",
            "OutputSection",
            "MatchingOutputMeta",
        ]
    }
}

impl Visitor for RequirementsSectionRule {
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

        // This rule should only be present for WDL v1.2 or later. Prior to that
        // version, the `runtime` section was recommended.
        if let SupportedVersion::V1(minor_version) =
            self.version.expect("version should exist here")
            && minor_version >= V1::Two
            && task.requirements().is_none()
            && task.runtime().is_none()
        {
            let name = task.name();
            diagnostics.exceptable_add(
                missing_requirements_section(self.severity, name.text(), name.span()),
                task.inner(),
                &self.exceptable_nodes(),
            );
        }
    }
}
