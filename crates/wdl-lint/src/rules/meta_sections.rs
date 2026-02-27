//! A lint rule for missing meta and parameter_meta sections.

use std::fmt;

use wdl_analysis::Diagnostics;
use wdl_analysis::Document;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Ident;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
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

    fn examples(&self) -> &'static [&'static str] {
        &[
            r#"```wdl
version 1.2

task say_hello {
    input {
        String name
    }

    command <<<
        echo "Hello, ~{name}!"
    >>>
}
```"#,
            r#"Use instead:

```wdl
version 1.2

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
```"#,
        ]
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

    fn related_rules(&self) -> &[&'static str] {
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

        let inputs_present = task.input().is_some();

        if inputs_present && task.metadata().is_none() && task.parameter_metadata().is_none() {
            diagnostics.exceptable_add(
                missing_sections(task.name(), Context::Task),
                SyntaxElement::from(task.inner().clone()),
                &self.exceptable_nodes(),
            );
        } else if task.metadata().is_none() {
            diagnostics.exceptable_add(
                missing_section(task.name(), Section::Meta, Context::Task),
                SyntaxElement::from(task.inner().clone()),
                &self.exceptable_nodes(),
            );
        } else if inputs_present && task.parameter_metadata().is_none() {
            diagnostics.exceptable_add(
                missing_section(task.name(), Section::ParameterMeta, Context::Task),
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

        let inputs_present = workflow.input().is_some();

        if inputs_present
            && workflow.metadata().is_none()
            && workflow.parameter_metadata().is_none()
        {
            diagnostics.exceptable_add(
                missing_sections(workflow.name(), Context::Workflow),
                SyntaxElement::from(workflow.inner().clone()),
                &self.exceptable_nodes(),
            );
        } else if workflow.metadata().is_none() {
            diagnostics.exceptable_add(
                missing_section(workflow.name(), Section::Meta, Context::Workflow),
                SyntaxElement::from(workflow.inner().clone()),
                &self.exceptable_nodes(),
            );
        } else if inputs_present && workflow.parameter_metadata().is_none() {
            diagnostics.exceptable_add(
                missing_section(workflow.name(), Section::ParameterMeta, Context::Workflow),
                SyntaxElement::from(workflow.inner().clone()),
                &self.exceptable_nodes(),
            );
        }
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

        if def.metadata().next().is_none() && def.parameter_metadata().next().is_none() {
            diagnostics.exceptable_add(
                missing_sections(def.name(), Context::Struct),
                SyntaxElement::from(def.inner().clone()),
                &self.exceptable_nodes(),
            );
        } else if def.metadata().next().is_none() {
            diagnostics.exceptable_add(
                missing_section(def.name(), Section::Meta, Context::Struct),
                SyntaxElement::from(def.inner().clone()),
                &self.exceptable_nodes(),
            );
        } else if def.parameter_metadata().next().is_none() {
            diagnostics.exceptable_add(
                missing_section(def.name(), Section::ParameterMeta, Context::Struct),
                SyntaxElement::from(def.inner().clone()),
                &self.exceptable_nodes(),
            );
        }
    }
}
