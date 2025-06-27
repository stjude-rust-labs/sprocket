//! A lint rule for missing output sections.

use std::fmt;

use wdl_analysis::Diagnostics;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::WorkflowDefinition;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The context for where the output is missing.
enum Context {
    /// A task.
    Task,
    /// A workflow.
    Workflow,
}

impl fmt::Display for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Task => write!(f, "task"),
            Self::Workflow => write!(f, "workflow"),
        }
    }
}

/// The identifier for the missing output rule.
const ID: &str = "OutputSection";

/// Creates a "missing output section" diagnostic.
fn missing_output_section(name: &str, context: Context, span: Span) -> Diagnostic {
    Diagnostic::warning(format!("{context} `{name}` is missing an output section"))
        .with_rule(ID)
        .with_label(format!("this {context} is missing an output section"), span)
        .with_fix(format!(
            "add an output section to the {context} to enable call-caching",
        ))
}

/// Detects missing `output` section for tasks and workflows.
#[derive(Default, Debug, Clone, Copy)]
pub struct OutputSectionRule;

impl Rule for OutputSectionRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that tasks and workflows have an `output` section."
    }

    fn explanation(&self) -> &'static str {
        "Some execution engines require an output be defined in order to enable call-caching. When \
         an output is not the result of a successful execution, it is recommended to define a \
         \"dummy\" output to enable call-caching. An example may be `String check = \"passed\"`."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Completeness, Tag::Portability])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::TaskDefinitionNode,
            SyntaxKind::WorkflowDefinitionNode,
        ])
    }

    fn related_rules(&self) -> &[&'static str] {
        &[
            "MetaDescription",
            "ParameterMetaMatched",
            "RequirementsSection",
            "RuntimeSection",
            "MatchingOutputMeta",
        ]
    }
}

impl Visitor for OutputSectionRule {
    fn reset(&mut self) {
        *self = Self;
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

        if task.output().is_none() {
            let name = task.name();
            diagnostics.exceptable_add(
                missing_output_section(name.text(), Context::Task, name.span()),
                SyntaxElement::from(task.inner().clone()),
                &self.exceptable_nodes(),
            );
        }
    }

    fn workflow_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        workflow: &WorkflowDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if workflow.output().is_none() {
            let name = workflow.name();
            diagnostics.exceptable_add(
                missing_output_section(name.text(), Context::Workflow, name.span()),
                SyntaxElement::from(workflow.inner().clone()),
                &self.exceptable_nodes(),
            );
        }
    }
}
