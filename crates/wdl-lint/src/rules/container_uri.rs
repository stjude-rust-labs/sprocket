//! Ensures that the value for `container` keys in `runtime`/`requirements`
//! sections are well-formed.
//!
//! This check only occurs if the `container` key exists in the
//! `runtime`/`requirements` sections.

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
use wdl_ast::v1::common::container::value::uri::ANY_CONTAINER_VALUE;

use crate::Config;
use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the container value rule.
const ID: &str = "ContainerUri";

/// Ensures that values for `container` keys within `runtime`/`requirements`
/// sections are well-formed.
#[derive(Debug, Clone, Copy)]
pub struct ContainerUriRule {
    /// The severity of the rule's diagnostics.
    severity: Severity,
}

/// Creates a missing tag diagnostic.
fn missing_tag(severity: Severity, span: Span) -> Diagnostic {
    Diagnostic::new(severity, String::from("container URI is missing a tag"))
        .with_rule(ID)
        .with_highlight(span)
        .with_fix(
            "add a tag to the container URI (e.g., `ubuntu@sha256:foobar` instead of `ubuntu`)",
        )
}

/// Creates an "empty array" diagnostic.
fn empty_array(severity: Severity, span: Span) -> Diagnostic {
    Diagnostic::new(
        severity,
        String::from("empty arrays are ambiguous and should contain at least one entry"),
    )
    .with_rule(ID)
    .with_highlight(span)
    .with_fix("add an entry or remove the entry altogether")
}

/// Creates a diagnostic indicating that an array contains one or more 'any'
/// URIs.
fn array_containing_anys(severity: Severity, spans: impl Iterator<Item = Span>) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        severity,
        format!("container arrays containing `{ANY_CONTAINER_VALUE}` are ambiguous"),
    )
    .with_rule(ID)
    .with_fix(format!(
        "remove these entries or change the array to a string literal with the value of \
         `{ANY_CONTAINER_VALUE}`"
    ));

    for span in spans {
        diagnostic = diagnostic.with_highlight(span)
    }

    diagnostic
}

impl ContainerUriRule {
    /// Creates a new instance of the rule.
    pub fn new(config: &Config) -> Self {
        Self {
            severity: config.container_uri.diagnostic_severity(),
        }
    }
}

impl Rule for ContainerUriRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that values for the `container` key within `runtime`/`requirements` sections are \
         well-formed."
    }

    fn explanation(&self) -> &'static str {
        "This rule checks the following:

- Containers should have a tag, as container URIs with no tags have no expectation that the \
         behavior of the containers won't change between runs.
- Use of the 'any' container URI (`*`) within an array of container URIs is ambiguous and should \
         be avoided.
- Empty container URI arrays are not disallowed by the specification but are ambiguous and should \
         be avoided."
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

    # No tag
    requirements {
        container: "ubuntu"
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
        container: "ubuntu@sha256:cc925e589b7543b910fea57a240468940003fbfc0515245a495dd0ad8fe7cef1"
    }
}
"#,
            }),
        }]
    }

    fn tags(&self) -> TagSet {
        // NOTE: these are the justification for these tags:
        //
        // - Clarity because it resolves the ambiguous situations described in
        //   the explanation above.
        // - Portability because this resolves situations where different
        //   execution engines might behave differently for untagged or
        //   ambiguous container values.
        TagSet::new(&[Tag::Clarity, Tag::Portability])
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::RuntimeSectionNode,
            SyntaxKind::RequirementsSectionNode,
            SyntaxKind::RequirementsItemNode,
            SyntaxKind::RuntimeItemNode,
        ])
    }

    fn related_rules(&self) -> &'static [&'static str] {
        &["MutableContainerTag", "RedundantContainerArray"]
    }
}

impl Visitor for ContainerUriRule {
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

/// Examines the value of the `container` item in both the `runtime` and
/// `requirements` sections.
fn check_container_value(
    severity: Severity,
    diagnostics: &mut Diagnostics,
    value: Value,
    node: &SyntaxNode,
    exceptable_nodes: &Option<&'static [SyntaxKind]>,
) {
    if let Kind::Array(array) = value.kind() {
        if array.is_empty() {
            diagnostics.exceptable_add(
                empty_array(severity, value.expr().span()),
                node,
                exceptable_nodes,
            );
        } else if array.len() > 1 {
            let mut anys = array.iter().filter(|uri| uri.kind().is_any()).peekable();

            if anys.peek().is_some() {
                diagnostics.exceptable_add(
                    array_containing_anys(severity, anys.map(|any| any.literal_string().span())),
                    node,
                    exceptable_nodes,
                );
            }
        }
    }

    for uri in value.uris() {
        if let Some(entry) = uri.kind().as_entry()
            && entry.tag().is_none()
        {
            diagnostics.exceptable_add(
                missing_tag(severity, uri.literal_string().span()),
                node,
                exceptable_nodes,
            );
        }
    }
}
