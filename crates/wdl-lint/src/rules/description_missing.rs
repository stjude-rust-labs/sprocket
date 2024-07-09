//! A lint rule to ensure a description is included in `meta` sections.

use wdl_ast::v1::MetadataSection;
use wdl_ast::v1::TaskOrWorkflow;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::Span;
use wdl_ast::ToSpan;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the description missing rule.
const ID: &str = "DescriptionMissing";

/// Creates a description missing diagnostic.
fn description_missing(span: Span, context: TaskOrWorkflow) -> Diagnostic {
    let (ty, name) = match context {
        TaskOrWorkflow::Task(t) => ("task", t.name().as_str().to_string()),
        TaskOrWorkflow::Workflow(w) => ("workflow", w.name().as_str().to_string()),
    };

    Diagnostic::note(format!("{ty} `{name}` is missing a description key"))
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("add a `description` key to this meta section")
}

/// Detects unsorted input declarations.
#[derive(Default, Debug, Clone, Copy)]
pub struct DescriptionMissingRule;

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
}

impl Visitor for DescriptionMissingRule {
    type State = Diagnostics;

    fn document(&mut self, _: &mut Self::State, reason: VisitReason, _: &Document) {
        if reason == VisitReason::Exit {
            return;
        }

        // Reset the visitor upon document entry
        *self = Default::default();
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

        let description = section
            .items()
            .find(|entry| entry.name().syntax().to_string() == "description");

        if description.is_none() {
            state.add(description_missing(
                section
                    .syntax()
                    .first_token()
                    .unwrap()
                    .text_range()
                    .to_span(),
                section.parent(),
            ));
        }
    }
}
