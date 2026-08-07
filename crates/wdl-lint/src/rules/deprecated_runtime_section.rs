//! A lint rule for deprecated `runtime` sections.

use wdl_analysis::Diagnostics;
use wdl_analysis::Document;
use wdl_analysis::Example;
use wdl_analysis::LabeledSnippet;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::version::V1;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the deprecated runtime section rule.
const ID: &str = "DeprecatedRuntimeSection";

/// Creates a "deprecated runtime section" diagnostic.
fn deprecated_runtime_section(task: &str, span: Span) -> Diagnostic {
    Diagnostic::note(format!(
        "task `{task}` contains a deprecated `runtime` section"
    ))
    .with_rule(ID)
    .with_highlight(span)
    .with_fix("replace the `runtime` section with a `requirements` section")
}

/// Detects deprecated `runtime` sections.
#[derive(Default, Debug, Clone, Copy)]
pub struct DeprecatedRuntimeSectionRule(Option<SupportedVersion>);

impl Rule for DeprecatedRuntimeSectionRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Detects deprecated `runtime` sections."
    }

    fn explanation(&self) -> &'static str {
        "The `runtime` section is deprecated in WDL v1.2 and later. Replace it with a \
         `requirements` section."
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

    runtime {
        container: "ubuntu:latest"
    }
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
        TagSet::new(&[Tag::Deprecated])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::TaskDefinitionNode,
        ])
    }

    fn related_rules(&self) -> &'static [&'static str] {
        &["RequirementsSection"]
    }
}

impl Visitor for DeprecatedRuntimeSectionRule {
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

        // This rule should only be present for WDL v1.2 or later, where the
        // `runtime` section has been deprecated in favor of `requirements`.
        if let SupportedVersion::V1(minor_version) = self.0.expect("version should exist here")
            && minor_version >= V1::Two
            && let Some(runtime) = task.runtime()
        {
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
                runtime.inner(),
                &self.exceptable_nodes(),
            );
        }
    }
}
