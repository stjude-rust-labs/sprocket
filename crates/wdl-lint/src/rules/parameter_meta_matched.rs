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
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::Decl;
use wdl_ast::v1::ParameterMetadataSection;
use wdl_ast::v1::SectionParent;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::WorkflowDefinition;
use wdl_ast::version::V1;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the matching parameter meta rule.
const ID: &str = "ParameterMetaMatched";

/// Creates a "missing param meta" diagnostic.
fn missing_param_meta(
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

    let mut diagnostic = Diagnostic::warning(format!(
        "{context} `{parent}` is missing a {suggestion} for {decl_type} `{missing}`",
        parent = parent.text(),
    ))
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

/// Creates an "extra param meta" diagnostic.
fn extra_param_meta(parent: &SectionParent, extra: &str, span: Span) -> Diagnostic {
    let (context, parent) = match parent {
        SectionParent::Task(t) => ("task", t.name()),
        SectionParent::Workflow(w) => ("workflow", w.name()),
        SectionParent::Struct(s) => ("struct", s.name()),
    };

    Diagnostic::note(format!(
        "{context} `{parent}` has an extraneous parameter metadata key named `{extra}`",
        parent = parent.text(),
    ))
    .with_rule(ID)
    .with_label(
        "this key does not correspond to any input declaration",
        span,
    )
    .with_fix("remove the extraneous key from the `parameter_meta` section")
}

/// Creates a "mismatched order" diagnostic.
fn mismatched_param_order(parent: &SectionParent, span: Span, expected_order: &str) -> Diagnostic {
    let (context, parent) = match parent {
        SectionParent::Task(t) => ("task", t.name()),
        SectionParent::Workflow(w) => ("workflow", w.name()),
        SectionParent::Struct(s) => ("struct", s.name()),
    };

    Diagnostic::note(format!(
        "parameter metadata in {context} `{parent}` is out of order",
        parent = parent.text(),
    ))
    .with_rule(ID)
    .with_label(
        "parameter metadata must be in the same order as inputs",
        span,
    )
    .with_fix(format!(
        "based on the current `input` order, order the parameter metadata as:\n{expected_order}"
    ))
}

/// Detects missing or extraneous entries in a `parameter_meta` section.
#[derive(Default, Debug, Clone, Copy)]
pub struct ParameterMetaMatchedRule {
    /// The version of the WDL document being linted.
    version: Option<SupportedVersion>,
}

impl Rule for ParameterMetaMatchedRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that inputs have a matching entry in a `parameter_meta` section, or supplementary \
         doc comments."
    }

    fn explanation(&self) -> &'static str {
        "Each input parameter within a task or workflow should have an associated `parameter_meta` \
         entry or doc comment with a detailed description of the input. Non-input keys are not \
         permitted within the `parameter_meta` block."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.2

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
        ]
    }
}

/// Checks for both missing and extra items in a `parameter_meta` section
/// along with the order of the items.
fn check_parameter_meta(
    parent: &SectionParent,
    decls: Vec<Decl>,
    param_meta: Option<ParameterMetadataSection>,
    diagnostics: &mut Diagnostics,
    exceptable_nodes: &Option<&'static [SyntaxKind]>,
) {
    let decls_map: IndexMap<_, _> = decls
        .iter()
        .map(|decl| {
            (
                decl.name().text().to_string(),
                (
                    decl.name().span(),
                    decl.inner(),
                    decl.doc_comments().is_some_and(|docs| !docs.is_empty()),
                ),
            )
        })
        .collect();

    if param_meta.is_none()
        && decls_map
            .iter()
            .all(|(_, (_, _, has_doc_comments))| !has_doc_comments)
    {
        // Leave the case of no `parameter_meta` or doc comments for
        // `MetaSections`
        return;
    }

    let parameter_meta_map: IndexMap<_, _> =
        param_meta
            .as_ref()
            .map_or_else(IndexMap::default, |param_meta| {
                param_meta
                    .items()
                    .map(|m| {
                        let name = m.name();
                        (name.text().to_string(), name.span())
                    })
                    .collect()
            });

    // The suggestion depends on whatever we find first, a doc comment or a
    // `parameter_meta` entry
    let suggest_doc_comments = decls_map
        .iter()
        .find_map(|(name, (_, _, has_doc_comments))| {
            if *has_doc_comments {
                Some(true)
            } else if parameter_meta_map.contains_key(name) {
                Some(false)
            } else {
                None
            }
        })
        .unwrap_or(false);

    for (name, (span, decl_node, has_doc_comments)) in &decls_map {
        if !has_doc_comments && !parameter_meta_map.contains_key(name) {
            diagnostics.exceptable_add(
                missing_param_meta(parent, name, *span, suggest_doc_comments),
                param_meta
                    .as_ref()
                    .map_or(*decl_node, |param_meta| param_meta.inner()),
                exceptable_nodes,
            );
        }
    }

    let Some(param_meta) = param_meta else {
        return;
    };

    // We determine the intersection of expected and actual parameter names.
    // Using these we next check for missing and extraneous parameters
    // separately.
    let expected_order: Vec<_> = decls_map
        .iter()
        .filter_map(|(name, (_, _, has_doc_comments))| {
            if parameter_meta_map.contains_key(name) && !has_doc_comments {
                Some(name.to_string())
            } else {
                None
            }
        })
        .collect();

    let actual_order: Vec<_> = parameter_meta_map
        .keys()
        .filter(|name| {
            decls_map
                .get(&**name)
                .is_some_and(|(_, _, has_doc_comments)| !has_doc_comments)
        })
        .cloned()
        .collect();

    for (name, span) in &parameter_meta_map {
        if !decls_map.contains_key(name) {
            diagnostics.exceptable_add(
                extra_param_meta(parent, name, *span),
                param_meta.inner(),
                exceptable_nodes,
            );
        }
    }

    if expected_order != actual_order {
        let span = param_meta
            .inner()
            .first_token()
            .expect("must have parameter meta token")
            .text_range()
            .into();
        diagnostics.exceptable_add(
            mismatched_param_order(parent, span, &expected_order.join("\n")),
            param_meta.inner(),
            exceptable_nodes,
        );
    }
}

impl Visitor for ParameterMetaMatchedRule {
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

        // Note that only the first input and parameter_meta sections are
        // checked as any additional sections is considered a validation
        // error
        check_parameter_meta(
            &SectionParent::Task(task.clone()),
            task.input().iter().flat_map(|i| i.declarations()).collect(),
            task.parameter_metadata(),
            diagnostics,
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

        // Note that only the first input and parameter_meta sections are
        // checked as any additional sections is considered a validation
        // error
        check_parameter_meta(
            &SectionParent::Workflow(workflow.clone()),
            workflow
                .input()
                .iter()
                .flat_map(|i| i.declarations())
                .collect(),
            workflow.parameter_metadata(),
            diagnostics,
            &self.exceptable_nodes(),
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

        // Note that only the first input and parameter_meta sections are
        // checked as any additional sections is considered a validation
        // error
        check_parameter_meta(
            &SectionParent::Struct(def.clone()),
            def.members().map(Decl::Unbound).collect(),
            def.parameter_metadata().next(),
            diagnostics,
            &self.exceptable_nodes(),
        );
    }
}
