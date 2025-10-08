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

/// Reserved meta keys that must have string values for wdl-doc compatibility.
const RESERVED_META_KEYS: &[&str] = &[
    "description",
    "help",
    "external_help",
    "warning",
    "category", // for workflows
];

/// Reserved parameter_meta keys that must have string values for wdl-doc
/// compatibility.
const RESERVED_PARAMETER_META_KEYS: &[&str] = &[
    "description",
    "help",
    "external_help",
    "group", // for grouping inputs
];

/// Creates a "non-string meta value" diagnostic for meta section.
fn non_string_meta_value(key: &str, value_type: &str, span: Span) -> Diagnostic {
    Diagnostic::warning(format!(
        "reserved meta key `{key}` should have a string value, found {value_type}",
    ))
    .with_rule(ID)
    .with_label(
        format!("`{key}` must be a string for proper documentation rendering"),
        span,
    )
    .with_fix(format!("change the value of `{key}` to a string literal)"))
}

/// Creates a "non-string parameter meta value" diagnostic for parameter_meta
/// section.
fn non_string_parameter_meta_value(
    param_name: &str,
    key: &str,
    value_type: &str,
    span: Span,
) -> Diagnostic {
    Diagnostic::warning(format!(
        "reserved parameter_meta key `{key}` for parameter `{param_name}` should have a string \
         value, found {value_type}",
    ))
    .with_rule(ID)
    .with_label(
        format!("`{key}` must be a string for proper documentation rendering"),
        span,
    )
    .with_fix(format!("change the value of `{key}` to a string literal)"))
}

/// Creates a "non-string parameter description" diagnostic for simple
/// parameter_meta entries.
fn non_string_parameter_description(param_name: &str, value_type: &str, span: Span) -> Diagnostic {
    Diagnostic::warning(format!(
        "parameter `{param_name}` description should be a string, found {value_type}",
    ))
    .with_rule(ID)
    .with_label(
        "parameter description must be a string for proper documentation rendering",
        span,
    )
    .with_fix(format!(
        "change the parameter description to a string literal)"
    ))
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

/// Detects non-string values for reserved meta keys.
#[derive(Default, Debug, Clone, Copy)]
pub struct ExpectedMetaStringRule;

impl Rule for ExpectedMetaStringRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that reserved meta keys used by wdl-doc have string values."
    }

    fn explanation(&self) -> &'static str {
        "The wdl-doc tool reserves certain keys in `meta` and `parameter_meta` sections for \
         documentation generation. These keys (`description`, `help`, `external_help`, `warning`, \
         `category`, and `group`) must have string values. Using non-string values will cause \
         wdl-doc to skip rendering that documentation. This rule ensures all reserved keys have \
         string values for proper documentation generation."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[
            Tag::Correctness,
            Tag::Documentation,
            Tag::SprocketCompatibility,
        ])
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

            if !RESERVED_META_KEYS.contains(&key) {
                continue;
            }

            let value = item.value();

            // Check if the value is a string
            if !is_string_value(&value) {
                let value_type = get_value_type_name(&value);
                diagnostics.exceptable_add(
                    non_string_meta_value(key, value_type, item.span()),
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
            let name = item.name();
            let param_name = name.text();
            let value = item.value();

            match value {
                // Simple string description - this is valid
                MetadataValue::String(_) => {}

                // Object with potential reserved keys
                MetadataValue::Object(obj) => {
                    // Check each key in the object
                    for obj_item in obj.items() {
                        let obj_name = obj_item.name();
                        let key = obj_name.text();

                        if !RESERVED_PARAMETER_META_KEYS.contains(&key) {
                            continue;
                        }

                        let obj_value = obj_item.value();

                        // Check if the value is a string
                        if !is_string_value(&obj_value) {
                            let value_type = get_value_type_name(&obj_value);
                            diagnostics.exceptable_add(
                                non_string_parameter_meta_value(
                                    param_name,
                                    key,
                                    value_type,
                                    obj_item.span(),
                                ),
                                SyntaxElement::from(obj_item.inner().clone()),
                                &self.exceptable_nodes(),
                            );
                        }
                    }
                }

                // Any other type (number, boolean, array, null) - this is invalid for description
                _ => {
                    let value_type = get_value_type_name(&value);
                    diagnostics.exceptable_add(
                        non_string_parameter_description(param_name, value_type, item.span()),
                        SyntaxElement::from(item.inner().clone()),
                        &self.exceptable_nodes(),
                    );
                }
            }
        }
    }
}
