//! A lint rule that disallows mixing documentation sources.

use std::fmt::Display;
use std::fmt::Formatter;

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use strum::VariantArray;
use toml_spanner::Toml;
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
use wdl_ast::v1::BoundDecl;
use wdl_ast::v1::Decl;
use wdl_ast::v1::MetadataObjectItem;
use wdl_ast::v1::MetadataSection;
use wdl_ast::v1::MetadataValue;
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

/// The type of an item.
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

/// Gets the [`ItemKind`] for a [`SectionParent`].
fn parent_kind(parent: &SectionParent) -> ItemKind {
    match parent {
        SectionParent::Task(_) => ItemKind::Task,
        SectionParent::Workflow(_) => ItemKind::Workflow,
        SectionParent::Struct(_) => ItemKind::Struct,
    }
}

/// Gets the span of the first doc comment of a [`SectionParent`], if any.
fn item_doc_comment(parent: &SectionParent) -> Option<Span> {
    match parent {
        SectionParent::Task(t) => t.doc_comments().and_then(|c| c.first().map(|c| c.span())),
        SectionParent::Workflow(w) => w.doc_comments().and_then(|c| c.first().map(|c| c.span())),
        SectionParent::Struct(s) => s.doc_comments().and_then(|c| c.first().map(|c| c.span())),
    }
}

/// Gets the span of the first doc comment of a [`Decl`], if any.
fn decl_doc_comment(decl: &Decl) -> Option<Span> {
    decl.doc_comments()
        .and_then(|c| c.first().map(|c| c.span()))
}

/// Finds an input or struct member declaration by name in a [`SectionParent`].
fn input_by_name(parent: &SectionParent, name: &str) -> Option<Decl> {
    match parent {
        SectionParent::Task(t) => t
            .input()
            .and_then(|s| s.declarations().find(|d| d.name().text() == name)),
        SectionParent::Workflow(w) => w
            .input()
            .and_then(|s| s.declarations().find(|d| d.name().text() == name)),
        SectionParent::Struct(s) => s
            .members()
            .find(|d| d.name().text() == name)
            .map(Decl::Unbound),
    }
}

/// Finds an output declaration by name in a [`SectionParent`].
fn output_by_name(parent: &SectionParent, name: &str) -> Option<BoundDecl> {
    match parent {
        SectionParent::Task(t) => t
            .output()
            .and_then(|s| s.declarations().find(|d| d.name().text() == name)),
        SectionParent::Workflow(w) => w
            .output()
            .and_then(|s| s.declarations().find(|d| d.name().text() == name)),
        SectionParent::Struct(_) => None,
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

/// A lint rule for mixed documentation sources.
#[derive(Debug, Clone)]
pub struct MixedDocSourcesRule {
    /// The severity of the rule's diagnostics.
    severity: Severity,
    /// The user config for preferred doc sources.
    adherence: DocSourceAdherence,
    /// The version of the WDL document being linted.
    version: Option<SupportedVersion>,
}

impl MixedDocSourcesRule {
    /// Create a new `MixedDocSources` rule.
    pub fn new(config: &Config) -> Self {
        Self {
            severity: config.mixed_doc_sources.diagnostic_severity(),
            adherence: config.mixed_doc_sources.doc_source_adherence,
            version: None,
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

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
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

impl MixedDocSourcesRule {
    /// Validates that an item and its declarations do not have doc comments in
    /// [`DocSourceAdherence::Meta`] mode.
    fn validate_meta_adherence(&self, diagnostics: &mut Diagnostics, parent: &SectionParent) {
        let name = parent.name();
        let kind = parent_kind(parent);

        if item_doc_comment(parent).is_some() {
            diagnostics.exceptable_add(
                description_meta_only(self.severity, kind, name.text(), name.span()),
                parent.inner(),
                &self.exceptable_nodes(),
            );
        }

        match parent {
            SectionParent::Task(task) => {
                if let Some(input) = task.input() {
                    for decl in input.declarations() {
                        if decl_doc_comment(&decl).is_none() {
                            continue;
                        }

                        let decl_name = decl.name();
                        diagnostics.exceptable_add(
                            parameter_meta_only(
                                self.severity,
                                Section::ParameterMeta,
                                ParameterKind::Input,
                                decl_name.text(),
                                decl_name.span(),
                            ),
                            task.inner(),
                            &self.exceptable_nodes(),
                        );
                    }
                }

                let Some(output) = task.output() else {
                    return;
                };

                for decl in output.declarations().map(Decl::Bound) {
                    if decl_doc_comment(&decl).is_none() {
                        continue;
                    }

                    let decl_name = decl.name();
                    diagnostics.exceptable_add(
                        parameter_meta_only(
                            self.severity,
                            Section::MetaOutputs,
                            ParameterKind::Output,
                            decl_name.text(),
                            decl_name.span(),
                        ),
                        task.inner(),
                        &self.exceptable_nodes(),
                    );
                }
            }
            SectionParent::Workflow(workflow) => {
                if let Some(input) = workflow.input() {
                    for decl in input.declarations() {
                        if decl_doc_comment(&decl).is_none() {
                            continue;
                        }

                        let decl_name = decl.name();
                        diagnostics.exceptable_add(
                            parameter_meta_only(
                                self.severity,
                                Section::ParameterMeta,
                                ParameterKind::Input,
                                decl_name.text(),
                                decl_name.span(),
                            ),
                            workflow.inner(),
                            &self.exceptable_nodes(),
                        );
                    }
                }

                let Some(output) = workflow.output() else {
                    return;
                };

                for decl in output.declarations().map(Decl::Bound) {
                    if decl_doc_comment(&decl).is_none() {
                        continue;
                    }

                    let decl_name = decl.name();
                    diagnostics.exceptable_add(
                        parameter_meta_only(
                            self.severity,
                            Section::MetaOutputs,
                            ParameterKind::Output,
                            decl_name.text(),
                            decl_name.span(),
                        ),
                        workflow.inner(),
                        &self.exceptable_nodes(),
                    );
                }
            }
            SectionParent::Struct(s) => {
                // Struct metadata sections are only supported in WDL >= 1.2
                if self
                    .version
                    .is_none_or(|v| v < SupportedVersion::V1(V1::Two))
                {
                    return;
                }

                for decl in s.members().map(Decl::Unbound) {
                    if decl_doc_comment(&decl).is_none() {
                        continue;
                    }

                    let decl_name = decl.name();
                    diagnostics.exceptable_add(
                        parameter_meta_only(
                            self.severity,
                            Section::ParameterMeta,
                            ParameterKind::Field,
                            decl_name.text(),
                            decl_name.span(),
                        ),
                        s.inner(),
                        &self.exceptable_nodes(),
                    );
                }
            }
        }
    }
}

impl Visitor for MixedDocSourcesRule {
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
        if reason != VisitReason::Enter {
            return;
        }

        self.version = Some(version);
    }

    fn struct_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        def: &StructDefinition,
    ) {
        if reason != VisitReason::Enter || self.adherence != DocSourceAdherence::Meta {
            return;
        }

        self.validate_meta_adherence(diagnostics, &SectionParent::Struct(def.clone()));
    }

    fn task_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        task: &TaskDefinition,
    ) {
        if reason != VisitReason::Enter || self.adherence != DocSourceAdherence::Meta {
            return;
        }

        self.validate_meta_adherence(diagnostics, &SectionParent::Task(task.clone()));
    }

    fn workflow_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        workflow: &WorkflowDefinition,
    ) {
        if reason != VisitReason::Enter || self.adherence != DocSourceAdherence::Meta {
            return;
        }

        self.validate_meta_adherence(diagnostics, &SectionParent::Workflow(workflow.clone()));
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

        let parent = section.parent();
        let name = parent.name();
        let kind = parent_kind(&parent);

        let mut description = None;
        let mut help = None;
        let mut outputs = None;
        for item in section.items() {
            let item_name = item.name();

            match item_name.text() {
                // Any non-string metadata values are ignored. The user might be
                // doing something special
                "description" if matches!(item.value(), MetadataValue::String(_)) => {
                    description = Some(item_name.span());
                }
                "help" if matches!(item.value(), MetadataValue::String(_)) => {
                    help = Some(item_name.span());
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
                            kind,
                            name.text(),
                            name.span(),
                            description,
                            help,
                        ),
                        section.inner(),
                        &self.exceptable_nodes(),
                    );
                }

                let Some(outputs) = outputs else {
                    return;
                };

                for item in outputs.items() {
                    if meta_item_doc_spans(&item).is_empty() {
                        continue;
                    }

                    diagnostics.exceptable_add(
                        parameter_comment_only(
                            self.severity,
                            Section::MetaOutputs,
                            ParameterKind::Output,
                            item.name().text(),
                            item.span(),
                        ),
                        section.inner(),
                        &self.exceptable_nodes(),
                    );
                }
            }
            DocSourceAdherence::Consistent => {
                if let Some(top_level_comment) = item_doc_comment(&parent) {
                    let field_spans: Vec<Span> =
                        [description, help].into_iter().flatten().collect();
                    if !field_spans.is_empty() {
                        diagnostics.exceptable_add(
                            conflict(
                                self.severity,
                                Documentable::Item(kind),
                                name.text(),
                                name.span(),
                                top_level_comment,
                                field_spans,
                            ),
                            section.inner(),
                            &self.exceptable_nodes(),
                        );
                    }
                }

                let Some(outputs) = outputs else {
                    return;
                };

                for item in outputs.items() {
                    let doc_spans = meta_item_doc_spans(&item);
                    if doc_spans.is_empty() {
                        continue;
                    }

                    let item_name = item.name();
                    if let Some(output_decl) =
                        output_by_name(&parent, item_name.text()).map(Decl::Bound)
                        && let Some(comment) = decl_doc_comment(&output_decl)
                    {
                        diagnostics.exceptable_add(
                            conflict(
                                self.severity,
                                Documentable::Parameter(ParameterKind::Output),
                                item_name.text(),
                                output_decl.name().span(),
                                comment,
                                doc_spans,
                            ),
                            section.inner(),
                            &self.exceptable_nodes(),
                        );
                    }
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

        let parent = section.parent();
        let param_kind = match parent {
            SectionParent::Struct(_) => ParameterKind::Field,
            _ => ParameterKind::Input,
        };

        for item in section.items() {
            let doc_spans = meta_item_doc_spans(&item);
            if doc_spans.is_empty() {
                continue;
            }

            let item_name = item.name();

            match self.adherence {
                DocSourceAdherence::Comments => {
                    diagnostics.exceptable_add(
                        parameter_comment_only(
                            self.severity,
                            Section::ParameterMeta,
                            param_kind,
                            item_name.text(),
                            item.span(),
                        ),
                        section.inner(),
                        &self.exceptable_nodes(),
                    );
                }
                DocSourceAdherence::Consistent => {
                    if let Some(decl) = input_by_name(&parent, item_name.text())
                        && let Some(comment) = decl_doc_comment(&decl)
                    {
                        diagnostics.exceptable_add(
                            conflict(
                                self.severity,
                                Documentable::Parameter(param_kind),
                                item_name.text(),
                                decl.name().span(),
                                comment,
                                doc_spans,
                            ),
                            section.inner(),
                            &self.exceptable_nodes(),
                        );
                    }
                }
                DocSourceAdherence::Meta => {}
            }
        }
    }
}
