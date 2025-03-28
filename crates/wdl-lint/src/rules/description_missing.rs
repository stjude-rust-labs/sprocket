//! A lint rule to ensure a description is included in `meta` sections.

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
use wdl_ast::v1::SectionParent;
use wdl_ast::version::V1;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the description missing rule.
const ID: &str = "DescriptionMissing";

/// Creates a description missing diagnostic.
fn description_missing(span: Span, parent: SectionParent) -> Diagnostic {
    let (ty, name) = match parent {
        SectionParent::Task(t) => ("task", t.name()),
        SectionParent::Workflow(w) => ("workflow", w.name()),
        SectionParent::Struct(s) => ("struct", s.name()),
    };

    Diagnostic::note(format!(
        "{ty} `{name}` is missing a description key",
        name = name.text()
    ))
    .with_rule(ID)
    .with_highlight(span)
    .with_fix("add a `description` key to the meta section")
}

/// Detects unsorted input declarations.
#[derive(Default, Debug, Clone, Copy)]
pub struct DescriptionMissingRule {
    /// The version of the WDL document being linted.
    version: Option<SupportedVersion>,
    /// Whether or not we're currently in a struct definition.
    in_struct: bool,
}

impl Rule for DescriptionMissingRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that a description is present for each meta section."
    }

    fn explanation(&self) -> &'static str {
        "Each task or workflow should have a description in the meta section. This description \
         should be an explanation of the task or workflow. The description should be written in \
         active voice and complete sentences. More detailed information can be included in the \
         `help` key."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Completeness])
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::MetadataSectionNode,
        ])
    }

    fn related_rules(&self) -> &[&'static str] {
        &[
            "MatchingParameterMeta",
            "MissingOutput",
            "MissingRequirements",
            "MissingRuntime",
            "NonmatchingOutput",
        ]
    }
}

impl Visitor for DescriptionMissingRule {
    type State = Diagnostics;

    fn document(
        &mut self,
        _: &mut Self::State,
        reason: VisitReason,
        _: &Document,
        version: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Reset the visitor upon document entry
        *self = Default::default();
        self.version = Some(version);
    }

    fn struct_definition(
        &mut self,
        _: &mut Self::State,
        reason: VisitReason,
        _: &wdl_ast::v1::StructDefinition,
    ) {
        self.in_struct = reason == VisitReason::Enter;
    }

    fn metadata_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &MetadataSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Only check struct definitions for WDL >=1.2
        if self.in_struct
            && self.version.expect("should have version") < SupportedVersion::V1(V1::Two)
        {
            return;
        }

        let description = section
            .items()
            .find(|entry| entry.name().inner().to_string() == "description");

        if description.is_none() {
            state.exceptable_add(
                description_missing(
                    section
                        .inner()
                        .first_token()
                        .expect("metadata section should have tokens")
                        .text_range()
                        .into(),
                    section.parent(),
                ),
                SyntaxElement::from(section.inner().clone()),
                &self.exceptable_nodes(),
            );
        }
    }
}
