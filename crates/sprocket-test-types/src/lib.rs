//! Facilities for unit testing WDL documents.

use std::collections::BTreeMap;
use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Result;
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
mod assertions;
pub use assertions::*;
mod inputs;
pub mod yaml;

use wdl_analysis::Diagnostics;
use wdl_analysis::Document;
use wdl_ast::Diagnostic;
use wdl_ast::Span;

use crate::assertions::RawAssertions;
use crate::inputs::InputMatrix;
use crate::yaml::MaybeMap;
use crate::yaml::Spanned;
use crate::yaml::SpannedField;
use crate::yaml::spanned_fields;

/// Convert a [`serde_saphyr::Span`] to our [`Span`] type.
pub(crate) fn convert_yaml_span(span: serde_saphyr::Span) -> Span {
    // SAFETY: `serde-saphyr` guarantees that byte-level information is available
    //         when parsing from a string, which we always do.
    Span::new(
        span.byte_offset().expect("byte info should be available") as usize,
        span.byte_len().expect("byte info should be available") as usize,
    )
}

/// The parser encountered an unknown field.
fn unknown_field(span: serde_saphyr::Span, field: &str) -> Diagnostic {
    Diagnostic::warning(format!("unknown field `{field}`")).with_highlight(convert_yaml_span(span))
}

/// A target has multiple test definitions with the same name.
fn duplicate_test_definition(first: &Spanned<Value>, duplicate: &Spanned<Value>) -> Diagnostic {
    let Value::String(name) = &first.0.value else {
        unreachable!("should be validated beforehand")
    };
    Diagnostic::error(format!("the name `{name}` is defined multiple times"))
        .with_label(
            "first definition here",
            convert_yaml_span(first.0.defined.span()),
        )
        .with_label(
            format!("`{name}` redefined here",),
            convert_yaml_span(duplicate.0.defined.span()),
        )
}

/// The parser expected a YAML mapping.
fn expected_mapping(field: &Spanned<String>) -> Diagnostic {
    Diagnostic::error(format!(
        "expected a mapping of values in field `{}`",
        field.0.value
    ))
    .with_highlight(convert_yaml_span(field.0.defined.span()))
}

/// Collection of tests for an entire WDL document.
#[derive(Clone, Debug, JsonSchema)]
pub struct DocumentTests {
    /// Tasks or Workflows with test definitions.
    ///
    /// Each task or workflow may have one or more test definitions.
    pub targets: IndexMap<Spanned<String>, Vec<TestDefinition>>,
}

impl DocumentTests {
    /// Attempt to parse a [`DocumentTests`] from a YAML source string.
    pub fn parse(source: &str) -> Result<(Self, Diagnostics), Diagnostics> {
        #[derive(Debug, Deserialize)]
        #[serde(transparent)]
        struct RawDocumentTests {
            pub targets: IndexMap<Spanned<String>, Vec<Spanned<RawTestDefinition>>>,
        }

        let mut diagnostics = Diagnostics::default();
        let raw: RawDocumentTests = match serde_saphyr::from_str(source) {
            Ok(map) => map,
            Err(_e) => {
                // TODO(serial): create nice diagnostics from serde-saphyr errors
                diagnostics.add(Diagnostic::error(
                    "expected test document to be a YAML mapping from target names to test \
                     definitions",
                ));
                return Err(diagnostics);
            }
        };

        let mut document_tests = DocumentTests {
            targets: IndexMap::with_capacity(raw.targets.len()),
        };

        for (target, definitions) in raw.targets {
            let mut definition_names = HashSet::new();
            let mut parsed_definitions = Vec::with_capacity(definitions.len());
            for definition_val in definitions {
                let name_span = definition_val
                    .0
                    .value
                    .name
                    .as_ref()
                    .map(|name| name.value.clone());
                match TestDefinition::parse(definition_val) {
                    Ok(test_definition) => {
                        // SAFETY: The parse would fail if no name was provided
                        let name_span = name_span.unwrap();
                        if definition_names.insert(name_span.clone()) {
                            parsed_definitions.push(test_definition);
                        } else {
                            diagnostics.add(duplicate_test_definition(
                                definition_names
                                    .get(&name_span)
                                    .expect("just verified it exists"),
                                &name_span,
                            ));
                        }
                    }
                    Err(e) => diagnostics.extend(e),
                }
            }

            document_tests.targets.insert(target, parsed_definitions);
        }

        if diagnostics.has_errors() {
            Err(diagnostics)
        } else {
            Ok((document_tests, diagnostics))
        }
    }

    /// Validate the test definition against the associated WDL document.
    pub fn validate(&self, associated_wdl: &Document) -> Result<(), Diagnostics> {
        let mut diagnostics = Diagnostics::default();

        for (target, definitions) in &self.targets {
            let target_callable = match associated_wdl.callable_by_name(&target.0.value) {
                Some(callable) => callable,
                None => {
                    diagnostics.add(
                        Diagnostic::error(format!(
                            "no target named `{name}` in `{path}`",
                            name = target.0.value,
                            path = associated_wdl.path()
                        ))
                        .with_highlight(convert_yaml_span(target.0.defined.span())),
                    );
                    continue;
                }
            };

            for definition in definitions {
                definition
                    .assertions
                    .validate(target_callable, &mut diagnostics);
            }
        }

        if diagnostics.is_empty() {
            Ok(())
        } else {
            Err(diagnostics)
        }
    }
}

/// A test definition. Defines at least a single execution, but may define many
/// executions.
#[derive(Clone, Debug, JsonSchema)]
pub struct TestDefinition {
    /// Name for the test.
    pub name: Arc<str>,
    /// Any tags associated with the test.
    #[schemars(default)]
    pub tags: HashSet<String>,
    /// Matrix of inputs to combinatorially execute.
    #[schemars(default)]
    pub inputs: InputMatrix,
    /// Assertions (shared for all executions).
    ///
    /// If no assertions defined, it is assumed that failing execution for any
    /// reason is considered a test fail.
    #[schemars(default)]
    pub assertions: Assertions,
}

spanned_fields! {
    #[derive(Debug)]
    struct RawTestDefinition {
        name: Option<SpannedField<Spanned<Value>>>,
        tags: Option<SpannedField<Value>>,
        inputs: Option<SpannedField<MaybeMap<MaybeMap<Value>>>>,
        assertions: Option<SpannedField<RawAssertions>>,
    }
}

impl TestDefinition {
    /// Attempt to parse a [`TestDefinition`] within the given callable
    /// `target`.
    fn parse(raw: Spanned<RawTestDefinition>) -> Result<Self, Diagnostics> {
        let mut diagnostics = Diagnostics::default();

        let parsed_name = match raw.0.value.name {
            Some(raw_name) => {
                let Value::String(name) = raw_name.value.0.value else {
                    diagnostics.add(
                        Diagnostic::error("`name` must be a string")
                            .with_highlight(convert_yaml_span(raw_name.value.0.defined.span())),
                    );
                    return Err(diagnostics);
                };
                Some(name)
            }
            None => None,
        };

        let parsed_tags = match raw.0.value.tags {
            Some(tags) => match serde_json::from_value::<HashSet<String>>(tags.value) {
                Ok(tags) => tags,
                Err(e) => {
                    diagnostics.add(Diagnostic::error("invalid tags").with_help(e.to_string()));
                    HashSet::new()
                }
            },
            None => HashSet::new(),
        };
        let parsed_inputs = match raw.0.value.inputs {
            Some(inputs) => match InputMatrix::parse(inputs) {
                Ok(inputs) => inputs,
                Err(d) => {
                    diagnostics.extend(d);
                    InputMatrix::default()
                }
            },
            None => InputMatrix::default(),
        };
        let parsed_assertions = {
            match raw.0.value.assertions {
                Some(assertions) => match Assertions::parse(assertions.value) {
                    Ok(assertions) => assertions,
                    Err(d) => {
                        diagnostics.extend(d);
                        Assertions::default()
                    }
                },
                _ => Assertions::default(),
            }
        };

        for (unknown, _) in raw.0.value.unknown_fields {
            diagnostics.add(unknown_field(unknown.0.defined.span(), &unknown.0.value));
        }

        let Some(name) = parsed_name else {
            diagnostics.add(
                Diagnostic::error("missing required field `name`")
                    .with_highlight(convert_yaml_span(raw.0.defined.span())),
            );
            return Err(diagnostics);
        };

        if diagnostics.is_empty() {
            Ok(Self {
                name: name.into(),
                tags: parsed_tags,
                inputs: parsed_inputs,
                assertions: parsed_assertions,
            })
        } else {
            Err(diagnostics)
        }
    }
}
