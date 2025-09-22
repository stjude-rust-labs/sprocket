//! A lint rule to ensure `description` meta entries are short enough for
//! `wdl-doc`.

use wdl_analysis::Diagnostics;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::MetadataSection;
use wdl_ast::v1::MetadataValue;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the description length rule.
const ID: &str = "DescriptionLength";

/// The maximum length of a description before it is summarized.
const DESCRIPTION_MAX_LENGTH: usize = 140;

/// Creates a description too long diagnostic.
fn description_too_long(span: Span) -> Diagnostic {
    Diagnostic::note("this description may be clipped in documentation")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix(format!(
            "shorten this string so it is less than or equal to {DESCRIPTION_MAX_LENGTH} \
             characters"
        ))
}

/// Detects a malformed lint directive.
#[derive(Default, Debug, Clone, Copy)]
pub struct DescriptionLengthRule;

impl Rule for DescriptionLengthRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that `description` meta entries are not too long for display in documentation."
    }

    fn explanation(&self) -> &'static str {
        "Descriptions should be kept short so that they can always render in full. If a \
         `description` is too long, it may be clipped in some contexts during documentation. \
         `help` meta entries are permitted to be of any length, and may be a better place for long \
         form text."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::DocRendering])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[SyntaxKind::MetadataObjectItemNode])
    }

    fn related_rules(&self) -> &[&'static str] {
        &[]
    }
}

impl Visitor for DescriptionLengthRule {
    fn reset(&mut self) {
        *self = Self
    }

    fn metadata_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &MetadataSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Some(description_item) = section
            .items()
            .find(|entry| entry.name().inner().to_string() == "description")
            && let MetadataValue::String(description) = description_item.value()
            && !description.is_empty()
        {
            let mut text = String::new();
            description
                .text()
                .expect("meta strings cannot be interpolated")
                .unescape_to(&mut text);

            if text.len() > DESCRIPTION_MAX_LENGTH {
                diagnostics.exceptable_add(
                    description_too_long(description_item.name().span()),
                    SyntaxElement::from(description_item.inner().clone()),
                    &self.exceptable_nodes(),
                );
            }
        }

        if let Some(outputs_item) = section
            .items()
            .find(|entry| entry.name().inner().to_string() == "outputs")
            && let MetadataValue::Object(outputs_object) = outputs_item.value()
        {
            for output in outputs_object.items() {
                if let MetadataValue::String(description) = output.value()
                    && !description.is_empty()
                {
                    let mut text = String::new();
                    description
                        .text()
                        .expect("meta strings cannot be interpolated")
                        .unescape_to(&mut text);

                    if text.len() > DESCRIPTION_MAX_LENGTH {
                        diagnostics.exceptable_add(
                            description_too_long(output.name().span()),
                            SyntaxElement::from(output.inner().clone()),
                            &self.exceptable_nodes(),
                        );
                    }
                } else if let MetadataValue::Object(inner_object) = output.value()
                    && let Some(description_item) = inner_object
                        .items()
                        .find(|entry| entry.name().inner().to_string() == "description")
                    && let MetadataValue::String(description) = description_item.value()
                    && !description.is_empty()
                {
                    let mut text = String::new();
                    description
                        .text()
                        .expect("meta strings cannot be interpolated")
                        .unescape_to(&mut text);

                    if text.len() > DESCRIPTION_MAX_LENGTH {
                        diagnostics.exceptable_add(
                            description_too_long(description_item.name().span()),
                            SyntaxElement::from(description_item.inner().clone()),
                            &self.exceptable_nodes(),
                        );
                    }
                }
            }
        }
    }

    fn parameter_metadata_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &wdl_ast::v1::ParameterMetadataSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        for param in section.items() {
            if let MetadataValue::String(description) = param.value()
                && !description.is_empty()
            {
                let mut text = String::new();
                description
                    .text()
                    .expect("meta strings cannot be interpolated")
                    .unescape_to(&mut text);

                if text.len() > DESCRIPTION_MAX_LENGTH {
                    diagnostics.exceptable_add(
                        description_too_long(param.name().span()),
                        SyntaxElement::from(param.inner().clone()),
                        &self.exceptable_nodes(),
                    );
                }
            } else if let MetadataValue::Object(inner_object) = param.value()
                && let Some(description_item) = inner_object
                    .items()
                    .find(|entry| entry.name().inner().to_string() == "description")
                && let MetadataValue::String(description) = description_item.value()
                && !description.is_empty()
            {
                let mut text = String::new();
                description
                    .text()
                    .expect("meta strings cannot be interpolated")
                    .unescape_to(&mut text);

                if text.len() > DESCRIPTION_MAX_LENGTH {
                    diagnostics.exceptable_add(
                        description_too_long(description_item.name().span()),
                        SyntaxElement::from(description_item.inner().clone()),
                        &self.exceptable_nodes(),
                    );
                }
            }
        }
    }
}
