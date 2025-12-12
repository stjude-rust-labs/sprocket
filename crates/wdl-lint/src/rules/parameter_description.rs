//! A lint rule for ensuring parameters have proper descriptions.

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
use wdl_ast::v1::ParameterMetadataSection;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the parameter description rule.
const ID: &str = "ParameterDescription";

/// The reserved key name for descriptions.
const DESCRIPTION_KEY: &str = "description";

/// Creates a diagnostic for missing descriptions.
fn missing_description_diagnostic(name: &str, is_output: bool, span: Span) -> Diagnostic {
    let item_type = if is_output { "output" } else { "parameter" };
    let location = if is_output { " in `meta.outputs`" } else { "" };

    Diagnostic::warning(format!(
        "{} `{}` is missing a description{}",
        item_type, name, location
    ))
    .with_rule(ID)
    .with_label(
        format!(
            "{} should be documented with either a `String` description or an object with a \
             `description` key",
            item_type
        ),
        span,
    )
    .with_fix(format!("add a description for `{}`", name,))
}

/// Checks if a metadata value is a valid description
fn has_valid_description(value: &MetadataValue) -> bool {
    match value {
        // Simple string is valid
        MetadataValue::String(_) => true,

        // Object must have a "description" key
        MetadataValue::Object(obj) => {
            for item in obj.items() {
                let name = item.name();
                let key = name.text();
                if key == DESCRIPTION_KEY {
                    // Found description key, check if it's a string
                    return matches!(item.value(), MetadataValue::String(_));
                }
            }
            // No description key found
            false
        }

        // Any other type is invalid
        _ => false,
    }
}

/// Detects parameters without proper descriptions.
#[derive(Default, Debug, Clone, Copy)]
pub struct ParameterDescriptionRule;

impl Rule for ParameterDescriptionRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that parameters and outputs have proper descriptions."
    }

    fn explanation(&self) -> &'static str {
        "Documentation is expected for each parameter (in `parameter_meta`) and each output (in \
         `meta.outputs`). A valid description is either a simple `String` value or an object \
         containing a `description` key with a `String` value."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Documentation, Tag::Completeness])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::MetadataSectionNode,
            SyntaxKind::ParameterMetadataSectionNode,
            SyntaxKind::MetadataObjectItemNode,
        ])
    }

    fn related_rules(&self) -> &[&'static str] {
        &[]
    }
}

impl Visitor for ParameterDescriptionRule {
    fn reset(&mut self) {
        *self = Default::default();
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

        // Check for meta.outputs section
        for item in section.items() {
            let name = item.name();
            let key = name.text();

            if key != "outputs" {
                continue;
            }

            let value = item.value();

            if let MetadataValue::Object(outputs_obj) = value {
                // Check each output in the outputs object
                for output_item in outputs_obj.items() {
                    let output_name_ident = output_item.name();
                    let output_name = output_name_ident.text();
                    let output_value = output_item.value();

                    // Check if this output has a valid description
                    if !has_valid_description(&output_value) {
                        diagnostics.exceptable_add(
                            missing_description_diagnostic(output_name, true, output_item.span()),
                            SyntaxElement::from(output_item.inner().clone()),
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
        section: &ParameterMetadataSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Check each parameter in the parameter_meta section
        for item in section.items() {
            let name = item.name();
            let param_name = name.text();
            let value = item.value();

            // Check if this parameter has a valid description
            if !has_valid_description(&value) {
                diagnostics.exceptable_add(
                    missing_description_diagnostic(param_name, false, item.span()),
                    SyntaxElement::from(item.inner().clone()),
                    &self.exceptable_nodes(),
                );
            }
        }
    }
}
