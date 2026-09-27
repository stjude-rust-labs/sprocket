//! A lint rule for extraneous parameter metadata keys.

use wdl_analysis::Diagnostics;
use wdl_analysis::Document;
use wdl_analysis::Example;
use wdl_analysis::LabeledSnippet;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Severity;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::SectionParent;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::WorkflowDefinition;

use crate::Config;
use crate::Rule;
use crate::Tag;
use crate::TagSet;
use crate::rules::ParameterMetaContext;
use crate::rules::ParameterMetaTarget;
use crate::rules::collect_parameter_meta;
use crate::util::diagnostic;

/// The identifier for the extraneous parameter meta rule.
const ID: &str = "ExtraneousParameterMeta";

/// Creates an "extra param meta" diagnostic.
fn extra_param_meta(
    severity: Severity,
    parent: &SectionParent,
    extra: &str,
    span: Span,
) -> Diagnostic {
    let (context, parent) = match parent {
        SectionParent::Task(t) => ("task", t.name()),
        SectionParent::Workflow(w) => ("workflow", w.name()),
        SectionParent::Struct(s) => ("struct", s.name()),
    };

    diagnostic(
        severity,
        format!(
            "{context} `{parent}` has an extraneous parameter metadata key named `{extra}`",
            parent = parent.text(),
        ),
    )
    .with_rule(ID)
    .with_label(
        "this key does not correspond to any input declaration",
        span,
    )
    .with_fix("remove the extraneous key from the `parameter_meta` section")
}

/// Detects extraneous entries in a `parameter_meta` section.
#[derive(Debug, Clone, Copy)]
pub struct ExtraneousParameterMetaRule {
    /// The severity of the rule's diagnostics.
    severity: Severity,
    /// The version of the WDL document being linted.
    version: Option<SupportedVersion>,
}

impl ExtraneousParameterMetaRule {
    /// Creates a new instance of the rule.
    pub fn new(config: &Config) -> Self {
        Self {
            severity: config.extraneous_parameter_meta.diagnostic_severity(),
            version: None,
        }
    }
}

impl Default for ExtraneousParameterMetaRule {
    fn default() -> Self {
        Self::new(&Config::default())
    }
}

impl Rule for ExtraneousParameterMetaRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that `parameter_meta` sections do not contain extraneous keys."
    }

    fn explanation(&self) -> &'static str {
        "Every key in a `parameter_meta` section should document a matching task input, workflow \
         input, or struct field. Keys without matching declarations are stale or misleading."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.3

task say_hello {
    parameter_meta {
        name: "The name of the person to greet"
        does_not_exist: "This is not a real parameter"
    }

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
                snippet: r#"version 1.3

task say_hello {
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
        TagSet::new(&[
            Tag::Completeness,
            Tag::Sorting,
            Tag::Documentation,
            Tag::SprocketCompatibility,
        ])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::TaskDefinitionNode,
            SyntaxKind::WorkflowDefinitionNode,
            SyntaxKind::StructDefinitionNode,
            SyntaxKind::ParameterMetadataSectionNode,
        ])
    }

    fn related_rules(&self) -> &'static [&'static str] {
        &["MissingParameterMeta", "ParameterMetaOrder"]
    }
}

impl Visitor for ExtraneousParameterMetaRule {
    fn reset(&mut self) {
        self.version = None;
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

        if let Some(context) = collect_parameter_meta(ParameterMetaTarget::Task(task)) {
            check_parameter_meta(
                self.severity,
                context,
                diagnostics,
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

        if let Some(context) = collect_parameter_meta(ParameterMetaTarget::Workflow(workflow)) {
            check_parameter_meta(
                self.severity,
                context,
                diagnostics,
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

        if let Some(context) = collect_parameter_meta(ParameterMetaTarget::Struct(
            def,
            self.version.expect("should have version"),
        )) {
            check_parameter_meta(
                self.severity,
                context,
                diagnostics,
                &self.exceptable_nodes(),
            );
        }
    }
}

/// Checks for extraneous items in a `parameter_meta` section.
fn check_parameter_meta(
    severity: Severity,
    context: ParameterMetaContext,
    diagnostics: &mut Diagnostics,
    exceptable_nodes: &Option<&'static [SyntaxKind]>,
) {
    let Some(param_meta) = context.parameter_meta else {
        return;
    };

    for (name, span) in &context.parameter_meta_map {
        if !context.decls.contains_key(name) {
            diagnostics.exceptable_add(
                extra_param_meta(severity, &context.parent, name, *span),
                param_meta.inner(),
                exceptable_nodes,
            );
        }
    }
}
