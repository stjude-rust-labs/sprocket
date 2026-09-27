//! A lint rule for mutable container tags.

use wdl_analysis::Diagnostics;
use wdl_analysis::Example;
use wdl_analysis::LabeledSnippet;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Severity;
use wdl_ast::Span;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_ast::v1::RequirementsSection;
use wdl_ast::v1::RuntimeSection;
use wdl_ast::v1::common::container::value::Value;

use crate::Config;
use crate::Rule;
use crate::Tag;
use crate::TagSet;
use crate::util::diagnostic;

/// The identifier for the mutable container tag rule.
const ID: &str = "MutableContainerTag";

/// Creates a mutable tag diagnostic.
fn mutable_tag(severity: Severity, span: Span) -> Diagnostic {
    diagnostic(severity, String::from("container URI uses a mutable tag"))
        .with_rule(ID)
        .with_highlight(span)
        .with_fix(
            "replace the mutable tag with its SHA256 equivalent (e.g., `ubuntu@sha256:foobar` \
             instead of `ubuntu:latest`)",
        )
}

/// Detects mutable container tags.
#[derive(Debug, Clone, Copy)]
pub struct MutableContainerTagRule {
    /// The severity of the rule's diagnostics.
    severity: Severity,
}

impl MutableContainerTagRule {
    /// Creates a new instance of the rule.
    pub fn new(config: &Config) -> Self {
        Self {
            severity: config.mutable_container_tag.diagnostic_severity(),
        }
    }
}

impl Default for MutableContainerTagRule {
    fn default() -> Self {
        Self::new(&Config::default())
    }
}

impl Rule for MutableContainerTagRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that container URIs use immutable tags."
    }

    fn explanation(&self) -> &'static str {
        "Immutable containers tagged with SHA256 sums are preferred. This is due to the \
         requirement from the WDL specification that tasks produce functionally equivalent output \
         across runs. When a mutable tag is used, there is a risk that changes to the container \
         will cause different behavior between runs."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.3

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
            },
            revised: Some(LabeledSnippet {
                label: None,
                snippet: r#"version 1.3

task say_hello {
    input {
        String name
    }

    command <<<
        echo "Hello, ~{name}!"
    >>>

    requirements {
        container: "ubuntu@sha256:cc925e589b7543b910fea57a240468940003fbfc0515245a495dd0ad8fe7cef1"
    }
}
"#,
            }),
        }]
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Clarity, Tag::Portability])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::RuntimeSectionNode,
            SyntaxKind::RequirementsSectionNode,
            SyntaxKind::RequirementsItemNode,
            SyntaxKind::RuntimeItemNode,
        ])
    }

    fn related_rules(&self) -> &'static [&'static str] {
        &["ContainerUri"]
    }
}

impl Visitor for MutableContainerTagRule {
    fn reset(&mut self) {}

    fn runtime_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &RuntimeSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Some(container) = section.container()
            && let Ok(value) = container.value()
        {
            check_container_value(
                self.severity,
                diagnostics,
                value,
                container.inner(),
                &self.exceptable_nodes(),
            );
        }
    }

    fn requirements_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &RequirementsSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Some(container) = section.container()
            && let Ok(value) = container.value()
        {
            check_container_value(
                self.severity,
                diagnostics,
                value,
                container.inner(),
                &self.exceptable_nodes(),
            );
        }
    }
}

/// Examines the value of the `container` item.
fn check_container_value(
    severity: Severity,
    diagnostics: &mut Diagnostics,
    value: Value,
    node: &SyntaxNode,
    exceptable_nodes: &Option<&'static [SyntaxKind]>,
) {
    for uri in value.uris() {
        if let Some(entry) = uri.kind().as_entry()
            && entry.tag().is_some()
            && !entry.immutable()
        {
            diagnostics.exceptable_add(
                mutable_tag(severity, uri.literal_string().span()),
                node,
                exceptable_nodes,
            );
        }
    }
}
