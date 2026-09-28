//! A lint rule for enforcing configurable naming conventions on tasks,
//! workflows, variables, and user-defined types.

use std::collections::HashSet;
use std::fmt;

use convert_case::Boundary;
use convert_case::Case;
use convert_case::Converter;
use schemars::JsonSchema;
use serde::Serialize;
use strum::Display;
use strum::EnumString;
use toml_spanner::Toml;
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
use wdl_ast::v1::EnumDefinition;
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
    /// The warning occurred in an enum.
    Enum,
    /// The warning occurred in an enum choice.
    EnumChoice,
    /// The warning occurred in a struct member.
    StructMember,
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

/// A case style for identifiers.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, Display, EnumString, Serialize, Toml, JsonSchema,
)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
#[toml(Toml, rename_all = "kebab-case")]
#[schemars(rename_all = "kebab-case")]
pub enum CaseStyle {
    /// `snake_case`.
    SnakeCase,
    /// `SCREAMING_SNAKE_CASE`.
    ScreamingSnakeCase,
    /// `camelCase`.
    CamelCase,
    /// `PascalCase`.
    PascalCase,
}

impl From<CaseStyle> for Case<'static> {
    fn from(style: CaseStyle) -> Self {
        match style {
            CaseStyle::SnakeCase => Case::Snake,
            CaseStyle::ScreamingSnakeCase => Case::Constant,
            CaseStyle::CamelCase => Case::Camel,
            CaseStyle::PascalCase => Case::Pascal,
        }
    }
}

impl CaseStyle {
    /// Gets the name of the style as used in diagnostics.
    fn diagnostic_name(self) -> &'static str {
        match self {
            Self::SnakeCase => "snake case",
            Self::ScreamingSnakeCase => "screaming snake case",
            Self::CamelCase => "camel case",
            Self::PascalCase => "pascal case",
        }
    }

    /// Converts a name to this case style.
    ///
    /// Boundaries between a digit and a preceding letter, or a following
    /// lowercase letter, are not word boundaries, so a name like `v1` is not
    /// split into `v_1`.
    pub fn convert(self, name: &str) -> String {
        // A name without lowercase letters has no word boundaries other than
        // underscores, so it is already in screaming snake case (this keeps
        // names like `X2Y` from being split apart at their digits).
        if self == Self::ScreamingSnakeCase && !name.chars().any(char::is_lowercase) {
            return name.to_string();
        }

        Converter::new()
            .remove_boundaries(&[
                Boundary::DigitLower,
                Boundary::LowerDigit,
                Boundary::UpperDigit,
            ])
            .to_case(self.into())
            .convert(name)
    }

    /// Determines whether a name is already in this case style.
    pub fn matches(self, name: &str) -> bool {
        self.convert(name) == name
    }
}

/// Creates a "naming convention" diagnostic.
fn naming_convention(
    severity: Severity,
    context: Context,
    name: &str,
    style: CaseStyle,
    properly_cased_name: &str,
    span: Span,
) -> Diagnostic {
    let style_name = style.diagnostic_name();
    Diagnostic::new(
        severity,
        format!("{context} name `{name}` is not {style_name}"),
    )
    .with_rule(ID)
    .with_label(format!("this name must be {style_name}"), span)
    .with_fix(format!("replace `{name}` with `{properly_cased_name}`"))
}

/// Enforces configurable naming conventions.
#[derive(Debug, Clone)]
pub struct NamingConventionRule {
    /// The severity of the rule's diagnostics.
    severity: Severity,
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
    /// Allowed names from the config.
    allowed_names: HashSet<String>,
}

impl NamingConventionRule {
    /// Creates a new instance of `NamingConventionRule`.
    pub fn new(config: &Config) -> Self {
        let config = &config.naming_convention;
        Self {
            severity: config.diagnostic_severity(),
            within_struct: false,
            within_input: false,
            within_output: false,
            task: config.task,
            workflow: config.workflow,
            variable: config.variable,
            type_style: config.r#type,
            struct_member: config.struct_member,
            allowed_names: HashSet::from_iter(config.allowed_names.iter().cloned()),
        }
    }

    /// Determines current declaration context.
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

    /// Gets the case style configured for a context.
    fn style_for(&self, context: Context) -> CaseStyle {
        match context {
            Context::Task => self.task,
            Context::Workflow => self.workflow,
            Context::Struct | Context::Enum | Context::EnumChoice => self.type_style,
            Context::StructMember => self.struct_member,
            Context::Input | Context::Output | Context::PrivateDecl => self.variable,
        }
    }

    /// Checks if the given name matches its configured case style, and if not
    /// adds a diagnostic.
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

        let style = self.style_for(context);
        if !style.matches(name) {
            let properly_cased_name = style.convert(name);
            let warning = naming_convention(
                self.severity,
                context,
                name,
                style,
                &properly_cased_name,
                span,
            );
            diagnostics.exceptable_add(warning, node, &self.exceptable_nodes());
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
        "Names should follow a consistent case convention. By default, tasks, workflows, \
         variables, and struct members use snake case, and user-defined type names (structs, \
         enums, and enum choices) use pascal case. The case style for each category can be \
         configured. Maintaining a consistent naming convention makes the code easier to read and \
         understand."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.3

task SayHello {
    command <<<
        echo "Hello, World!"
    >>>
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: None,
                snippet: r#"version 1.3

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
            severity: self.severity,
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

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn convert_single_words() {
        assert_eq!(CaseStyle::SnakeCase.convert("word"), "word");
        assert_eq!(CaseStyle::SnakeCase.convert("Word"), "word");
        assert_eq!(CaseStyle::ScreamingSnakeCase.convert("word"), "WORD");
        assert_eq!(CaseStyle::CamelCase.convert("Word"), "word");
        assert_eq!(CaseStyle::PascalCase.convert("word"), "Word");
    }

    #[test]
    fn convert_between_styles() {
        for (style, expected) in [
            (CaseStyle::SnakeCase, "my_bad_name"),
            (CaseStyle::ScreamingSnakeCase, "MY_BAD_NAME"),
            (CaseStyle::CamelCase, "myBadName"),
            (CaseStyle::PascalCase, "MyBadName"),
        ] {
            for name in ["my_bad_name", "MY_BAD_NAME", "myBadName", "MyBadName"] {
                assert_eq!(style.convert(name), expected, "{style}: {name}");
            }
        }
    }

    #[test]
    fn convert_preserves_digit_boundaries() {
        assert_eq!(CaseStyle::SnakeCase.convert("v1"), "v1");
        assert_eq!(
            CaseStyle::SnakeCase.convert("sample1_reads2"),
            "sample1_reads2"
        );
        assert_eq!(CaseStyle::SnakeCase.convert("V1"), "v1");
        assert_eq!(
            CaseStyle::SnakeCase.convert("Sample1Reads2"),
            "sample1_reads2"
        );
        assert_eq!(CaseStyle::ScreamingSnakeCase.convert("v1"), "V1");
        assert_eq!(CaseStyle::ScreamingSnakeCase.convert("SAMPLE1"), "SAMPLE1");
        assert_eq!(CaseStyle::ScreamingSnakeCase.convert("X2Y"), "X2Y");
        assert_eq!(
            CaseStyle::ScreamingSnakeCase.convert("sample1Reads2"),
            "SAMPLE1_READS2"
        );
        assert_eq!(CaseStyle::CamelCase.convert("my_v1"), "myV1");
        assert_eq!(CaseStyle::PascalCase.convert("my_v1"), "MyV1");
        assert_eq!(CaseStyle::PascalCase.convert("v1"), "V1");
    }

    #[test]
    fn convert_acronyms() {
        assert_eq!(CaseStyle::SnakeCase.convert("HTTPServer"), "http_server");
        assert_eq!(CaseStyle::SnakeCase.convert("parseXML"), "parse_xml");
        assert_eq!(
            CaseStyle::ScreamingSnakeCase.convert("parseXML"),
            "PARSE_XML"
        );
        assert_eq!(CaseStyle::CamelCase.convert("parse_xml"), "parseXml");
        assert_eq!(CaseStyle::PascalCase.convert("HTTPServer"), "HttpServer");
    }

    #[test]
    fn convert_underscores() {
        // Snake case styles keep underscores as they are.
        assert_eq!(CaseStyle::SnakeCase.convert("trailing_"), "trailing_");
        assert_eq!(
            CaseStyle::SnakeCase.convert("double__underscore"),
            "double__underscore"
        );
        assert_eq!(
            CaseStyle::ScreamingSnakeCase.convert("trailing_"),
            "TRAILING_"
        );

        // Camel and Pascal case strip them.
        assert_eq!(CaseStyle::CamelCase.convert("trailing_"), "trailing");
        assert_eq!(
            CaseStyle::PascalCase.convert("_leading_name_"),
            "LeadingName"
        );
        assert_eq!(
            CaseStyle::PascalCase.convert("double__underscore"),
            "DoubleUnderscore"
        );
    }

    #[test]
    fn matches() {
        assert!(CaseStyle::SnakeCase.matches("snake_case"));
        assert!(CaseStyle::SnakeCase.matches("v1"));
        assert!(CaseStyle::SnakeCase.matches("a"));
        assert!(!CaseStyle::SnakeCase.matches("Snake_case"));
        assert!(!CaseStyle::SnakeCase.matches("snakeCase"));
        assert!(CaseStyle::ScreamingSnakeCase.matches("SCREAMING_SNAKE_CASE"));
        assert!(CaseStyle::ScreamingSnakeCase.matches("V1"));
        assert!(!CaseStyle::ScreamingSnakeCase.matches("snake_case"));
        assert!(CaseStyle::CamelCase.matches("camelCase"));
        assert!(CaseStyle::CamelCase.matches("word"));
        assert!(!CaseStyle::CamelCase.matches("CamelCase"));
        assert!(CaseStyle::PascalCase.matches("PascalCase"));
        assert!(CaseStyle::PascalCase.matches("V1"));
        assert!(!CaseStyle::PascalCase.matches("HTTPServer"));
        assert!(!CaseStyle::PascalCase.matches("pascalCase"));
        assert!(!CaseStyle::PascalCase.matches("Pascal_Case"));
        assert!(!CaseStyle::PascalCase.matches("Pascal_"));
        assert!(!CaseStyle::CamelCase.matches("camel_"));
    }

    #[test]
    fn conversion_is_idempotent() {
        for style in [
            CaseStyle::SnakeCase,
            CaseStyle::ScreamingSnakeCase,
            CaseStyle::CamelCase,
            CaseStyle::PascalCase,
        ] {
            for name in [
                "v1",
                "x2y",
                "X2Y",
                "HTTPServer",
                "a_",
                "a1B2",
                "my_v1_name",
                "ABC",
                "a",
                "A",
            ] {
                let converted = style.convert(name);
                assert!(style.matches(&converted), "{style}: {name} -> {converted}");
            }
        }
    }

    #[test]
    fn parse_and_display() {
        for (style, s) in [
            (CaseStyle::SnakeCase, "snake-case"),
            (CaseStyle::ScreamingSnakeCase, "screaming-snake-case"),
            (CaseStyle::CamelCase, "camel-case"),
            (CaseStyle::PascalCase, "pascal-case"),
        ] {
            assert_eq!(style.to_string(), s);
            assert_eq!(s.parse::<CaseStyle>().unwrap(), style);
        }

        assert!("snake_case".parse::<CaseStyle>().is_err());
        assert!("PascalCase".parse::<CaseStyle>().is_err());
    }
}
