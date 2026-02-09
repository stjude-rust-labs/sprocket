//! A lint rule for missing `requirements` sections.

use wdl_analysis::Diagnostics;
use wdl_analysis::Document;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::version::V1;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the missing requirements rule.
const ID: &str = "RequirementsSection";

/// Creates a "deprecated runtime section" diagnostic.
fn deprecated_runtime_section(task: &str, span: Span) -> Diagnostic {
    Diagnostic::note(format!(
        "task `{task}` contains a deprecated `runtime` section"
    ))
    .with_rule(ID)
    .with_highlight(span)
    .with_fix("replace the `runtime` section with a `requirements` section")
}

/// Creates a "missing requirements section" diagnostic.
fn missing_requirements_section(task: &str, span: Span) -> Diagnostic {
    Diagnostic::warning(format!("task `{task}` is missing a `requirements` section"))
        .with_rule(ID)
        .with_label("this task is missing a `requirements` section", span)
        .with_fix("add a `requirements` section")
}

/// Detects missing `requirements` section for tasks.
#[derive(Default, Debug, Clone, Copy)]
pub struct RequirementsSectionRule(Option<SupportedVersion>);

impl Rule for RequirementsSectionRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn version(&self) -> &'static str {
        "0.5.0"
    }

    fn description(&self) -> &'static str {
        "Ensures that tasks have a `requirements` section (for WDL v1.2 and beyond)."
    }

    fn explanation(&self) -> &'static str {
        "Tasks that don't declare `requirements` sections are unlikely to be portable.

For tasks that _should_ contain a `requirements` section but a `runtime` section exists instead, \
         the `runtime` section is flagged as deprecated."
    }

    fn examples(&self) -> &'static [&'static str] {
        &[
            r#"```wdl
version 1.2

task say_hello {
    input {
        String name
    }

    command <<<
        echo "Hello, ~{name}!"
    >>>
}
```"#,
            r#"Use instead:

```wdl
version 1.2

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
```"#,
        ]
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Completeness, Tag::Portability, Tag::Deprecated])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::TaskDefinitionNode,
        ])
    }

    fn related_rules(&self) -> &[&'static str] {
        &[
            "ExpectedRuntimeKeys",
            "MetaDescription",
            "ParameterMetaMatched",
            "MetaSections",
            "OutputSection",
            "MatchingOutputMeta",
        ]
    }
}

impl Visitor for RequirementsSectionRule {
    fn reset(&mut self) {
        *self = Self::default();
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

        self.0 = Some(version);
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
        if let SupportedVersion::V1(minor_version) = self.0.expect("version should exist here")
            && minor_version >= V1::Two
        {
            match task.runtime() {
                Some(runtime) => {
                    let name = task.name();
                    diagnostics.exceptable_add(
                        deprecated_runtime_section(
                            name.text(),
                            runtime
                                .inner()
                                .first_token()
                                .expect("runtime section should have tokens")
                                .text_range()
                                .into(),
                        ),
                        SyntaxElement::from(runtime.inner().clone()),
                        &self.exceptable_nodes(),
                    );
                }
                _ => {
                    if task.requirements().is_none() {
                        let name = task.name();
                        diagnostics.exceptable_add(
                            missing_requirements_section(name.text(), name.span()),
                            SyntaxElement::from(task.inner().clone()),
                            &self.exceptable_nodes(),
                        );
                    }
                }
            }
        }
    }
}
