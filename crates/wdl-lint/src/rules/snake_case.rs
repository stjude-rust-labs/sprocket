//! A lint rule for ensuring tasks, workflows, and variables are named using
//! snake_case.

use std::fmt;

use convert_case::Boundary;
use convert_case::Case;
use convert_case::Converter;
use wdl_analysis::Diagnostics;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::BoundDecl;
use wdl_ast::v1::InputSection;
use wdl_ast::v1::OutputSection;
use wdl_ast::v1::StructDefinition;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::UnboundDecl;
use wdl_ast::v1::WorkflowDefinition;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// Represents context of an warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Context {
    /// The warning occurred in a task.
    Task,
    /// The warning occurred in a workflow.
    Workflow,
    /// The warning occurred in a struct.
    Struct,
    /// The warning occurred in an input section.
    Input,
    /// The warning occurred in an output section.
    Output,
    /// The warning occurred in a private declaration.
    PrivateDecl,
}

impl fmt::Display for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Task => write!(f, "task"),
            Self::Workflow => write!(f, "workflow"),
            Self::Struct => write!(f, "struct member"),
            Self::Input => write!(f, "input"),
            Self::Output => write!(f, "output"),
            Self::PrivateDecl => write!(f, "private declaration"),
        }
    }
}

/// The identifier for the snake_case rule.
const ID: &str = "SnakeCase";

/// Creates a "snake case" diagnostic.
fn snake_case(context: Context, name: &str, properly_cased_name: &str, span: Span) -> Diagnostic {
    Diagnostic::warning(format!("{context} name `{name}` is not snake_case"))
        .with_rule(ID)
        .with_label("this name must be snake_case", span)
        .with_fix(format!("replace `{name}` with `{properly_cased_name}`"))
}

/// Checks if the given name is snake case, and if not adds a warning to the
/// diagnostics.
fn check_name(
    context: Context,
    name: &str,
    span: Span,
    diagnostics: &mut Diagnostics,
    element: SyntaxElement,
    exceptable_nodes: &Option<&'static [SyntaxKind]>,
) {
    let converter = Converter::new()
        .remove_boundaries(&[Boundary::DIGIT_LOWER, Boundary::LOWER_DIGIT])
        .to_case(Case::Snake);
    let properly_cased_name = converter.convert(name);
    if name != properly_cased_name {
        let warning = snake_case(context, name, &properly_cased_name, span);
        diagnostics.exceptable_add(warning, element, exceptable_nodes);
    }
}

/// Detects non-snake_cased identifiers.
#[derive(Default, Debug, Clone, Copy)]
pub struct SnakeCaseRule {
    /// Whether the visitor is currently within a struct.
    within_struct: bool,
    /// Whether the visitor is currently within an input section.
    within_input: bool,
    /// Whether the visitor is currently within an output section.
    within_output: bool,
}

impl SnakeCaseRule {
    /// Determines current declaration context.
    fn determine_decl_context(&self) -> Context {
        if self.within_struct {
            Context::Struct
        } else if self.within_input {
            Context::Input
        } else if self.within_output {
            Context::Output
        } else {
            Context::PrivateDecl
        }
    }
}

impl Rule for SnakeCaseRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that tasks, workflows, and variables are defined with snake_case names."
    }

    fn explanation(&self) -> &'static str {
        "Workflow, task, and variable names should be in snake case. Maintaining a consistent \
         naming convention makes the code easier to read and understand."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Naming, Tag::Style, Tag::Clarity])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::StructDefinitionNode,
            SyntaxKind::TaskDefinitionNode,
            SyntaxKind::WorkflowDefinitionNode,
            SyntaxKind::InputSectionNode,
            SyntaxKind::OutputSectionNode,
            SyntaxKind::BoundDeclNode,
            SyntaxKind::UnboundDeclNode,
        ])
    }

    fn related_rules(&self) -> &[&'static str] {
        &["PascalCase"]
    }
}

impl Visitor for SnakeCaseRule {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn struct_definition(
        &mut self,
        _diagnostics: &mut Diagnostics,
        reason: VisitReason,
        _def: &StructDefinition,
    ) {
        match reason {
            VisitReason::Enter => {
                self.within_struct = true;
            }
            VisitReason::Exit => {
                self.within_struct = false;
            }
        }
    }

    fn input_section(
        &mut self,
        _diagnostics: &mut Diagnostics,
        reason: VisitReason,
        _section: &InputSection,
    ) {
        match reason {
            VisitReason::Enter => {
                self.within_input = true;
            }
            VisitReason::Exit => {
                self.within_input = false;
            }
        }
    }

    fn output_section(
        &mut self,
        _diagnostics: &mut Diagnostics,
        reason: VisitReason,
        _section: &OutputSection,
    ) {
        match reason {
            VisitReason::Enter => {
                self.within_output = true;
            }
            VisitReason::Exit => {
                self.within_output = false;
            }
        }
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

        let name = task.name();
        check_name(
            Context::Task,
            name.text(),
            name.span(),
            diagnostics,
            SyntaxElement::from(task.inner().clone()),
            &self.exceptable_nodes(),
        );
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

        let name = workflow.name();
        check_name(
            Context::Workflow,
            name.text(),
            name.span(),
            diagnostics,
            SyntaxElement::from(workflow.inner().clone()),
            &self.exceptable_nodes(),
        );
    }

    fn bound_decl(&mut self, diagnostics: &mut Diagnostics, reason: VisitReason, decl: &BoundDecl) {
        if reason == VisitReason::Exit {
            return;
        }

        let name = decl.name();
        let context = self.determine_decl_context();
        check_name(
            context,
            name.text(),
            name.span(),
            diagnostics,
            SyntaxElement::from(decl.inner().clone()),
            &self.exceptable_nodes(),
        );
    }

    fn unbound_decl(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        decl: &UnboundDecl,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let name = decl.name();
        let context = self.determine_decl_context();
        check_name(
            context,
            name.text(),
            name.span(),
            diagnostics,
            SyntaxElement::from(decl.inner().clone()),
            &self.exceptable_nodes(),
        );
    }
}
