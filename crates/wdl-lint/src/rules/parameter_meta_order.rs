//! A lint rule for parameter metadata ordering.

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

/// The identifier for the parameter meta order rule.
const ID: &str = "ParameterMetaOrder";

/// Creates a "mismatched order" diagnostic.
///
/// # Note on Relative Direction Invariants
/// Because `i` is the first index where `actual_order[i] != expected_order[i]`,
/// and all earlier positions `j < i` match identically, `expected_name` has not
/// yet been encountered in `actual_order`. Therefore:
/// - In `parameter_meta`, `expected_name` is guaranteed to appear strictly
///   after `actual_name` in document order.
/// - In declarations, `expected_name` is guaranteed to be declared strictly
///   before `actual_name`.
#[allow(clippy::too_many_arguments)]
fn mismatched_param_order(
    severity: Severity,
    parent: &SectionParent,
    actual_span: Span,
    actual_name: &str,
    expected_meta_span: Span,
    expected_name: &str,
    decl_span: Span,
    expected_order: &str,
) -> Diagnostic {
    let (context, decl_type, parent) = match parent {
        SectionParent::Task(t) => ("task", "input", t.name()),
        SectionParent::Workflow(w) => ("workflow", "input", w.name()),
        SectionParent::Struct(s) => ("struct", "field", s.name()),
    };

    Diagnostic::new(
        severity,
        format!(
            "parameter metadata in {context} `{parent}` is out of order",
            parent = parent.text(),
        ),
    )
    .with_rule(ID)
    .with_label(
        format!(
            "expected parameter metadata for `{expected_name}` here, but found `{actual_name}`"
        ),
        actual_span,
    )
    .with_label(
        format!("parameter metadata for `{expected_name}` is defined after `{actual_name}` here"),
        expected_meta_span,
    )
    .with_label(
        format!("{decl_type} `{expected_name}` is declared before `{actual_name}`"),
        decl_span,
    )
    .with_fix(format!(
        "based on the current `{decl_type}` order, order the parameter metadata \
         as:\n{expected_order}"
    ))
}

/// Detects out-of-order entries in a `parameter_meta` section.
#[derive(Debug, Clone, Copy)]
pub struct ParameterMetaOrderRule {
    /// The severity of the rule's diagnostics.
    severity: Severity,
    /// The version of the WDL document being linted.
    version: Option<SupportedVersion>,
}

impl ParameterMetaOrderRule {
    /// Creates a new instance of the rule.
    pub fn new(config: &Config) -> Self {
        Self {
            severity: config.parameter_meta_order.diagnostic_severity(),
            version: None,
        }
    }
}

impl Rule for ParameterMetaOrderRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that `parameter_meta` keys follow input declaration order."
    }

    fn explanation(&self) -> &'static str {
        "When inputs or struct fields are documented in `parameter_meta`, the metadata keys should \
         appear in the same order as the declarations they document."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.3

task say_hello {
    parameter_meta {
        punctuation: "The punctuation to use"
        name: "The name of the person to greet"
    }

    input {
        String name
        String punctuation = "!"
    }

    command <<<
        echo "Hello, ~{name}~{punctuation}"
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
        punctuation: "The punctuation to use"
    }

    input {
        String name
        String punctuation = "!"
    }

    command <<<
        echo "Hello, ~{name}~{punctuation}"
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
        &["MissingParameterMeta", "ExtraneousParameterMeta"]
    }
}

impl Visitor for ParameterMetaOrderRule {
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

/// Checks the order of items in a `parameter_meta` section.
fn check_parameter_meta(
    severity: Severity,
    context: ParameterMetaContext,
    diagnostics: &mut Diagnostics,
    exceptable_nodes: &Option<&'static [SyntaxKind]>,
) {
    let Some(param_meta) = context.parameter_meta else {
        return;
    };

    let expected_order: Vec<_> = context
        .decls
        .iter()
        .filter_map(|(name, info)| {
            if context.parameter_meta_map.contains_key(name) && !info.has_doc_comments {
                Some(name.to_string())
            } else {
                None
            }
        })
        .collect();

    let actual_order: Vec<_> = context
        .parameter_meta_map
        .keys()
        .filter(|name| {
            context
                .decls
                .get(&**name)
                .is_some_and(|info| !info.has_doc_comments)
        })
        .cloned()
        .collect();

    if expected_order != actual_order {
        let mismatch_idx = actual_order
            .iter()
            .zip(expected_order.iter())
            .position(|(actual, expected)| actual != expected)
            .expect("orders differ");

        let actual_name = &actual_order[mismatch_idx];
        let expected_name = &expected_order[mismatch_idx];

        let actual_span = *context
            .parameter_meta_map
            .get(actual_name)
            .expect("must have actual key span");
        let expected_meta_span = *context
            .parameter_meta_map
            .get(expected_name)
            .expect("must have expected key span in parameter_meta");
        let decl_span = context
            .decls
            .get(expected_name)
            .expect("must have expected key declaration")
            .span;

        diagnostics.exceptable_add(
            mismatched_param_order(
                severity,
                &context.parent,
                actual_span,
                actual_name,
                expected_meta_span,
                expected_name,
                decl_span,
                &expected_order.join("\n"),
            ),
            param_meta.inner(),
            exceptable_nodes,
        );
    }
}
