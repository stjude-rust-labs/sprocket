//! A lint rule that disallows mixing documentation sources.

use std::collections::HashMap;
use std::fmt::Display;
use std::fmt::Formatter;

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use strum::VariantArray;
use toml_spanner::Toml;
use wdl_analysis::Diagnostics;
use wdl_analysis::Example;
use wdl_analysis::LabeledSnippet;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Documented;
use wdl_ast::Ident;
use wdl_ast::Severity;
use wdl_ast::Span;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_ast::v1::BoundDecl;
use wdl_ast::v1::Decl;
use wdl_ast::v1::MetadataObjectItem;
use wdl_ast::v1::MetadataSection;
use wdl_ast::v1::MetadataValue;
use wdl_ast::v1::ParameterMetadataSection;
use wdl_ast::v1::StructDefinition;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::WorkflowDefinition;

use crate::Config;
use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The behavior of the [`MixedDocSourcesRule`].
#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    Toml,
    VariantArray,
    JsonSchema,
)]
#[serde(rename_all = "lowercase")]
#[toml(FromToml, ToToml, rename_all = "lowercase")]
pub enum DocSourceAdherence {
    /// The workspace should only use doc comments. Any documentation in
    /// `meta`/`parameter_meta` sections will be flagged.
    Comments,
    /// The workspace should only use `meta`/`parameter_meta` sections. Any doc
    /// comments will be unconditionally flagged.
    Meta,
    /// The workspace can use doc comments _or_ `meta`/`parameter_meta`. Any
    /// case of a doc comment overwriting a meta value will be flagged.
    #[default]
    Consistent,
}

/// The metadata section where a parameter may be documented.
#[derive(Copy, Clone, Debug)]
enum Section {
    /// A `meta.outputs` section.
    MetaOutputs,
    /// A `parameter_meta` section.
    ParameterMeta,
}

impl Display for Section {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MetaOutputs => f.write_str("meta.outputs"),
            Self::ParameterMeta => f.write_str("parameter_meta"),
        }
    }
}

/// The type of an [`Item`].
#[derive(Copy, Clone, Debug)]
enum ItemKind {
    /// A task.
    Task,
    /// A workflow.
    Workflow,
    /// A struct.
    Struct,
}

impl Display for ItemKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ItemKind::Task => f.write_str("task"),
            ItemKind::Workflow => f.write_str("workflow"),
            ItemKind::Struct => f.write_str("struct"),
        }
    }
}

/// The type of a parameter.
#[derive(Copy, Clone, Debug)]
enum ParameterKind {
    /// An input.
    Input,
    /// An output.
    Output,
    /// A struct field.
    Field,
}

impl Display for ParameterKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ParameterKind::Input => f.write_str("input"),
            ParameterKind::Output => f.write_str("output"),
            ParameterKind::Field => f.write_str("field"),
        }
    }
}

/// A documentable entity.
enum Documentable {
    /// A top-level item.
    Item(ItemKind),
    /// A parameter of an item.
    Parameter(ParameterKind),
}

impl Display for Documentable {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Documentable::Item(kind) => kind.fmt(f),
            Documentable::Parameter(kind) => kind.fmt(f),
        }
    }
}

/// Encountered an item with doc comments in a [`DocSourceAdherence::Meta`]
/// workspace.
fn description_meta_only(severity: Severity, kind: ItemKind, name: &str, span: Span) -> Diagnostic {
    Diagnostic::new(
        severity,
        format!("{kind} `{name}` is documented with doc comments"),
    )
    .with_rule(ID)
    .with_highlight(span)
    .with_help("this workspace is configured to only allow `meta` sections")
    .with_fix("split the doc comment into the `meta.description`/`meta.help` fields")
}

/// Encountered a parameter with doc comments in a [`DocSourceAdherence::Meta`]
/// workspace.
fn parameter_meta_only(
    severity: Severity,
    section: Section,
    kind: ParameterKind,
    name: &str,
    span: Span,
) -> Diagnostic {
    Diagnostic::new(
        severity,
        format!("{kind} `{name}` is documented with doc comments"),
    )
    .with_rule(ID)
    .with_highlight(span)
    .with_help("this workspace is configured to only allow metadata sections")
    .with_fix(format!(
        "split the doc comment into the `{section}.{name}.description`/`{section}.{name}.help` \
         fields"
    ))
}

/// Encountered an item documented with a `meta` section in a
/// [`DocSourceAdherence::Comments`] workspace.
fn description_comment_only(
    severity: Severity,
    kind: ItemKind,
    name: &str,
    span: Span,
    description: Option<Span>,
    help: Option<Span>,
) -> Diagnostic {
    let mut diag = Diagnostic::new(
        severity,
        format!("{kind} `{name}` is documented with a `meta` section"),
    )
    .with_rule(ID)
    .with_highlight(span)
    .with_help("this workspace is configured to only allow doc comments")
    .with_fix("move the contents of `meta` into a doc comment");

    if let Some(description) = description {
        diag = diag.with_label("`description` defined here", description);
    }

    if let Some(help) = help {
        diag = diag.with_label("`help` defined here", help);
    }

    diag
}

/// Encountered a parameter documented with a metadata section in a
/// [`DocSourceAdherence::Comments`] workspace.
fn parameter_comment_only(
    severity: Severity,
    section: Section,
    kind: ParameterKind,
    name: &str,
    span: Span,
) -> Diagnostic {
    Diagnostic::new(
        severity,
        format!("{kind} `{name}` is documented with a `{section}` section"),
    )
    .with_rule(ID)
    .with_highlight(span)
    .with_help("this workspace is configured to only allow doc comments")
    .with_fix(format!(
        "move the contents of `{section}.{name}` into a doc comment"
    ))
}

/// Encountered an item with both doc comments and a
/// `meta.description`/`meta.help`/`parameter_meta.*` field.
fn conflict(
    severity: Severity,
    kind: Documentable,
    name: &str,
    span: Span,
    comment: Span,
    meta_keys: impl IntoIterator<Item = Span>,
) -> Diagnostic {
    let mut diag = Diagnostic::new(
        severity,
        format!("{kind} `{name}` has multiple documentation sources"),
    )
    .with_rule(ID)
    .with_highlight(span)
    .with_label("this comment...", comment);

    for meta_key in meta_keys {
        diag = diag.with_label("...overwrites this meta key", meta_key);
    }

    diag.with_fix("remove one of the documentation sources")
}

/// The identifier for the mixed doc sources rule.
const ID: &str = "MixedDocSources";

/// Spans for a documented parameter (name span and first comment line span).
#[derive(Clone, Copy, Debug)]
struct ParamSpans {
    /// The span of the parameter's name identifier.
    name: Span,
    /// The span of the first line of the parameter's doc comment.
    comment: Span,
}

/// Documentation comments found on an item.
#[derive(Default, Debug, Clone)]
struct ItemDocComments {
    /// The span of the current item's first doc comment line.
    top_level: Option<Span>,
    /// The spans of the item's inputs/fields.
    inputs: HashMap<String, ParamSpans>,
    /// The spans of the item's outputs.
    outputs: HashMap<String, ParamSpans>,
}

/// An item currently being visited.
#[derive(Debug, Clone)]
struct Item {
    /// The type of the item.
    kind: ItemKind,
    /// The item's name.
    name: String,
    /// The span of the item's name.
    name_span: Span,
    /// The doc comments found on the current item, if any.
    doc_comments: Option<ItemDocComments>,
}

/// A lint rule for mixed documentation sources.
#[derive(Debug, Clone)]
pub struct MixedDocSourcesRule {
    /// The severity of the rule's diagnostics.
    severity: Severity,
    /// The user config for preferred doc sources.
    adherence: DocSourceAdherence,
    /// The item currently being linted.
    current_item: Option<Item>,
}

impl MixedDocSourcesRule {
    /// Create a new `MixedDocSources` rule.
    pub fn new(config: &Config) -> Self {
        Self {
            severity: config.mixed_doc_sources.diagnostic_severity(),
            adherence: config.mixed_doc_sources.doc_source_adherence,
            current_item: None,
        }
    }
}

impl Rule for MixedDocSourcesRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that items use doc comments and `meta`/`parameter_meta` sections consistently."
    }

    fn explanation(&self) -> &'static str {
        "Sprocket supports two documentation sources: doc comments and `meta`/`parameter_meta` \
         sections. For clarity, one source should be chosen and used consistently throughout the \
         project. If both are present for a single item/input/output, doc comments will always \
         take precedence."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.3

## Generates a greeting
task generate_greeting {
    input {
        ## The name of the person to greet
        String name
    }

    meta {
        description: "Generates a greeting"
    }

    parameter_meta {
        name: "The name of the person to greet"
    }

    command <<<
    >>>

    output {
        String output_greeting = "Hello, ~{name}!"
    }
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: Some("Consider dropping one of the documentation sources"),
                snippet: r#"version 1.3

## Generates a greeting
task generate_greeting {
    input {
        ## The name of the person to greet
        String name
    }

    command <<<
    >>>

    output {
        String output_greeting = "Hello, ~{name}!"
    }
}
"#,
            }),
        }]
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Documentation, Tag::Clarity])
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::TaskDefinitionNode,
            SyntaxKind::WorkflowDefinitionNode,
            SyntaxKind::StructDefinitionNode,
            SyntaxKind::MetadataSectionNode,
            SyntaxKind::ParameterMetadataSectionNode,
        ])
    }

    fn related_rules(&self) -> &'static [&'static str] {
        &["MatchingOutputMeta", "MetaDescription", "MetaSections"]
    }
}

/// Extracts the documentation spans for a metadata item.
///
/// This checks for:
///
/// * A string value.
/// * An object value with a string `description` and/or `help` field.
fn meta_item_doc_spans(item: &MetadataObjectItem) -> Vec<Span> {
    match item.value() {
        MetadataValue::String(_) => vec![item.span()],
        MetadataValue::Object(obj) => obj
            .items()
            .filter_map(|obj_item| {
                let name = obj_item.name();
                let key = name.text();
                if (key == "description" || key == "help")
                    && matches!(obj_item.value(), MetadataValue::String(_))
                {
                    Some(obj_item.span())
                } else {
                    None
                }
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Collects parameters documented with doc comments into a map of name ->
/// spans.
fn collect_documented_params(decls: impl Iterator<Item = Decl>) -> HashMap<String, ParamSpans> {
    let mut documented = HashMap::new();
    for decl in decls {
        let Some(comments) = decl.doc_comments() else {
            continue;
        };
        let Some(first_comment) = comments.first() else {
            continue;
        };

        let name = decl.name();
        documented.insert(
            name.text().to_string(),
            ParamSpans {
                name: name.span(),
                comment: first_comment.span(),
            },
        );
    }
    documented
}

impl MixedDocSourcesRule {
    /// Validates that an item and its declarations do not have doc comments in
    /// [`DocSourceAdherence::Meta`] mode.
    #[allow(clippy::too_many_arguments)]
    fn validate_meta_adherence(
        &self,
        diagnostics: &mut Diagnostics,
        kind: ItemKind,
        name: Ident,
        node: &impl Documented<SyntaxNode>,
        param_kind: ParameterKind,
        inputs: impl Iterator<Item = Decl>,
        outputs: impl Iterator<Item = BoundDecl>,
    ) {
        if let Some(docs) = node.doc_comments()
            && !docs.is_empty()
        {
            diagnostics.exceptable_add(
                description_meta_only(self.severity, kind, name.text(), name.span()),
                node.inner(),
                &self.exceptable_nodes(),
            );
        }

        let input_section = match param_kind {
            ParameterKind::Output => Section::MetaOutputs,
            ParameterKind::Input | ParameterKind::Field => Section::ParameterMeta,
        };

        for input in inputs {
            if let Some(docs) = input.doc_comments()
                && !docs.is_empty()
            {
                let input_name = input.name();
                diagnostics.exceptable_add(
                    parameter_meta_only(
                        self.severity,
                        input_section,
                        param_kind,
                        input_name.text(),
                        input_name.span(),
                    ),
                    node.inner(),
                    &self.exceptable_nodes(),
                );
            }
        }

        for output in outputs {
            if let Some(docs) = output.doc_comments()
                && !docs.is_empty()
            {
                let output_name = output.name();
                diagnostics.exceptable_add(
                    parameter_meta_only(
                        self.severity,
                        Section::MetaOutputs,
                        ParameterKind::Output,
                        output_name.text(),
                        output_name.span(),
                    ),
                    node.inner(),
                    &self.exceptable_nodes(),
                );
            }
        }
    }

    /// Check if the current node has any doc comments.
    #[allow(clippy::too_many_arguments)]
    fn check_doc_comments(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        kind: ItemKind,
        name: Ident,
        node: &impl Documented<SyntaxNode>,
        param_kind: ParameterKind,
        inputs: impl Iterator<Item = Decl>,
        outputs: impl Iterator<Item = BoundDecl>,
    ) {
        match reason {
            VisitReason::Enter => match self.adherence {
                DocSourceAdherence::Meta => {
                    self.validate_meta_adherence(
                        diagnostics,
                        kind,
                        name,
                        node,
                        param_kind,
                        inputs,
                        outputs,
                    );
                }
                DocSourceAdherence::Comments => {
                    self.current_item = Some(Item {
                        kind,
                        name: name.text().to_string(),
                        name_span: name.span(),
                        doc_comments: None,
                    });
                }
                DocSourceAdherence::Consistent => {
                    let top_level_docs = node
                        .doc_comments()
                        .and_then(|docs| docs.first().map(|cmt| cmt.span()));
                    let documented_inputs = collect_documented_params(inputs);
                    let documented_outputs = collect_documented_params(outputs.map(Decl::Bound));

                    let doc_comments = (top_level_docs.is_some()
                        || !documented_inputs.is_empty()
                        || !documented_outputs.is_empty())
                    .then_some(ItemDocComments {
                        top_level: top_level_docs,
                        inputs: documented_inputs,
                        outputs: documented_outputs,
                    });

                    self.current_item = Some(Item {
                        kind,
                        name: name.text().to_string(),
                        name_span: name.span(),
                        doc_comments,
                    });
                }
            },
            VisitReason::Exit => {
                self.current_item = None;
            }
        }
    }

    /// Lint a collection of [`MetadataObjectItem`]s.
    ///
    /// This checks for items in the form:
    ///
    /// * `String`
    /// * `Object` with a `description` and/or `help` `String` field
    fn check_meta_items(
        &self,
        diagnostics: &mut Diagnostics,
        section: Section,
        node: &impl AstNode<SyntaxNode>,
        items: impl Iterator<Item = MetadataObjectItem>,
    ) {
        if self.adherence == DocSourceAdherence::Meta {
            return;
        }

        let Some(current_item) = &self.current_item else {
            return;
        };

        let kind = match section {
            Section::MetaOutputs => ParameterKind::Output,
            Section::ParameterMeta => match current_item.kind {
                ItemKind::Struct => ParameterKind::Field,
                _ => ParameterKind::Input,
            },
        };

        for item in items {
            let doc_spans = meta_item_doc_spans(&item);
            if doc_spans.is_empty() {
                continue;
            }

            match self.adherence {
                DocSourceAdherence::Comments => {
                    diagnostics.exceptable_add(
                        parameter_comment_only(
                            self.severity,
                            section,
                            kind,
                            item.name().text(),
                            item.span(),
                        ),
                        node.inner(),
                        &self.exceptable_nodes(),
                    );
                }
                DocSourceAdherence::Consistent => {
                    let Some(doc_comments) = &current_item.doc_comments else {
                        continue;
                    };

                    let params = match section {
                        Section::MetaOutputs => &doc_comments.outputs,
                        Section::ParameterMeta => &doc_comments.inputs,
                    };

                    let Some(param_spans) = params.get(item.name().text()).copied() else {
                        continue;
                    };

                    diagnostics.exceptable_add(
                        conflict(
                            self.severity,
                            Documentable::Parameter(kind),
                            item.name().text(),
                            param_spans.name,
                            param_spans.comment,
                            doc_spans,
                        ),
                        node.inner(),
                        &self.exceptable_nodes(),
                    );
                }
                DocSourceAdherence::Meta => {}
            }
        }
    }
}

impl Visitor for MixedDocSourcesRule {
    fn reset(&mut self) {
        let Self {
            severity: _,
            adherence: _,
            current_item,
        } = self;

        *current_item = None;
    }

    fn struct_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        def: &StructDefinition,
    ) {
        self.check_doc_comments(
            diagnostics,
            reason,
            ItemKind::Struct,
            def.name(),
            def,
            ParameterKind::Field,
            def.members().map(Decl::Unbound),
            [].into_iter(),
        );
    }

    fn task_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        task: &TaskDefinition,
    ) {
        self.check_doc_comments(
            diagnostics,
            reason,
            ItemKind::Task,
            task.name(),
            task,
            ParameterKind::Input,
            task.input()
                .iter()
                .flat_map(|section| section.declarations()),
            task.output()
                .iter()
                .flat_map(|section| section.declarations()),
        );
    }

    fn workflow_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        workflow: &WorkflowDefinition,
    ) {
        self.check_doc_comments(
            diagnostics,
            reason,
            ItemKind::Workflow,
            workflow.name(),
            workflow,
            ParameterKind::Input,
            workflow
                .input()
                .iter()
                .flat_map(|section| section.declarations()),
            workflow
                .output()
                .iter()
                .flat_map(|section| section.declarations()),
        );
    }

    fn metadata_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &MetadataSection,
    ) {
        if reason != VisitReason::Enter || self.adherence == DocSourceAdherence::Meta {
            return;
        }

        let Some(current_item) = &self.current_item else {
            return;
        };

        let mut description = None;
        let mut help = None;
        let mut outputs = None;
        for item in section.items() {
            let name = item.name();

            match name.text() {
                // Any non-string metadata values are ignored. The user might be
                // doing something special
                "description" if matches!(item.value(), MetadataValue::String(_)) => {
                    description = Some(name.span())
                }
                "help" if matches!(item.value(), MetadataValue::String(_)) => {
                    help = Some(name.span())
                }
                "outputs" => {
                    let MetadataValue::Object(obj) = item.value() else {
                        continue;
                    };

                    outputs = Some(obj);
                }
                _ => {}
            }
        }

        match self.adherence {
            DocSourceAdherence::Comments => {
                if description.is_some() || help.is_some() {
                    diagnostics.exceptable_add(
                        description_comment_only(
                            self.severity,
                            current_item.kind,
                            &current_item.name,
                            current_item.name_span,
                            description,
                            help,
                        ),
                        section.inner(),
                        &self.exceptable_nodes(),
                    );
                }

                if let Some(outputs) = outputs {
                    self.check_meta_items(
                        diagnostics,
                        Section::MetaOutputs,
                        section,
                        outputs.items(),
                    );
                }
            }
            DocSourceAdherence::Consistent => {
                if let Some(ItemDocComments {
                    top_level: Some(top_level_docs),
                    ..
                }) = &current_item.doc_comments
                {
                    let field_spans: Vec<Span> =
                        [description, help].into_iter().flatten().collect();
                    if !field_spans.is_empty() {
                        diagnostics.exceptable_add(
                            conflict(
                                self.severity,
                                Documentable::Item(current_item.kind),
                                &current_item.name,
                                current_item.name_span,
                                *top_level_docs,
                                field_spans,
                            ),
                            section.inner(),
                            &self.exceptable_nodes(),
                        );
                    }
                }

                if let Some(outputs) = outputs {
                    self.check_meta_items(
                        diagnostics,
                        Section::MetaOutputs,
                        section,
                        outputs.items(),
                    );
                }
            }
            DocSourceAdherence::Meta => {}
        }
    }

    fn parameter_metadata_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &ParameterMetadataSection,
    ) {
        if reason != VisitReason::Enter || self.adherence == DocSourceAdherence::Meta {
            return;
        }

        self.check_meta_items(
            diagnostics,
            Section::ParameterMeta,
            section,
            section.items(),
        );
    }
}
