//! A lint rule for missing `runtime` sections.

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

/// The identifier for the missing runtime rule.
const ID: &str = "RuntimeSection";

/// Creates a "missing runtime section" diagnostic.
fn missing_runtime_section(task: &str, span: Span) -> Diagnostic {
    Diagnostic::warning(format!("task `{task}` is missing a `runtime` section"))
        .with_rule(ID)
        .with_label("this task is missing a `runtime` section", span)
        .with_fix("add a `runtime` section")
}

/// Detects missing `runtime` section for tasks.
#[derive(Default, Debug, Clone, Copy)]
pub struct RuntimeSectionRule(Option<SupportedVersion>);

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

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Completeness, Tag::Portability])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::TaskDefinitionNode,
        ])
    }

    fn related_rules(&self) -> &[&'static str] {
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

        // This rule should only be present for WDL v1.1 or earlier, as the
        // `requirements` section replaces it in WDL v1.2.
        if let SupportedVersion::V1(minor_version) = self.0.expect("version should exist here")
            && minor_version <= V1::One
            && task.runtime().is_none()
        {
            let name = task.name();
            diagnostics.exceptable_add(
                missing_runtime_section(name.text(), name.span()),
                SyntaxElement::from(task.inner().clone()),
                &self.exceptable_nodes(),
            );
        }
    }
}
