//! A lint rule for ensuring reserved meta keys have string values.

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

/// The identifier for the expected meta string rule.
const ID: &str = "ExpectedMetaString";

/// Reserved keys that must have string values for Sprocket's doc command.
const RESERVED_KEYS: &[&str] = &[
    "description",
    "help",
    "external_help",
    "warning",
    "category",
    "group",
];

/// Creates a diagnostic for non-string metadata values.
fn non_string_value_diagnostic(key: &str, value_type: &str, span: Span) -> Diagnostic {
    Diagnostic::warning(format!(
        "metadata key `{}` should have a string value, found {}",
        key, value_type
    ))
    .with_rule(ID)
    .with_label(
        format!(
            "`{}` must be a string for proper documentation rendering",
            key
        ),
        span,
    )
    .with_fix(format!("change the value of `{}` to a string literal", key))
}

/// Gets a human-readable type name for a metadata value.
fn get_value_type_name(value: &MetadataValue) -> &'static str {
    match value {
        MetadataValue::Null(_) => "null",
        MetadataValue::Boolean(_) => "boolean",
        MetadataValue::Integer(_) => "integer",
        MetadataValue::Float(_) => "float",
        MetadataValue::String(_) => "string",
        MetadataValue::Array(_) => "array",
        MetadataValue::Object(_) => "object",
    }
}

/// Checks if a metadata value is a string.
fn is_string_value(value: &MetadataValue) -> bool {
    matches!(value, MetadataValue::String(_))
}

/// Checks metadata object items for reserved keys with non-string values.
fn check_object_items(
    obj: &wdl_ast::v1::MetadataObject,
    diagnostics: &mut Diagnostics,
    exceptable_nodes: &Option<&'static [SyntaxKind]>,
) {
    for item in obj.items() {
        let name = item.name();
        let key = name.text();

        if !RESERVED_KEYS.contains(&key) {
            continue;
        }

        let value = item.value();

        if !is_string_value(&value) {
            let value_type = get_value_type_name(&value);
            diagnostics.exceptable_add(
                non_string_value_diagnostic(key, value_type, item.span()),
                SyntaxElement::from(item.inner().clone()),
                exceptable_nodes,
            );
        }
    }
}

/// Detects non-string values for reserved meta keys.
#[derive(Default, Debug, Clone, Copy)]
pub struct ExpectedMetaStringRule;

impl Rule for ExpectedMetaStringRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that reserved meta keys have string values."
    }

    fn explanation(&self) -> &'static str {
        "Sprocket's documentation command reserves certain keys in `meta` and `parameter_meta` \
         sections for documentation generation. These keys (`description`, `help`, \
         `external_help`, `warning`, `category`, and `group`) must have string values. Using \
         non-string values will cause the documentation to be rendered incorrectly or not at all. \
         This rule ensures all reserved keys have string values for proper documentation \
         generation."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::SprocketCompatibility])
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
        &[
            "MetaDescription",
            "MetaSections",
            "ParameterMetaMatched",
            "DescriptionLength",
        ]
    }
}

impl Visitor for ExpectedMetaStringRule {
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

        // Check each item in the meta section
        for item in section.items() {
            let name = item.name();
            let key = name.text();
            let value = item.value();

            // Special handling for "outputs" key - check its nested content
            if key == "outputs" {
                if let MetadataValue::Object(ref obj) = value {
                    for output_item in obj.items() {
                        let output_name = output_item.name();
                        let output_key = output_name.text();
                        let output_value = output_item.value();

                        // Check if the key itself is reserved
                        if RESERVED_KEYS.contains(&output_key) && !is_string_value(&output_value) {
                            let value_type = get_value_type_name(&output_value);
                            diagnostics.exceptable_add(
                                non_string_value_diagnostic(
                                    output_key,
                                    value_type,
                                    output_item.span(),
                                ),
                                SyntaxElement::from(output_item.inner().clone()),
                                &self.exceptable_nodes(),
                            );
                        }

                        // Check nested objects
                        if let MetadataValue::Object(ref nested_obj) = output_value {
                            check_object_items(nested_obj, diagnostics, &self.exceptable_nodes());
                        }
                    }
                }
                continue;
            }

            // Check if this is a reserved key
            if !RESERVED_KEYS.contains(&key) {
                continue;
            }

            // Check if the value is a string
            if !is_string_value(&value) {
                let value_type = get_value_type_name(&value);
                diagnostics.exceptable_add(
                    non_string_value_diagnostic(key, value_type, item.span()),
                    SyntaxElement::from(item.inner().clone()),
                    &self.exceptable_nodes(),
                );
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
            let value = item.value();

            match value {
                // Simple string description - this is valid
                MetadataValue::String(_) => {}

                // Object with potential reserved keys
                MetadataValue::Object(obj) => {
                    check_object_items(&obj, diagnostics, &self.exceptable_nodes());
                }

                // Any other type - check if it would be a reserved key as a simple description
                // This handles cases like: parameter_name: 123 (instead of parameter_name:
                // "description")
                _ => {
                    let name = item.name();
                    let _param_name = name.text();

                    // Only warn if this looks like it should be a description
                    // (simple non-string value for a parameter)
                    let value_type = get_value_type_name(&value);
                    diagnostics.exceptable_add(
                        non_string_value_diagnostic("description", value_type, item.span()),
                        SyntaxElement::from(item.inner().clone()),
                        &self.exceptable_nodes(),
                    );
                }
            }
        }
    }
}
