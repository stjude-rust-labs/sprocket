//! A lint rule for redundant container URI arrays.

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
use wdl_ast::v1::common::container::Kind;
use wdl_ast::v1::common::container::value::Value;

use crate::Config;
use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the redundant container array rule.
const ID: &str = "RedundantContainerArray";

/// Creates a diagnostic indicating that a single value array should instead be
/// a string literal.
fn array_to_string_literal(severity: Severity, span: Span) -> Diagnostic {
    Diagnostic::new(
        severity,
        String::from("an array with a single value should be a string literal"),
    )
    .with_rule(ID)
    .with_highlight(span)
    .with_fix("change the array to a string literal representing the first value")
}

/// Detects single-item container arrays.
#[derive(Debug, Clone, Copy)]
pub struct RedundantContainerArrayRule {
    /// The severity of the rule's diagnostics.
    severity: Severity,
}

impl RedundantContainerArrayRule {
    /// Creates a new instance of the rule.
    pub fn new(config: &Config) -> Self {
        Self {
            severity: config.redundant_container_array.diagnostic_severity(),
        }
    }
}

impl Rule for RedundantContainerArrayRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that single-item container arrays are written as string literals."
    }

    fn explanation(&self) -> &'static str {
        "An array of container URIs with a single element is redundant and should be changed to a \
         single string value."
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
        container: [
            "ubuntu@sha256:cc925e589b7543b910fea57a240468940003fbfc0515245a495dd0ad8fe7cef1",
        ]
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

impl Visitor for RedundantContainerArrayRule {
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
    if let Kind::Array(array) = value.kind()
        && array.len() == 1
    {
        let uri = array.iter().next().expect("array should have one element");
        diagnostics.exceptable_add(
            array_to_string_literal(severity, uri.literal_string().span()),
            node,
            exceptable_nodes,
        );
    }
}
