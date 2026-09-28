//! A lint rule for matching parameter metadata.

use indexmap::IndexMap;
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
use wdl_ast::Severity;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_ast::v1::Decl;
use wdl_ast::v1::ParameterMetadataSection;
use wdl_ast::v1::SectionParent;
use wdl_ast::v1::StructDefinition;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::WorkflowDefinition;
use wdl_ast::version::V1;

use crate::Config;
use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the matching parameter meta rule.
const ID: &str = "MissingParameterMeta";

/// Creates a "missing param meta" diagnostic.
fn missing_param_meta(
    severity: Severity,
    parent: &SectionParent,
    missing: &str,
    span: Span,
    suggest_doc_comments: bool,
) -> Diagnostic {
    let (context, decl_type, parent) = match parent {
        SectionParent::Task(t) => ("task", "input", t.name()),
        SectionParent::Workflow(w) => ("workflow", "input", w.name()),
        SectionParent::Struct(s) => ("struct", "field", s.name()),
    };

    let suggestion = if suggest_doc_comments {
        "doc comment"
    } else {
        "parameter metadata key"
    };

    let mut diagnostic = Diagnostic::new(
        severity,
        format!(
            "{context} `{parent}` is missing a {suggestion} for {decl_type} `{missing}`",
            parent = parent.text(),
        ),
    )
    .with_rule(ID)
    .with_label(
        format!(
            "this {decl_type} does not have {}",
            if suggest_doc_comments {
                "a doc comment"
            } else {
                "an entry in the parameter metadata section"
            }
        ),
        span,
    );

    if suggest_doc_comments {
        diagnostic = diagnostic.with_fix(format!(
            "add a doc comment to `{missing}` with a detailed description of the {decl_type}.",
        ));
    } else {
        diagnostic = diagnostic.with_fix(format!(
            "add a `{missing}` key to the `parameter_meta` section with a detailed description of \
             the {decl_type}.",
        ));
    }

    diagnostic
}

/// Information about a declaration checked against `parameter_meta`.
#[derive(Debug, Clone)]
pub(crate) struct ParameterDeclInfo {
    /// The span of the declaration name.
    pub(crate) span: Span,
    /// The syntax node to attach except directives to when no
    /// `parameter_meta` section exists.
    pub(crate) node: SyntaxNode,
    /// Whether the declaration has doc comments.
    pub(crate) has_doc_comments: bool,
}

/// Shared parameter metadata information for a task, workflow, or struct.
#[derive(Debug, Clone)]
pub(crate) struct ParameterMetaContext {
    /// The parent that owns the parameter metadata.
    pub(crate) parent: SectionParent,
    /// The declarations keyed by name.
    pub(crate) decls: IndexMap<String, ParameterDeclInfo>,
    /// The first parameter metadata section.
    pub(crate) parameter_meta: Option<ParameterMetadataSection>,
    /// Parameter metadata entries keyed by name.
    pub(crate) parameter_meta_map: IndexMap<String, Span>,
}

/// A task, workflow, or struct to collect parameter metadata from.
pub(crate) enum ParameterMetaTarget<'a> {
    /// A task definition.
    Task(&'a TaskDefinition),
    /// A workflow definition.
    Workflow(&'a WorkflowDefinition),
    /// A struct definition and the document version it appears in.
    Struct(&'a StructDefinition, SupportedVersion),
}

/// Collects declarations and parameter metadata for a task, workflow, or
/// struct.
pub(crate) fn collect_parameter_meta(
    target: ParameterMetaTarget<'_>,
) -> Option<ParameterMetaContext> {
    let (parent, decls, parameter_meta) = match target {
        ParameterMetaTarget::Task(task) => (
            SectionParent::Task(task.clone()),
            task.input().iter().flat_map(|i| i.declarations()).collect(),
            task.parameter_metadata(),
        ),
        ParameterMetaTarget::Workflow(workflow) => (
            SectionParent::Workflow(workflow.clone()),
            workflow
                .input()
                .iter()
                .flat_map(|i| i.declarations())
                .collect(),
            workflow.parameter_metadata(),
        ),
        ParameterMetaTarget::Struct(def, version) => {
            if version < SupportedVersion::V1(V1::Two) {
                return None;
            }

            (
                SectionParent::Struct(def.clone()),
                def.members().map(Decl::Unbound).collect(),
                def.parameter_metadata().next(),
            )
        }
    };

    let decls = collect_declarations(decls);
    let parameter_meta_map =
        parameter_meta
            .as_ref()
            .map_or_else(IndexMap::default, |parameter_meta| {
                parameter_meta
                    .items()
                    .map(|m| {
                        let name = m.name();
                        (name.text().to_string(), name.span())
                    })
                    .collect()
            });

    Some(ParameterMetaContext {
        parent,
        decls,
        parameter_meta,
        parameter_meta_map,
    })
}

/// Collects declaration information keyed by declaration name.
fn collect_declarations(decls: Vec<Decl>) -> IndexMap<String, ParameterDeclInfo> {
    decls
        .iter()
        .map(|decl| {
            (
                decl.name().text().to_string(),
                ParameterDeclInfo {
                    span: decl.name().span(),
                    node: decl.inner().clone(),
                    has_doc_comments: decl.doc_comments().is_some_and(|docs| !docs.is_empty()),
                },
            )
        })
        .collect()
}

/// Detects missing entries in a `parameter_meta` section.
#[derive(Debug, Clone, Copy)]
pub struct MissingParameterMetaRule {
    /// The severity of the rule's diagnostics.
    severity: Severity,
    /// The version of the WDL document being linted.
    version: Option<SupportedVersion>,
}

impl MissingParameterMetaRule {
    /// Creates a new instance of the rule.
    pub fn new(config: &Config) -> Self {
        Self {
            severity: config.missing_parameter_meta.diagnostic_severity(),
            version: Default::default(),
        }
    }
}

impl Rule for MissingParameterMetaRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that inputs and struct fields have `parameter_meta` entries or supplementary doc \
         comments."
    }

    fn explanation(&self) -> &'static str {
        "Each input parameter within a task or workflow and each struct field should have an \
         associated `parameter_meta` entry or doc comment with a detailed description."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.2

task say_hello {
    parameter_meta {
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
                snippet: r#"version 1.2

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
        &[
            "MetaDescription",
            "OutputSection",
            "RequirementsSection",
            "RuntimeSection",
            "MatchingOutputMeta",
            "DescriptionLength",
            "ExtraneousParameterMeta",
            "ParameterMetaOrder",
        ]
    }
}

/// Checks for missing items in a `parameter_meta` section.
fn check_parameter_meta(
    severity: Severity,
    context: ParameterMetaContext,
    diagnostics: &mut Diagnostics,
    exceptable_nodes: &Option<&'static [SyntaxKind]>,
) {
    if context.parameter_meta.is_none()
        && context.decls.iter().all(|(_, info)| !info.has_doc_comments)
    {
        // Leave the case of no `parameter_meta` or doc comments for
        // `MetaSections`
        return;
    }

    // The suggestion depends on whatever we find first, a doc comment or a
    // `parameter_meta` entry
    let suggest_doc_comments = context
        .decls
        .iter()
        .find_map(|(name, info)| {
            if info.has_doc_comments {
                Some(true)
            } else if context.parameter_meta_map.contains_key(name) {
                Some(false)
            } else {
                None
            }
        })
        .unwrap_or(false);

    for (name, info) in &context.decls {
        if !info.has_doc_comments && !context.parameter_meta_map.contains_key(name) {
            let node = context.parameter_meta.as_ref().map_or_else(
                || info.node.clone(),
                |param_meta| param_meta.inner().clone(),
            );
            diagnostics.exceptable_add(
                missing_param_meta(
                    severity,
                    &context.parent,
                    name,
                    info.span,
                    suggest_doc_comments,
                ),
                &node,
                exceptable_nodes,
            );
        }
    }
}

impl Visitor for MissingParameterMetaRule {
    fn reset(&mut self) {
        self.version = Default::default();
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
