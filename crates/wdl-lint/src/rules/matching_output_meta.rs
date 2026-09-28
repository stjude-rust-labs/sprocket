//! A lint rule to ensure each output is documented in `meta`.

use indexmap::IndexMap;
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
use wdl_ast::v1::MetadataSection;
use wdl_ast::v1::MetadataValue;
use wdl_ast::v1::OutputSection;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::WorkflowDefinition;

use crate::Config;
use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the non-matching output rule.
const ID: &str = "MatchingOutputMeta";

/// Creates a "non-matching output" diagnostic.
fn nonmatching_output(
    severity: Severity,
    span: Span,
    name: &str,
    item_name: &str,
    ty: &str,
) -> Diagnostic {
    Diagnostic::new(
        severity,
        format!("output `{name}` is missing from `meta.outputs` section in {ty} `{item_name}`"),
    )
    .with_rule(ID)
    .with_highlight(span)
    .with_fix(format!(
        "add a description of output `{name}` to documentation in `meta.outputs`"
    ))
}

/// Creates a missing outputs in meta diagnostic.
fn missing_outputs_in_meta(
    severity: Severity,
    span: Span,
    item_name: &str,
    ty: &str,
) -> Diagnostic {
    Diagnostic::new(
        severity,
        format!("`outputs` key missing in `meta` section for the {ty} `{item_name}`"),
    )
    .with_rule(ID)
    .with_highlight(span)
    .with_fix("add an `outputs` key to `meta` section describing the outputs")
}

/// Creates a diagnostic for extra `meta.outputs` entries.
fn extra_output_in_meta(
    severity: Severity,
    span: Span,
    name: &str,
    item_name: &str,
    ty: &str,
) -> Diagnostic {
    Diagnostic::new(
        severity,
        format!(
            "`{name}` appears in `outputs` section of the {ty} `{item_name}` but is not a \
             declared `output`"
        ),
    )
    .with_rule(ID)
    .with_highlight(span)
    .with_fix(format!(
        "ensure the output exists or remove the `{name}` key from `meta.outputs`"
    ))
}

/// Creates a diagnostic for non-object `meta.outputs` entries.
fn non_object_meta_outputs(
    severity: Severity,
    span: Span,
    item_name: &str,
    ty: &str,
) -> Diagnostic {
    Diagnostic::new(
        severity,
        format!(
            "{ty} `{item_name}` has a `meta.outputs` key that is not an object containing output \
             descriptions"
        ),
    )
    .with_rule(ID)
    .with_highlight(span)
    .with_fix("ensure `meta.outputs` is an object containing descriptions for each output")
}

/// Collects output declarations and `meta.outputs` entries for a task or
/// workflow.
#[derive(Debug, Clone, Default)]
pub(crate) struct OutputMetaCollector<'a> {
    /// The span of the `meta` section.
    current_meta_span: Option<Span>,
    /// Are we currently within a `meta` section?
    in_meta: bool,
    /// The span of the `meta.outputs` section.
    current_meta_outputs_span: Option<Span>,
    /// The span of the `output` section.
    current_output_span: Option<Span>,
    /// Are we currently within an `output` section?
    in_output: bool,
    /// The keys seen in `meta.outputs`.
    meta_outputs_keys: IndexMap<String, Span>,
    /// The keys seen in `output`.
    output_keys: IndexMap<String, Span>,
    /// The context type.
    ty: Option<&'a str>,
    /// The item name.
    name: Option<String>,
    /// Prior metadata objects.
    prior_objects: Vec<String>,
}

impl<'a> OutputMetaCollector<'a> {
    /// Resets all collector state.
    pub(crate) fn reset(&mut self) {
        self.current_meta_span = None;
        self.in_meta = false;
        self.current_meta_outputs_span = None;
        self.current_output_span = None;
        self.in_output = false;
        self.meta_outputs_keys.clear();
        self.output_keys.clear();
        self.ty = None;
        self.name = None;
        self.prior_objects.clear();
    }

    /// Resets state for the current task or workflow.
    pub(crate) fn reset_current(&mut self) {
        self.current_meta_span = None;
        self.current_meta_outputs_span = None;
        self.current_output_span = None;
        self.output_keys.clear();
        self.meta_outputs_keys.clear();
        self.name = None;
        self.ty = None;
        self.prior_objects.clear();
    }

    /// Enters a workflow definition.
    pub(crate) fn enter_workflow(&mut self, workflow: &WorkflowDefinition) {
        self.name = Some(workflow.name().text().to_string());
        self.ty = Some("workflow");
    }

    /// Enters a task definition.
    pub(crate) fn enter_task(&mut self, task: &TaskDefinition) {
        self.name = Some(task.name().text().to_string());
        self.ty = Some("task");
    }

    /// Gets the span of the `meta` section.
    pub(crate) fn current_meta_span(&self) -> Option<Span> {
        self.current_meta_span
    }

    /// Gets the span of the `meta.outputs` section.
    pub(crate) fn current_meta_outputs_span(&self) -> Option<Span> {
        self.current_meta_outputs_span
    }

    /// Gets the span of the `output` section.
    pub(crate) fn current_output_span(&self) -> Option<Span> {
        self.current_output_span
    }

    /// Gets the keys seen in `meta.outputs`.
    pub(crate) fn meta_outputs_keys(&self) -> &IndexMap<String, Span> {
        &self.meta_outputs_keys
    }

    /// Gets the keys seen in `output`.
    pub(crate) fn output_keys(&self) -> &IndexMap<String, Span> {
        &self.output_keys
    }

    /// Gets the item name.
    pub(crate) fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Gets the context type.
    pub(crate) fn ty(&self) -> Option<&'a str> {
        self.ty
    }

    /// Visits a metadata section.
    pub(crate) fn metadata_section(&mut self, reason: VisitReason, section: &MetadataSection) {
        match reason {
            VisitReason::Enter => {
                self.current_meta_span = Some(
                    section
                        .inner()
                        .first_token()
                        .expect("metadata section should have tokens")
                        .text_range()
                        .into(),
                );
                self.in_meta = true;
            }
            VisitReason::Exit => {
                self.in_meta = false;
            }
        }
    }

    /// Visits an output section.
    pub(crate) fn output_section(&mut self, reason: VisitReason, section: &OutputSection) {
        match reason {
            VisitReason::Enter => {
                self.current_output_span = Some(
                    section
                        .inner()
                        .first_token()
                        .expect("output section should have tokens")
                        .text_range()
                        .into(),
                );
                self.in_output = true;
            }
            VisitReason::Exit => {
                self.in_output = false;
            }
        }
    }

    /// Visits an output declaration.
    pub(crate) fn bound_decl(&mut self, reason: VisitReason, decl: &wdl_ast::v1::BoundDecl) {
        if reason == VisitReason::Enter && self.in_output {
            self.output_keys
                .insert(decl.name().text().to_string(), decl.name().span());
        }
    }

    /// Visits a metadata object item.
    pub(crate) fn metadata_object_item(
        &mut self,
        reason: VisitReason,
        item: &wdl_ast::v1::MetadataObjectItem,
        mut report_non_object_outputs: impl FnMut(Span, &SyntaxNode),
    ) {
        if !self.in_meta {
            return;
        }

        match reason {
            VisitReason::Exit => {
                if let MetadataValue::Object(_) = item.value() {
                    self.prior_objects.pop();
                }
            }
            VisitReason::Enter => {
                if self.current_meta_span.is_some() && item.name().text() == "outputs" {
                    self.current_meta_outputs_span = Some(item.span());
                    if !matches!(item.value(), MetadataValue::Object(_)) {
                        report_non_object_outputs(item.span(), item.inner());
                    }
                } else if let Some(meta_outputs_span) = self.current_meta_outputs_span {
                    let span = item.span();
                    if span.start() > meta_outputs_span.start()
                        && span.end() < meta_outputs_span.end()
                        && self
                            .prior_objects
                            .last()
                            .expect("should have seen `meta.outputs`")
                            == "outputs"
                    {
                        self.meta_outputs_keys
                            .insert(item.name().text().to_string(), item.span());
                    }
                }

                if let MetadataValue::Object(_) = item.value() {
                    self.prior_objects.push(item.name().text().to_string());
                }
            }
        }
    }
}

/// Detects non-matching outputs.
#[derive(Debug, Clone)]
pub struct MatchingOutputMetaRule<'a> {
    /// The severity of the rule's diagnostics.
    severity: Severity,
    /// The collected output metadata state.
    collector: OutputMetaCollector<'a>,
}

impl<'a> MatchingOutputMetaRule<'a> {
    /// Creates a new instance of the rule.
    pub fn new(config: &Config) -> Self {
        Self {
            severity: config.matching_output_meta.diagnostic_severity(),
            collector: Default::default(),
        }
    }
}

impl Rule for MatchingOutputMetaRule<'_> {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that each output field is documented in the meta section under `meta.outputs`."
    }

    fn explanation(&self) -> &'static str {
        "The meta section should have an `outputs` key that is an object and contains keys with \
         descriptions for each output of the task/workflow. These must match exactly. i.e. for \
         each named output of a task or workflow, there should be an entry under `meta.outputs` \
         with that same name. No extraneous `meta.outputs` entries are allowed."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.2

task generate_greeting {
    meta {
        outputs: {
        # Missing `greeting`
        }
    }

    input {
        String name
    }

    output {
        String greeting = "Hello, ~{name}!"
    }
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: None,
                snippet: r#"version 1.2

task generate_greeting {
    meta {
        outputs: {
            greeting: "The generated greeting for the provided name",
        }
    }

    input {
        String name
    }

    output {
        String greeting = "Hello, ~{name}!"
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
        &[
            "MetaDescription",
            "MissingParameterMeta",
            "OutputSection",
            "RequirementsSection",
            "RuntimeSection",
            "DescriptionLength",
            "OutputMetaOrder",
        ]
    }
}

/// Check each output key exists in the `outputs` key within the `meta` section.
fn check_matching(
    diagnostics: &mut Diagnostics,
    rule: &mut MatchingOutputMetaRule<'_>,
    node: &SyntaxNode,
) {
    // Check for expected entries missing from `meta.outputs`.
    for (name, span) in rule.collector.output_keys() {
        if !rule.collector.meta_outputs_keys().contains_key(name)
            && rule.collector.current_meta_span().is_some()
        {
            diagnostics.exceptable_add(
                nonmatching_output(
                    rule.severity,
                    *span,
                    name,
                    rule.collector.name().expect("should have a name"),
                    rule.collector.ty().expect("should have a type"),
                ),
                node,
                &rule.exceptable_nodes(),
            );
        }
    }

    // Check for extra entries in `meta.outputs`.
    // This should flag any meta.outputs entry that doesn't have a corresponding
    // declared output, even if the output section is entirely missing.
    for (name, span) in rule.collector.meta_outputs_keys() {
        if !rule.collector.output_keys().contains_key(name) {
            diagnostics.exceptable_add(
                extra_output_in_meta(
                    rule.severity,
                    *span,
                    name,
                    rule.collector.name().expect("should have a name"),
                    rule.collector.ty().expect("should have a type"),
                ),
                node,
                &rule.exceptable_nodes(),
            );
        }
    }
}

/// Handle missing `meta.outputs` and reset the visitor.
fn handle_meta_outputs_and_reset(
    diagnostics: &mut Diagnostics,
    rule: &mut MatchingOutputMetaRule<'_>,
    node: &SyntaxNode,
) {
    if let Some(current_meta_span) = rule.collector.current_meta_span()
        && rule.collector.current_meta_outputs_span().is_none()
        && !rule.collector.output_keys().is_empty()
    {
        diagnostics.exceptable_add(
            missing_outputs_in_meta(
                rule.severity,
                current_meta_span,
                rule.collector.name().expect("should have a name"),
                rule.collector.ty().expect("should have a type"),
            ),
            node,
            &rule.exceptable_nodes(),
        );
    } else {
        check_matching(diagnostics, rule, node);
    }

    rule.collector.reset_current();
}

impl Visitor for MatchingOutputMetaRule<'_> {
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
            VisitReason::Exit => {
                handle_meta_outputs_and_reset(diagnostics, self, workflow.inner());
            }
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
            VisitReason::Exit => {
                handle_meta_outputs_and_reset(diagnostics, self, task.inner());
            }
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
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        item: &wdl_ast::v1::MetadataObjectItem,
    ) {
        let severity = self.severity;
        let name = self.collector.name().map(str::to_string);
        let ty = self.collector.ty();
        let exceptable_nodes = self.exceptable_nodes();
        self.collector
            .metadata_object_item(reason, item, |span, node| {
                diagnostics.exceptable_add(
                    non_object_meta_outputs(
                        severity,
                        span,
                        name.as_deref().expect("should have a name"),
                        ty.expect("should have a type"),
                    ),
                    node,
                    &exceptable_nodes,
                );
            });
    }
}
