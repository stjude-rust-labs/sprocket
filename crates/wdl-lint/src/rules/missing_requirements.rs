//! A lint rule for missing `requirements` sections.

use wdl_ast::v1::TaskDefinition;
use wdl_ast::version::V1;
use wdl_ast::AstNode;
use wdl_ast::AstNodeExt;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the missing requirements rule.
const ID: &str = "MissingRequirements";

/// Creates a "deprecated runtime section" diagnostic.
fn deprecated_runtime_section(task: &str, span: Span) -> Diagnostic {
    Diagnostic::warning(format!(
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
        .with_fix("add a `requirements` section to the task")
}

/// Detects missing `requirements` section for tasks.
#[derive(Default, Debug, Clone, Copy)]
pub struct MissingRequirementsRule(Option<SupportedVersion>);

impl Rule for MissingRequirementsRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that tasks have a `requirements` section (for WDL v1.2 and beyond)."
    }

    fn explanation(&self) -> &'static str {
        "Tasks that don't declare `requirements` sections are unlikely to be portable.

        For tasks that _should_ contain a `requirements` section but a `runtime` section exists \
         instead, the `runtime` section is flagged as deprecated."
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
}

impl Visitor for MissingRequirementsRule {
    type State = Diagnostics;

    fn document(
        &mut self,
        _: &mut Self::State,
        reason: VisitReason,
        _: &Document,
        version: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Reset the visitor upon document entry
        *self = Self(Some(version));
    }

    fn task_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        task: &TaskDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // This rule should only be present for WDL v1.2 or later. Prior to that
        // version, the `runtime` section was recommended.
        if let SupportedVersion::V1(minor_version) = self.0.expect("version should exist here") {
            if minor_version >= V1::Two {
                if task.requirements().is_none() {
                    let name = task.name();
                    state.exceptable_add(
                        missing_requirements_section(name.as_str(), name.span()),
                        SyntaxElement::from(task.syntax().clone()),
                        &self.exceptable_nodes(),
                    );
                }

                if let Some(runtime) = task.runtime() {
                    let name = task.name();
                    state.exceptable_add(
                        deprecated_runtime_section(name.as_str(), runtime.span()),
                        SyntaxElement::from(runtime.syntax().clone()),
                        &self.exceptable_nodes(),
                    );
                }
            }
        }
    }
}
