//! A lint rule for ensuring tasks, workflows, and variables are named using
//! snake_case.

use std::fmt;

use convert_case::Boundary;
use convert_case::Case;
use convert_case::Converter;
use wdl_ast::v1::BoundDecl;
use wdl_ast::v1::InputSection;
use wdl_ast::v1::OutputSection;
use wdl_ast::v1::StructDefinition;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::UnboundDecl;
use wdl_ast::v1::Visitor;
use wdl_ast::v1::WorkflowDefinition;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Span;
use wdl_ast::VisitReason;

use super::Rule;
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
fn check_name(context: Context, name: &str, span: Span, diagnostics: &mut Diagnostics) {
    let converter = Converter::new()
        .remove_boundaries(&[Boundary::DigitLower, Boundary::LowerDigit])
        .to_case(Case::Snake);
    let properly_cased_name = converter.convert(name);
    if name != properly_cased_name {
        let warning = snake_case(context, name, &properly_cased_name, span);
        diagnostics.add(warning);
    }
}

/// Detects non-snake_cased identifiers.
#[derive(Debug, Clone, Copy)]
pub struct SnakeCaseRule;

impl Rule for SnakeCaseRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that tasks, workflows, and variables use snake_case."
    }

    fn explanation(&self) -> &'static str {
        "Workflow, task, and variable names should be in snake case. Maintaining a consistent \
         naming convention makes the code easier to read and understand."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Naming, Tag::Style, Tag::Clarity])
    }

    fn visitor(&self) -> Box<dyn Visitor<State = Diagnostics>> {
        Box::<SnakeCaseVisitor>::default()
    }
}

/// Implements the visitor for the snake case rule.
#[derive(Debug, Default)]
struct SnakeCaseVisitor {
    /// Whether the visitor is currently within a struct.
    within_struct: bool,
    /// Whether the visitor is currently within an input section.
    within_input: bool,
    /// Whether the visitor is currently within an output section.
    within_output: bool,
}

impl SnakeCaseVisitor {
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

impl Visitor for SnakeCaseVisitor {
    type State = Diagnostics;

    fn struct_definition(
        &mut self,
        _state: &mut Self::State,
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
        _state: &mut Self::State,
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
        _state: &mut Self::State,
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
        state: &mut Self::State,
        reason: VisitReason,
        task: &TaskDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let name = task.name();
        check_name(Context::Task, name.as_str(), name.span(), state);
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

        let name = workflow.name();
        check_name(Context::Workflow, name.as_str(), name.span(), state);
    }

    fn bound_decl(&mut self, state: &mut Self::State, reason: VisitReason, decl: &BoundDecl) {
        if reason == VisitReason::Exit {
            return;
        }

        let name = decl.name();
        let context = self.determine_decl_context();
        check_name(context, name.as_str(), name.span(), state);
    }

    fn unbound_decl(&mut self, state: &mut Self::State, reason: VisitReason, decl: &UnboundDecl) {
        if reason == VisitReason::Exit {
            return;
        }

        let name = decl.name();
        let context = self.determine_decl_context();
        check_name(context, name.as_str(), name.span(), state);
    }
}
