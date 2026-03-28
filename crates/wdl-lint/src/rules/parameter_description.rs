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

/// The reserved key name for `description`.
const DESCRIPTION_KEY: &str = "description";

/// The key name for the `outputs` section in `meta`.
const OUTPUTS_KEY: &str = "outputs";

/// Creates a diagnostic for missing descriptions.
fn missing_description_diagnostic(name: &str, is_output: bool, span: Span) -> Diagnostic {
    let item_type = if is_output { "output" } else { "parameter" };
    let location = if is_output { " in `meta.outputs`" } else { "" };

    Diagnostic::note(format!(
        "{} `{}` is missing a description{}",
        item_type, name, location
    ))
    .with_rule(ID)
    .with_highlight(span)
    .with_fix(format!("add a description for `{}`", name))
}

/// Checks if a metadata value is a valid description.
fn has_valid_description(value: &MetadataValue) -> bool {
    match value {
        MetadataValue::String(_) => true,
        MetadataValue::Object(obj) => obj
            .items()
            .any(|item| item.name().text() == DESCRIPTION_KEY),
        // NOTE: non-string/non-object types are handled by `DocMetaStrings`.
        _ => true,
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

    fn examples(&self) -> &'static [&'static str] {
        &[
            r#"```wdl
version 1.2

task greet {
    meta {
        outputs: {
            greeting: {}
        }
    }

    parameter_meta {
        name: {}
    }

    input {
        String name
    }

    command <<<
        echo "Hello, ~{name}"
    >>>

    output {
        String greeting = stdout()
    }
}
```"#,
            r#"Use instead:

```wdl
version 1.2

task greet {
    meta {
        outputs: {
            greeting: "The generated greeting message."
        }
    }

    parameter_meta {
        name: "The name of the person to greet."
    }

    input {
        String name
    }

    command <<<
        echo "Hello, ~{name}"
    >>>

    output {
        String greeting = stdout()
    }
}
```"#,
        ]
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

    fn related_rules(&self) -> &'static [&'static str] {
        &["DescriptionLength", "MatchingOutputMeta", "MetaDescription"]
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

        for item in section.items() {
            if item.name().text() != OUTPUTS_KEY {
                continue;
            }

            if let MetadataValue::Object(outputs_obj) = item.value() {
                for output_item in outputs_obj.items() {
                    if !has_valid_description(&output_item.value()) {
                        diagnostics.exceptable_add(
                            missing_description_diagnostic(
                                output_item.name().text(),
                                true,
                                output_item.name().span(),
                            ),
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

        for item in section.items() {
            if !has_valid_description(&item.value()) {
                diagnostics.exceptable_add(
                    missing_description_diagnostic(item.name().text(), false, item.name().span()),
                    SyntaxElement::from(item.inner().clone()),
                    &self.exceptable_nodes(),
                );
            }
        }
    }
}
