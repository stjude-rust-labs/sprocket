//! A lint rule to ensure each output is documented in `meta`.

use indexmap::IndexMap;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;
use wdl_ast::v1::MetadataSection;
use wdl_ast::v1::MetadataValue;
use wdl_ast::v1::OutputSection;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::WorkflowDefinition;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the non-matching output rule.
const ID: &str = "NonmatchingOutput";

/// Creates a "non-matching output" diagnostic.
fn nonmatching_output(span: Span, name: &str, item_name: &str, ty: &str) -> Diagnostic {
    Diagnostic::warning(format!(
        "output `{name}` is missing from `meta.outputs` section in {ty} `{item_name}`"
    ))
    .with_rule(ID)
    .with_highlight(span)
    .with_fix(format!(
        "add a description of output `{name}` to documentation in `meta.outputs`"
    ))
}

/// Creates a missing outputs in meta diagnostic.
fn missing_outputs_in_meta(span: Span, item_name: &str, ty: &str) -> Diagnostic {
    Diagnostic::warning(format!(
        "`outputs` key missing in `meta` section for the {ty} `{item_name}`"
    ))
    .with_rule(ID)
    .with_highlight(span)
    .with_fix("add an `outputs` key to `meta` section describing the outputs")
}

/// Creates a diagnostic for extra `meta.outputs` entries.
fn extra_output_in_meta(span: Span, name: &str, item_name: &str, ty: &str) -> Diagnostic {
    Diagnostic::warning(format!(
        "`{name}` appears in `outputs` section of the {ty} `{item_name}` but is not a declared \
         `output`"
    ))
    .with_rule(ID)
    .with_highlight(span)
    .with_fix(format!(
        "ensure the output exists or remove the `{name}` key from `meta.outputs`"
    ))
}

/// Creates a diagnostic for out-of-order entries.
fn out_of_order(span: Span, output_span: Span, item_name: &str, ty: &str) -> Diagnostic {
    Diagnostic::note(format!(
        "`outputs` section of `meta` for the {ty} `{item_name}` is out of order"
    ))
    .with_rule(ID)
    .with_highlight(span)
    .with_highlight(output_span)
    .with_fix(
        "ensure the keys within `meta.outputs` have the same order as they appear in `output`",
    )
}

/// Creates a diagnostic for non-object `meta.outputs` entries.
fn non_object_meta_outputs(span: Span, item_name: &str, ty: &str) -> Diagnostic {
    Diagnostic::warning(format!(
        "{ty} `{item_name}` has a `meta.outputs` key that is not an object containing output \
         descriptions"
    ))
    .with_rule(ID)
    .with_highlight(span)
    .with_fix("ensure `meta.outputs` is an object containing descriptions for each output")
}

/// Detects non-matching outputs.
#[derive(Default, Debug, Clone)]
pub struct NonmatchingOutputRule<'a> {
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
    /// Prior objects
    prior_objects: Vec<String>,
}

impl Rule for NonmatchingOutputRule<'_> {
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
         with that same name. Additionally, these entries should be in the same order (that order \
         is up to the developer to decide). No extraneous `meta.outputs` entries are allowed."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Completeness])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::TaskDefinitionNode,
            SyntaxKind::WorkflowDefinitionNode,
        ])
    }
}

/// Check each output key exists in the `outputs` key within the `meta` section.
fn check_matching(
    state: &mut Diagnostics,
    rule: &mut NonmatchingOutputRule<'_>,
    element: SyntaxElement,
) {
    let mut exact_match = true;
    // Check for expected entries missing from `meta.outputs`.
    for (name, span) in &rule.output_keys {
        if !rule.meta_outputs_keys.contains_key(name) {
            exact_match = false;
            if rule.current_meta_span.is_some() {
                state.exceptable_add(
                    nonmatching_output(
                        *span,
                        name,
                        rule.name.as_deref().expect("should have a name"),
                        rule.ty.expect("should have a type"),
                    ),
                    element.clone(),
                    &rule.exceptable_nodes(),
                );
            }
        }
    }

    // Check for extra entries in `meta.outputs`.
    for (name, span) in &rule.meta_outputs_keys {
        if !rule.output_keys.contains_key(name) {
            exact_match = false;
            if rule.current_output_span.is_some() {
                state.exceptable_add(
                    extra_output_in_meta(
                        *span,
                        name,
                        rule.name.as_deref().expect("should have a name"),
                        rule.ty.expect("should have a type"),
                    ),
                    element.clone(),
                    &rule.exceptable_nodes(),
                );
            }
        }
    }

    // Check for out-of-order entries.
    if exact_match && !rule.meta_outputs_keys.keys().eq(rule.output_keys.keys()) {
        state.exceptable_add(
            out_of_order(
                rule.current_meta_outputs_span
                    .expect("should have a `meta.outputs` span"),
                rule.current_output_span
                    .expect("should have an `output` span"),
                rule.name.as_deref().expect("should have a name"),
                rule.ty.expect("should have a type"),
            ),
            element,
            &rule.exceptable_nodes(),
        );
    }
}

/// Handle missing `meta.outputs` and reset the visitor.
fn handle_meta_outputs_and_reset(
    state: &mut Diagnostics,
    rule: &mut NonmatchingOutputRule<'_>,
    element: SyntaxElement,
) {
    if rule.current_meta_span.is_some()
        && rule.current_meta_outputs_span.is_none()
        && !rule.output_keys.is_empty()
    {
        state.exceptable_add(
            missing_outputs_in_meta(
                rule.current_meta_span.expect("should have a `meta` span"),
                rule.name.as_deref().expect("should have a name"),
                rule.ty.expect("should have a type"),
            ),
            element,
            &rule.exceptable_nodes(),
        );
    } else {
        check_matching(state, rule, element);
    }

    rule.name = None;
    rule.current_meta_outputs_span = None;
    rule.current_meta_span = None;
    rule.current_output_span = None;
    rule.output_keys.clear();
    rule.meta_outputs_keys.clear();
}

impl Visitor for NonmatchingOutputRule<'_> {
    type State = Diagnostics;

    fn document(
        &mut self,
        _: &mut Self::State,
        reason: VisitReason,
        _: &Document,
        _: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Reset the visitor upon document entry
        *self = Default::default();
    }

    fn workflow_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        workflow: &WorkflowDefinition,
    ) {
        match reason {
            VisitReason::Enter => {
                self.name = Some(workflow.name().text().to_string());
                self.ty = Some("workflow");
            }
            VisitReason::Exit => {
                handle_meta_outputs_and_reset(
                    state,
                    self,
                    SyntaxElement::from(workflow.inner().clone()),
                );
            }
        }
    }

    fn task_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        task: &TaskDefinition,
    ) {
        match reason {
            VisitReason::Enter => {
                self.name = Some(task.name().text().to_string());
                self.ty = Some("task");
            }
            VisitReason::Exit => {
                handle_meta_outputs_and_reset(
                    state,
                    self,
                    SyntaxElement::from(task.inner().clone()),
                );
            }
        }
    }

    fn metadata_section(
        &mut self,
        _state: &mut Self::State,
        reason: VisitReason,
        section: &MetadataSection,
    ) {
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

    fn output_section(
        &mut self,
        _state: &mut Self::State,
        reason: VisitReason,
        section: &OutputSection,
    ) {
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

    fn bound_decl(
        &mut self,
        _state: &mut Self::State,
        reason: VisitReason,
        decl: &wdl_ast::v1::BoundDecl,
    ) {
        if reason == VisitReason::Enter && self.in_output {
            self.output_keys
                .insert(decl.name().text().to_string(), decl.name().span());
        }
    }

    fn metadata_object_item(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        item: &wdl_ast::v1::MetadataObjectItem,
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
                if let Some(_meta_span) = self.current_meta_span {
                    if item.name().text() == "outputs" {
                        self.current_meta_outputs_span = Some(item.span());
                        match item.value() {
                            MetadataValue::Object(_) => {}
                            _ => {
                                state.exceptable_add(
                                    non_object_meta_outputs(
                                        item.span(),
                                        self.name.as_deref().expect("should have a name"),
                                        self.ty.expect("should have a type"),
                                    ),
                                    SyntaxElement::from(item.inner().clone()),
                                    &self.exceptable_nodes(),
                                );
                            }
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
                }
                if let MetadataValue::Object(_) = item.value() {
                    self.prior_objects.push(item.name().text().to_string());
                }
            }
        }
    }
}
