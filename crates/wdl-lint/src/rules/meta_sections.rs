//! A lint rule for missing meta and parameter_meta sections.

use std::fmt;

use wdl_analysis::Diagnostics;
use wdl_analysis::Document;
use wdl_analysis::Example;
use wdl_analysis::LabeledSnippet;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Documented;
use wdl_ast::Ident;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_ast::SyntaxTokenExt;
use wdl_ast::doc_comments;
use wdl_ast::v1::InputSection;
use wdl_ast::v1::MetadataSection;
use wdl_ast::v1::ParameterMetadataSection;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::WorkflowDefinition;
use wdl_ast::version::V1;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// Which section is missing.
enum Section {
    /// The `meta` section is missing.
    Meta,
    /// The `parameter_meta` section is missing.
    ParameterMeta,
}

impl fmt::Display for Section {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Meta => write!(f, "meta"),
            Self::ParameterMeta => write!(f, "parameter_meta"),
        }
    }
}

/// The context for which section is missing.
#[derive(PartialEq, Eq)]
enum Context {
    /// A task.
    Task,
    /// A workflow.
    Workflow,
    /// A struct.
    Struct,
}

impl fmt::Display for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Task => write!(f, "task"),
            Self::Workflow => write!(f, "workflow"),
            Self::Struct => write!(f, "struct"),
        }
    }
}

/// The identifier for the missing meta sections rule.
const ID: &str = "MetaSections";

/// Creates a "missing section" diagnostic.
fn missing_section(name: Ident, section: Section, context: Context) -> Diagnostic {
    Diagnostic::note(format!(
        "{context} `{name}` is missing a `{section}` section",
        name = name.text(),
    ))
    .with_rule(ID)
    .with_label(
        format!("this {context} is missing a `{section}` section"),
        name.span(),
    )
    .with_fix("add the missing section")
}

/// Creates a "missing sections" diagnostic.
fn missing_sections(name: Ident, context: Context) -> Diagnostic {
    Diagnostic::note(format!(
        "{context} `{name}` is missing both `meta` and `parameter_meta` sections",
        name = name.text(),
    ))
    .with_rule(ID)
    .with_label(
        format!("this {context} is missing both `meta` and `parameter_meta` sections"),
        name.span(),
    )
    .with_fix("add both the `meta` and `parameter_meta` sections")
}

/// A lint rule for missing meta and parameter_meta sections.
#[derive(Default, Debug, Clone, Copy)]
pub struct MetaSectionsRule {
    /// The version of the WDL document being linted.
    version: Option<SupportedVersion>,
}

impl Rule for MetaSectionsRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that tasks and workflows have the required `meta` and `parameter_meta` sections."
    }

    fn explanation(&self) -> &'static str {
        "It is important that WDL code is well-documented. Every task and workflow should have \
         both a meta and parameter_meta section. Tasks without an `input` section are permitted to \
         skip the `parameter_meta` section."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.2

task say_hello {
    input {
        String name
    }

    command <<<
        echo "Hello, ~{name}!"
    >>>
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: None,
                snippet: r#"version 1.2

task say_hello {
    meta {
        description: "Says hello for the given name"
    }

    parameter_meta {
        name: "The name of the person to greet"
    }

    input {
        String name
    }

    command <<<
        echo "Hello, ~{name}!"
    >>>
}
"#,
            }),
        }]
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Completeness, Tag::Clarity, Tag::Documentation])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::TaskDefinitionNode,
            SyntaxKind::WorkflowDefinitionNode,
            SyntaxKind::StructDefinitionNode,
        ])
    }

    fn related_rules(&self) -> &'static [&'static str] {
        &[
            "MetaDescription",
            "ParameterMetaMatched",
            "OutputSection",
            "RequirementsSection",
            "RuntimeSection",
            "MatchingOutputMeta",
            "DescriptionLength",
        ]
    }
}

impl MetaSectionsRule {
    /// The actual rule logic.
    #[allow(clippy::too_many_arguments)]
    fn check_meta_sections(
        &self,
        diagnostics: &mut Diagnostics,
        name: Ident,
        node: &SyntaxNode,
        inputs: Option<InputSection>,
        parameter_meta: Option<ParameterMetadataSection>,
        meta: Option<MetadataSection>,
        context: Context,
    ) {
        let self_documented = node.first_token().is_some_and(|t| {
            doc_comments::<SyntaxNode>(t.preceding_trivia(), false)
                .next()
                .is_some()
        });

        let inputs_present = inputs.is_some();
        let inputs_documented = inputs.is_some_and(|i| {
            i.declarations()
                .any(|d| d.doc_comments().is_some_and(|c| !c.is_empty()))
        });

        let needs_meta = meta.is_none() && !self_documented;
        let needs_parameter_meta = (inputs_present && !inputs_documented
            || context == Context::Struct)
            && parameter_meta.is_none();

        if needs_meta && needs_parameter_meta {
            diagnostics.exceptable_add(
                missing_sections(name, context),
                node,
                &self.exceptable_nodes(),
            );
        } else if needs_meta {
            diagnostics.exceptable_add(
                missing_section(name, Section::Meta, context),
                node,
                &self.exceptable_nodes(),
            );
        } else if needs_parameter_meta {
            diagnostics.exceptable_add(
                missing_section(name, Section::ParameterMeta, context),
                node,
                &self.exceptable_nodes(),
            );
        }
    }
}

impl Visitor for MetaSectionsRule {
    fn reset(&mut self) {
        *self = Default::default();
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

        self.version = Some(version);
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

        self.check_meta_sections(
            diagnostics,
            task.name(),
            task.inner(),
            task.input(),
            task.parameter_metadata(),
            task.metadata(),
            Context::Task,
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

        self.check_meta_sections(
            diagnostics,
            workflow.name(),
            workflow.inner(),
            workflow.input(),
            workflow.parameter_metadata(),
            workflow.metadata(),
            Context::Workflow,
        );
    }

    fn struct_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        def: &wdl_ast::v1::StructDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Only check struct definitions for WDL >=1.2
        if self.version.expect("should have version") < SupportedVersion::V1(V1::Two) {
            return;
        }

        self.check_meta_sections(
            diagnostics,
            def.name(),
            def.inner(),
            None,
            def.parameter_metadata().next(),
            def.metadata().next(),
            Context::Struct,
        );
    }
}
