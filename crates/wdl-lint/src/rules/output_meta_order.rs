//! A lint rule to ensure `meta.outputs` follows output declaration order.

use wdl_analysis::Diagnostics;
use wdl_analysis::Example;
use wdl_analysis::LabeledSnippet;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Severity;
use wdl_ast::Span;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_ast::v1::MetadataSection;
use wdl_ast::v1::OutputSection;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::WorkflowDefinition;

use crate::Config;
use crate::Rule;
use crate::Tag;
use crate::TagSet;
use crate::rules::OutputMetaCollector;

/// The identifier for the output meta order rule.
const ID: &str = "OutputMetaOrder";

/// Creates a diagnostic for out-of-order entries.
fn out_of_order(
    severity: Severity,
    span: Span,
    output_span: Span,
    item_name: &str,
    ty: &str,
) -> Diagnostic {
    Diagnostic::new(
        severity,
        format!("`outputs` section of `meta` for the {ty} `{item_name}` is out of order"),
    )
    .with_rule(ID)
    .with_highlight(span)
    .with_highlight(output_span)
    .with_fix(
        "ensure the keys within `meta.outputs` have the same order as they appear in `output`",
    )
}

/// Detects out-of-order `meta.outputs` entries.
#[derive(Debug, Clone)]
pub struct OutputMetaOrderRule<'a> {
    /// The severity of the rule's diagnostics.
    severity: Severity,
    /// The collected output metadata state.
    collector: OutputMetaCollector<'a>,
}

impl<'a> OutputMetaOrderRule<'a> {
    /// Creates a new instance of the rule.
    pub fn new(config: &Config) -> Self {
        Self {
            severity: config.output_meta_order.diagnostic_severity(),
            collector: Default::default(),
        }
    }
}

impl Rule for OutputMetaOrderRule<'_> {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that `meta.outputs` keys are in the same order as output declarations."
    }

    fn explanation(&self) -> &'static str {
        "When outputs without doc comments are documented in meta.outputs, those metadata keys \
         should appear in the same order as their corresponding declarations in the output section."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.3

task ordered_outputs {
    meta {
        outputs: {
            second: "The second output",
            first: "The first output",
        }
    }

    output {
        String first = "one"
        String second = "two"
    }
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: None,
                snippet: r#"version 1.3

task ordered_outputs {
    meta {
        outputs: {
            first: "The first output",
            second: "The second output",
        }
    }

    output {
        String first = "one"
        String second = "two"
    }
}
"#,
            }),
        }]
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[
            Tag::Completeness,
            Tag::Documentation,
            Tag::SprocketCompatibility,
        ])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::TaskDefinitionNode,
            SyntaxKind::WorkflowDefinitionNode,
        ])
    }

    fn related_rules(&self) -> &'static [&'static str] {
        &["MatchingOutputMeta"]
    }
}

impl Visitor for OutputMetaOrderRule<'_> {
    fn reset(&mut self) {
        self.collector.reset();
    }

    fn workflow_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        workflow: &WorkflowDefinition,
    ) {
        match reason {
            VisitReason::Enter => {
                self.collector.enter_workflow(workflow);
            }
            VisitReason::Exit => check_output_order(diagnostics, self, workflow.inner()),
        }
    }

    fn task_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        task: &TaskDefinition,
    ) {
        match reason {
            VisitReason::Enter => {
                self.collector.enter_task(task);
            }
            VisitReason::Exit => check_output_order(diagnostics, self, task.inner()),
        }
    }

    fn metadata_section(
        &mut self,
        _diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &MetadataSection,
    ) {
        self.collector.metadata_section(reason, section);
    }

    fn output_section(
        &mut self,
        _diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &OutputSection,
    ) {
        self.collector.output_section(reason, section);
    }

    fn bound_decl(
        &mut self,
        _diagnostics: &mut Diagnostics,
        reason: VisitReason,
        decl: &wdl_ast::v1::BoundDecl,
    ) {
        self.collector.bound_decl(reason, decl);
    }

    fn metadata_object_item(
        &mut self,
        _diagnostics: &mut Diagnostics,
        reason: VisitReason,
        item: &wdl_ast::v1::MetadataObjectItem,
    ) {
        self.collector.metadata_object_item(reason, item, |_, _| {});
    }
}

/// Checks that `meta.outputs` order matches output declaration order.
fn check_output_order(
    diagnostics: &mut Diagnostics,
    rule: &mut OutputMetaOrderRule<'_>,
    node: &SyntaxNode,
) {
    let expected_order: Vec<_> = rule
        .collector
        .output_decls()
        .iter()
        .filter_map(|(name, info)| {
            if !info.has_doc_comments {
                Some(name.as_str())
            } else {
                None
            }
        })
        .collect();

    let actual_order: Vec<_> = rule
        .collector
        .meta_outputs_keys()
        .keys()
        .filter(|name| {
            rule.collector
                .output_decls()
                .get(*name)
                .is_some_and(|info| !info.has_doc_comments)
        })
        .map(String::as_str)
        .collect();

    if !expected_order.is_empty()
        && actual_order.len() == expected_order.len()
        && actual_order.iter().all(|key| expected_order.contains(key))
        && actual_order != expected_order
    {
        diagnostics.exceptable_add(
            out_of_order(
                rule.severity,
                rule.collector
                    .current_meta_outputs_span()
                    .expect("should have a `meta.outputs` span"),
                rule.collector
                    .current_output_span()
                    .expect("should have an `output` span"),
                rule.collector.name().expect("should have a name"),
                rule.collector.ty().expect("should have a type"),
            ),
            node,
            &rule.exceptable_nodes(),
        );
    }

    rule.collector.reset_current();
}
