//! A lint rule for section ordering.

use wdl_analysis::Diagnostics;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::StructDefinition;
use wdl_ast::v1::StructItem;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::TaskItem;
use wdl_ast::v1::WorkflowDefinition;
use wdl_ast::v1::WorkflowItem;

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

/// Create a struct section order diagnostic.
fn struct_section_order(span: Span, name: &str, problem_span: Span) -> Diagnostic {
    Diagnostic::note(format!("sections are not in order for struct `{name}`"))
        .with_rule(ID)
        .with_label("this struct contains sections that are out of order", span)
        .with_label("this section is out of order", problem_span)
        .with_fix("order as `meta`, `parameter_meta`, members")
}

/// Detects section ordering issues.
#[derive(Default, Debug, Clone, Copy)]
pub struct SectionOrderingRule;

impl Rule for SectionOrderingRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn version(&self) -> &'static str {
        "0.4.0"
    }

    fn description(&self) -> &'static str {
        "Ensures that all sections are in the correct order."
    }

    fn explanation(&self) -> &'static str {
        "For workflows, if present, the following sections must be in this order: meta, \
         parameter_meta, input, (body), output. \"(body)\" represents all calls and declarations.

For tasks, if present, the following sections must be in this order: meta, parameter_meta, input, \
         (private declarations), command, output, runtime, requirements, hints.

For structs, if present, the following sections must be in this order: meta, parameter_meta, \
         members."
    }

    fn examples(&self) -> &'static [&'static str] {
        &[
            r#"```wdl
version 1.2

workflow hello {
    input {
        String name
    }

    meta {
        description: "Says hello"
    }

    parameter_meta {
        name: "The name of the target"
    }

    call say_hello {
        name
    }

    output {}
}

task say_hello {
    command <<<
        echo "Hello, ~{name}!"
    >>>

    input {
        String name
    }
}
```"#,
            r#"Use instead:

```wdl
version 1.2

workflow hello {
    meta {
        description: "Says hello"
    }

    parameter_meta {
        name: "The name of the target"
    }

    input {
        String name
    }

    call say_hello {
        name
    }

    output {}
}

task say_hello {
    input {
        String name
    }

    command <<<
        echo "Hello, ~{name}!"
    >>>
}
```"#,
        ]
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Style, Tag::Sorting])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::TaskDefinitionNode,
            SyntaxKind::WorkflowDefinitionNode,
            SyntaxKind::StructDefinitionNode,
        ])
    }

    fn related_rules(&self) -> &[&'static str] {
        &[]
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
    /// The declaration section. Overloaded to include call, scatter,
    /// and conditional statements in workflows.
    Decl,
    /// The command section.
    Command,
    /// The output section.
    Output,
    /// The runtime section (only in tasks).
    Runtime,
    /// The requirements section (only in tasks).
    Requirements,
    /// The hints section.
    Hints,
}

impl Visitor for SectionOrderingRule {
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
                TaskItem::Runtime(_) if encountered <= State::Runtime => {
                    encountered = State::Runtime;
                }
                TaskItem::Requirements(_) if encountered <= State::Requirements => {
                    encountered = State::Requirements;
                }
                TaskItem::Hints(_) if encountered <= State::Hints => {
                    encountered = State::Hints;
                }
                _ => {
                    diagnostics.exceptable_add(
                        task_section_order(
                            task.name().span(),
                            task.name().text(),
                            item.inner()
                                .first_token()
                                .expect("task item should have tokens")
                                .text_range()
                                .into(),
                        ),
                        SyntaxElement::from(task.inner().clone()),
                        &self.exceptable_nodes(),
                    );
                    break;
                }
            }
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
                WorkflowItem::Declaration(_)
                | WorkflowItem::Call(_)
                | WorkflowItem::Scatter(_)
                | WorkflowItem::Conditional(_)
                    if encountered <= State::Decl =>
                {
                    encountered = State::Decl;
                }
                WorkflowItem::Output(_) if encountered <= State::Output => {
                    encountered = State::Output;
                }
                WorkflowItem::Hints(_) if encountered <= State::Hints => {
                    encountered = State::Hints;
                }
                _ => {
                    diagnostics.exceptable_add(
                        workflow_section_order(
                            workflow.name().span(),
                            workflow.name().text(),
                            item.inner()
                                .first_token()
                                .expect("workflow item should have tokens")
                                .text_range()
                                .into(),
                        ),
                        SyntaxElement::from(workflow.inner().clone()),
                        &self.exceptable_nodes(),
                    );
                    break;
                }
            }
        }
    }

    fn struct_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        struct_def: &StructDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let mut encountered = State::Start;
        for item in struct_def.items() {
            match item {
                StructItem::Metadata(_) if encountered <= State::Meta => {
                    encountered = State::Meta;
                }
                StructItem::ParameterMetadata(_) if encountered <= State::ParameterMeta => {
                    encountered = State::ParameterMeta;
                }
                StructItem::Member(_) if encountered <= State::Decl => {
                    encountered = State::Decl;
                }
                _ => {
                    diagnostics.exceptable_add(
                        struct_section_order(
                            struct_def.name().span(),
                            struct_def.name().text(),
                            item.inner()
                                .first_token()
                                .expect("struct item should have tokens")
                                .text_range()
                                .into(),
                        ),
                        SyntaxElement::from(struct_def.inner().clone()),
                        &self.exceptable_nodes(),
                    );
                    break;
                }
            }
        }
    }
}
