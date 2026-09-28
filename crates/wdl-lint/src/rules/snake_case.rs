//! A lint rule for ensuring tasks, workflows, and variables are named using
//! snake_case.

use std::collections::HashSet;
use std::fmt;

use convert_case::Boundary;
use convert_case::Case;
use convert_case::Converter;
use wdl_analysis::Diagnostics;
use wdl_analysis::Example;
use wdl_analysis::LabeledSnippet;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Severity;
use wdl_ast::Span;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_ast::v1::BoundDecl;
use wdl_ast::v1::InputSection;
use wdl_ast::v1::OutputSection;
use wdl_ast::v1::StructDefinition;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::UnboundDecl;
use wdl_ast::v1::WorkflowDefinition;

use crate::Config;
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
fn snake_case(
    severity: Severity,
    context: Context,
    name: &str,
    properly_cased_name: &str,
    span: Span,
) -> Diagnostic {
    Diagnostic::new(
        severity,
        format!("{context} name `{name}` is not snake_case"),
    )
    .with_rule(ID)
    .with_label("this name must be snake_case", span)
    .with_fix(format!("replace `{name}` with `{properly_cased_name}`"))
}

/// Detects non-snake_cased identifiers.
#[derive(Debug, Clone)]
pub struct SnakeCaseRule {
    /// The severity of the rule's diagnostics.
    severity: Severity,
    /// Whether the visitor is currently within a struct.
    within_struct: bool,
    /// Whether the visitor is currently within an input section.
    within_input: bool,
    /// Whether the visitor is currently within an output section.
    within_output: bool,
    /// Allowed names from the config.
    allowed_names: HashSet<String>,
}

impl SnakeCaseRule {
    /// Create a new instance of `SnakeCaseRule`.
    pub fn new(config: &Config) -> Self {
        Self {
            severity: config.snake_case.diagnostic_severity(),
            within_struct: false,
            within_input: false,
            within_output: false,
            allowed_names: HashSet::from_iter(config.snake_case.allowed_names.iter().cloned()),
        }
    }
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

    /// Checks if the given name is snake case, and if not adds a diagnostic.
    fn check_name(
        &self,
        context: Context,
        name: &str,
        span: Span,
        diagnostics: &mut Diagnostics,
        node: &SyntaxNode,
    ) {
        if self.allowed_names.contains(name) {
            return;
        }

        let converter = Converter::new()
            .remove_boundaries(&[Boundary::DigitLower, Boundary::LowerDigit])
            .to_case(Case::Snake);
        let properly_cased_name = converter.convert(name);
        if name != properly_cased_name {
            let warning = snake_case(self.severity, context, name, &properly_cased_name, span);
            diagnostics.exceptable_add(warning, node, &self.exceptable_nodes());
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

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.2

task SayHello {
    command <<<
        echo "Hello, World!"
    >>>
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: None,
                snippet: r#"version 1.2

task say_hello {
    command <<<
        echo "Hello, World!"
    >>>
}
"#,
            }),
        }]
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

    fn related_rules(&self) -> &'static [&'static str] {
        &["PascalCase"]
    }
}

impl Visitor for SnakeCaseRule {
    fn reset(&mut self) {
        *self = Self {
            severity: self.severity,
            allowed_names: std::mem::take(&mut self.allowed_names),
            within_struct: false,
            within_input: false,
            within_output: false,
        };
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
        self.check_name(
            Context::Task,
            name.text(),
            name.span(),
            diagnostics,
            task.inner(),
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
        self.check_name(
            Context::Workflow,
            name.text(),
            name.span(),
            diagnostics,
            workflow.inner(),
        );
    }

    fn bound_decl(&mut self, diagnostics: &mut Diagnostics, reason: VisitReason, decl: &BoundDecl) {
        if reason == VisitReason::Exit {
            return;
        }

        let name = decl.name();
        let context = self.determine_decl_context();
        self.check_name(context, name.text(), name.span(), diagnostics, decl.inner());
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
        self.check_name(context, name.text(), name.span(), diagnostics, decl.inner());
    }
}
