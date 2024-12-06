//! Ensures that the value for `container` keys in `runtime`/`requirements`
//! sections are well-formed.
//!
//! This check only occurs if the `container` key exists in the
//! `runtime`/`requirements` sections.

use wdl_ast::AstNode;
use wdl_ast::AstNodeExt;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;
use wdl_ast::v1::RequirementsSection;
use wdl_ast::v1::RuntimeSection;
use wdl_ast::v1::common::container::Kind;
use wdl_ast::v1::common::container::value::Value;
use wdl_ast::v1::common::container::value::uri::ANY_CONTAINER_VALUE;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the container value rule.
const ID: &str = "ContainerValue";

/// Ensures that values for `container` keys within `runtime`/`requirements`
/// sections are well-formed.
#[derive(Default, Debug, Clone, Copy)]
pub struct ContainerValue;

/// Creates a missing tag diagnostic.
fn missing_tag(span: Span) -> Diagnostic {
    Diagnostic::warning(String::from("container URI is missing a tag"))
        .with_rule(ID)
        .with_highlight(span)
        .with_fix(
            "add a tag to the container URI (e.g., `ubuntu@sha256:foobar` instead of `ubuntu`)",
        )
}

/// Creates a mutable tag diagnostic.
fn mutable_tag(span: Span) -> Diagnostic {
    Diagnostic::note(String::from("container URI uses a mutable tag"))
        .with_rule(ID)
        .with_highlight(span)
        .with_fix(
            "replace the mutable tag with its SHA256 equivalent (e.g., `ubuntu@sha256:foobar` \
             instead of `ubuntu:latest`)",
        )
}

/// Creates an "empty array" diagnostic.
fn empty_array(span: Span) -> Diagnostic {
    Diagnostic::warning(String::from(
        "empty arrays are ambiguous and should contain at least one entry",
    ))
    .with_rule(ID)
    .with_highlight(span)
    .with_fix("add an entry or remove the entry altogether")
}

/// Creates a diagnostic indicating that a single value array should instead be
/// a string literal.
fn array_to_string_literal(span: Span) -> Diagnostic {
    Diagnostic::note(String::from(
        "an array with a single value should be a string literal",
    ))
    .with_rule(ID)
    .with_highlight(span)
    .with_fix("change the array to a string literal representing the first value")
}

/// Creates a diagnostic indicating that an array contains one or more 'any'
/// URIs.
fn array_containing_anys(spans: impl Iterator<Item = Span>) -> Diagnostic {
    let mut diagnostic = Diagnostic::warning(format!(
        "container arrays containing `{ANY_CONTAINER_VALUE}` are ambiguous"
    ))
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

impl Rule for ContainerValue {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that values for the `container` key within `runtime`/`requirements` sections are \
         well-formed."
    }

    fn explanation(&self) -> &'static str {
        "This rule checks the following:

        - Containers should have a tag, as container URIs with no tags have no expectation that \
         the behavior of the containers won't change between runs.
        - Further, immutable containers tagged with SHA256 sums are preferred. This is due to the \
         requirement from the WDL specification that tasks produce functionally equivalent output \
         across runs. When a mutable tag is used, there is a risk that changes to the container \
         will cause different behavior between runs.
        - Use of the 'any' container URI (`*`) within an array of container URIs is ambiguous and \
         should be avoided.
        - Empty container URI arrays are not disallowed by the specification but are ambiguous and \
         should be avoided.
        - An array of container URIs with a single element should be changed to a single string \
         value."
    }

    fn tags(&self) -> TagSet {
        // NOTE: these are the justification for these tags:
        //
        // - Clarity because it resolves the ambiguous situations described in the
        //   explanation above.
        // - Portability because this resolves situations where different execution
        //   engines might behave differently (e.g., one container engine might always
        //   pull the latest image for a mutably tagged container whereas another may
        //   use a older, cached version until the user prompts it to upgrade).
        TagSet::new(&[Tag::Clarity, Tag::Portability])
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::RuntimeSectionNode,
            SyntaxKind::RequirementsSectionNode,
        ])
    }
}

impl Visitor for ContainerValue {
    type State = Diagnostics;

    fn document(&mut self, _: &mut Self::State, _: VisitReason, _: &Document, _: SupportedVersion) {
        // This callback is intentionally empty.
    }

    fn runtime_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &RuntimeSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Some(container) = section.container() {
            if let Ok(value) = container.value() {
                check_container_value(
                    state,
                    value,
                    SyntaxElement::from(section.syntax().clone()),
                    &self.exceptable_nodes(),
                );
            }
        }
    }

    fn requirements_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &RequirementsSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Some(container) = section.container() {
            if let Ok(value) = container.value() {
                check_container_value(
                    state,
                    value,
                    SyntaxElement::from(section.syntax().clone()),
                    &self.exceptable_nodes(),
                );
            }
        }
    }
}

/// Examines the value of the `container` item in both the `runtime` and
/// `requirements` sections.
fn check_container_value(
    state: &mut Diagnostics,
    value: Value,
    syntax: SyntaxElement,
    exceptable_nodes: &Option<&'static [SyntaxKind]>,
) {
    if let Kind::Array(array) = value.kind() {
        if array.is_empty() {
            state.exceptable_add(
                empty_array(value.expr().span()),
                syntax.clone(),
                exceptable_nodes,
            );
        } else if array.len() == 1 {
            // SAFETY: we just checked to ensure that exactly one element exists in the
            // vec, so this will always unwrap.
            let uri = array.iter().next().unwrap();
            state.exceptable_add(
                array_to_string_literal(uri.literal_string().span()),
                syntax.clone(),
                exceptable_nodes,
            );
        } else {
            let mut anys = array.iter().filter(|uri| uri.kind().is_any()).peekable();

            if anys.peek().is_some() {
                state.exceptable_add(
                    array_containing_anys(anys.map(|any| any.literal_string().span())),
                    syntax.clone(),
                    exceptable_nodes,
                );
            }
        }
    }

    for uri in value.uris() {
        if let Some(entry) = uri.kind().as_entry() {
            if entry.tag().is_none() {
                state.exceptable_add(
                    missing_tag(uri.literal_string().span()),
                    syntax.clone(),
                    exceptable_nodes,
                );
            } else if !entry.immutable() {
                state.exceptable_add(
                    mutable_tag(uri.literal_string().span()),
                    syntax.clone(),
                    exceptable_nodes,
                );
            }
        }
    }
}
