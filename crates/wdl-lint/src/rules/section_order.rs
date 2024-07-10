//! A lint rule for section ordering.

use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::TaskItem;
use wdl_ast::v1::WorkflowDefinition;
use wdl_ast::v1::WorkflowItem;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::Span;
use wdl_ast::ToSpan;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the section ordering rule.
const ID: &str = "SectionOrdering";

/// Creates a workflow section order diagnostic.
fn workflow_section_order(span: Span, name: &str, problem_span: Span) -> Diagnostic {
    Diagnostic::note(format!("sections are not in order for workflow `{name}`"))
        .with_rule(ID)
        .with_label(
            "this workflow contains sections that are out of order",
            span,
        )
        .with_label("this section is out of order", problem_span)
        .with_fix(
            "order as `meta`, `parameter_meta`, `input`, private declarations/calls/scatters, \
             `output`",
        )
}

/// Creates a task section order diagnostic.
fn task_section_order(span: Span, name: &str, problem_span: Span) -> Diagnostic {
    Diagnostic::note(format!("sections are not in order for task `{name}`"))
        .with_rule(ID)
        .with_label("this task contains sections that are out of order", span)
        .with_label("this section is out of order", problem_span)
        .with_fix(
            "order as `meta`, `parameter_meta`, `input`, private declarations, `command`, \
             `output`, `requirements`/`runtime`",
        )
}

/// Detects section ordering issues.
#[derive(Default, Debug, Clone, Copy)]
pub struct SectionOrderingRule;

impl Rule for SectionOrderingRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that all sections are in the correct order."
    }

    fn explanation(&self) -> &'static str {
        "For workflows, the following sections must be present and in this order: meta, \
         parameter_meta, input, (body), output. \"(body)\" represents all calls and declarations.

        For tasks, the following sections must be present and in this order: meta, parameter_meta, \
         input, (private declarations), command, output, requirements/runtime"
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Style, Tag::Sorting])
    }
}

/// Track the encountered sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum State {
    /// The start state.
    Start,
    /// The meta section.
    Meta,
    /// The parameter_meta section.
    ParameterMeta,
    /// The input section.
    Input,
    /// The declaration section. Overloaded to include call and scatter
    /// statements in workflows.
    Decl,
    /// The command section.
    Command,
    /// The output section.
    Output,
    /// The requirements/runtime section.
    Requirements,
}

impl Visitor for SectionOrderingRule {
    type State = Diagnostics;

    fn document(&mut self, _: &mut Self::State, reason: VisitReason, _: &Document) {
        if reason == VisitReason::Exit {
            return;
        }

        // Reset the visitor upon document entry
        *self = Default::default();
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

        let mut encountered = State::Start;
        for item in task.items() {
            match item {
                TaskItem::Metadata(_) if encountered <= State::Meta => {
                    encountered = State::Meta;
                }
                TaskItem::ParameterMetadata(_) if encountered <= State::ParameterMeta => {
                    encountered = State::ParameterMeta;
                }
                TaskItem::Input(_) if encountered <= State::Input => {
                    encountered = State::Input;
                }
                TaskItem::Declaration(_) if encountered <= State::Decl => {
                    encountered = State::Decl;
                }
                TaskItem::Command(_) if encountered <= State::Command => {
                    encountered = State::Command;
                }
                TaskItem::Output(_) if encountered <= State::Output => {
                    encountered = State::Output;
                }
                TaskItem::Requirements(_) | TaskItem::Runtime(_)
                    if encountered <= State::Requirements =>
                {
                    encountered = State::Requirements;
                }
                _ => {
                    state.add(task_section_order(
                        task.name().span(),
                        task.name().as_str(),
                        item.syntax().first_token().unwrap().text_range().to_span(),
                    ));
                    break;
                }
            }
        }
    }

    fn workflow_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        workflow: &WorkflowDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let mut encountered = State::Start;
        for item in workflow.items() {
            match item {
                WorkflowItem::Metadata(_) if encountered <= State::Meta => {
                    encountered = State::Meta;
                }
                WorkflowItem::ParameterMetadata(_) if encountered <= State::ParameterMeta => {
                    encountered = State::ParameterMeta;
                }
                WorkflowItem::Input(_) if encountered <= State::Input => {
                    encountered = State::Input;
                }
                WorkflowItem::Declaration(_) | WorkflowItem::Call(_) | WorkflowItem::Scatter(_)
                    if encountered <= State::Decl =>
                {
                    encountered = State::Decl;
                }
                WorkflowItem::Output(_) if encountered <= State::Output => {
                    encountered = State::Output;
                }
                _ => {
                    state.add(workflow_section_order(
                        workflow.name().span(),
                        workflow.name().as_str(),
                        item.syntax().first_token().unwrap().text_range().to_span(),
                    ));
                    break;
                }
            }
        }
    }
}
