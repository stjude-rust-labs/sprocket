//! A lint rule for enforcing configurable naming conventions on tasks,
//! workflows, variables, and user-defined types.

use std::collections::HashSet;
use std::fmt;

use wdl_analysis::Diagnostics;
use wdl_analysis::Example;
use wdl_analysis::LabeledSnippet;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_ast::v1::BoundDecl;
use wdl_ast::v1::EnumDefinition;
use wdl_ast::v1::InputSection;
use wdl_ast::v1::OutputSection;
use wdl_ast::v1::StructDefinition;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::UnboundDecl;
use wdl_ast::v1::WorkflowDefinition;

use crate::CaseStyle;
use crate::Config;
use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The category of identifier being checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Context {
    /// A task name.
    Task,
    /// A workflow name.
    Workflow,
    /// A struct (user-defined type) name.
    Struct,
    /// An enum (user-defined type) name.
    Enum,
    /// An enum choice.
    EnumChoice,
    /// A struct member.
    StructMember,
    /// An input declaration.
    Input,
    /// An output declaration.
    Output,
    /// A private declaration.
    PrivateDecl,
}

impl fmt::Display for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Task => write!(f, "task"),
            Self::Workflow => write!(f, "workflow"),
            Self::Struct => write!(f, "struct"),
            Self::Enum => write!(f, "enum"),
            Self::EnumChoice => write!(f, "enum choice"),
            Self::StructMember => write!(f, "struct member"),
            Self::Input => write!(f, "input"),
            Self::Output => write!(f, "output"),
            Self::PrivateDecl => write!(f, "private declaration"),
        }
    }
}

/// The identifier for the naming convention rule.
const ID: &str = "NamingConvention";

/// Creates a "naming convention" diagnostic.
fn naming_convention(
    context: Context,
    name: &str,
    style: CaseStyle,
    properly_cased_name: &str,
    span: Span,
) -> Diagnostic {
    let style_name = style.diagnostic_name();
    Diagnostic::warning(format!("{context} name `{name}` is not {style_name}"))
        .with_rule(ID)
        .with_label(format!("this name must be {style_name}"), span)
        .with_fix(format!("replace `{name}` with `{properly_cased_name}`"))
}

/// Enforces configurable naming conventions.
#[derive(Debug, Clone)]
pub struct NamingConventionRule {
    /// Whether the visitor is currently within a struct.
    within_struct: bool,
    /// Whether the visitor is currently within an input section.
    within_input: bool,
    /// Whether the visitor is currently within an output section.
    within_output: bool,
    /// The case style for task names.
    task: CaseStyle,
    /// The case style for workflow names.
    workflow: CaseStyle,
    /// The case style for variable names.
    variable: CaseStyle,
    /// The case style for user-defined type names.
    type_style: CaseStyle,
    /// The case style for struct member names.
    struct_member: CaseStyle,
    /// Names exempt from the rule.
    allowed_names: HashSet<String>,
}

impl NamingConventionRule {
    /// Creates a new instance of the rule from the given configuration.
    pub fn new(config: &Config) -> Self {
        let resolved = config.resolved(ID);
        Self {
            within_struct: false,
            within_input: false,
            within_output: false,
            task: resolved.task,
            workflow: resolved.workflow,
            variable: resolved.variable,
            type_style: resolved.r#type,
            struct_member: resolved.struct_member,
            allowed_names: HashSet::from_iter(resolved.allowed_names),
        }
    }

    /// Determines the current declaration context.
    fn determine_decl_context(&self) -> Context {
        if self.within_struct {
            Context::StructMember
        } else if self.within_input {
            Context::Input
        } else if self.within_output {
            Context::Output
        } else {
            Context::PrivateDecl
        }
    }

    /// Returns the case style configured for a context.
    fn style_for(&self, context: Context) -> CaseStyle {
        match context {
            Context::Task => self.task,
            Context::Workflow => self.workflow,
            Context::Struct | Context::Enum | Context::EnumChoice => self.type_style,
            Context::StructMember => self.struct_member,
            Context::Input | Context::Output | Context::PrivateDecl => self.variable,
        }
    }

    /// Checks a name against its configured case style.
    fn check_name(
        &self,
        context: Context,
        name: &str,
        span: Span,
        diagnostics: &mut Diagnostics,
        element: &SyntaxNode,
    ) {
        if self.allowed_names.contains(name) {
            return;
        }

        let style = self.style_for(context);
        let properly_cased_name = style.convert(name);
        if name != properly_cased_name {
            diagnostics.exceptable_add(
                naming_convention(context, name, style, &properly_cased_name, span),
                element,
                &self.exceptable_nodes(),
            );
        }
    }
}

impl Rule for NamingConventionRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that tasks, workflows, variables, and types follow the configured naming \
         conventions."
    }

    fn explanation(&self) -> &'static str {
        "Names should follow a consistent case convention. By default, tasks, workflows, and \
         variables use snake_case. User-defined type names and enum choices use PascalCase, and \
         struct members use snake_case. The convention for each category can be configured. \
         Maintaining a consistent naming convention makes the code easier to read and understand."
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
            SyntaxKind::EnumDefinitionNode,
            SyntaxKind::EnumChoiceNode,
            SyntaxKind::TaskDefinitionNode,
            SyntaxKind::WorkflowDefinitionNode,
            SyntaxKind::InputSectionNode,
            SyntaxKind::OutputSectionNode,
            SyntaxKind::BoundDeclNode,
            SyntaxKind::UnboundDeclNode,
        ])
    }

    fn related_rules(&self) -> &'static [&'static str] {
        &["DeclarationName", "InputName", "OutputName"]
    }
}

impl Visitor for NamingConventionRule {
    fn reset(&mut self) {
        *self = Self {
            within_struct: false,
            within_input: false,
            within_output: false,
            task: self.task,
            workflow: self.workflow,
            variable: self.variable,
            type_style: self.type_style,
            struct_member: self.struct_member,
            allowed_names: std::mem::take(&mut self.allowed_names),
        };
    }

    fn struct_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        def: &StructDefinition,
    ) {
        match reason {
            VisitReason::Enter => {
                self.within_struct = true;
                let name = def.name();
                self.check_name(
                    Context::Struct,
                    name.text(),
                    name.span(),
                    diagnostics,
                    def.inner(),
                );
            }
            VisitReason::Exit => {
                self.within_struct = false;
            }
        }
    }

    fn enum_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        def: &EnumDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let name = def.name();
        self.check_name(
            Context::Enum,
            name.text(),
            name.span(),
            diagnostics,
            def.inner(),
        );

        for choice in def.choices() {
            let name = choice.name();
            self.check_name(
                Context::EnumChoice,
                name.text(),
                name.span(),
                diagnostics,
                choice.inner(),
            );
        }
    }

    fn input_section(
        &mut self,
        _diagnostics: &mut Diagnostics,
        reason: VisitReason,
        _section: &InputSection,
    ) {
        self.within_input = reason == VisitReason::Enter;
    }

    fn output_section(
        &mut self,
        _diagnostics: &mut Diagnostics,
        reason: VisitReason,
        _section: &OutputSection,
    ) {
        self.within_output = reason == VisitReason::Enter;
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
