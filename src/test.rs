//! Facilities for unit testing WDL documents.

use std::collections::HashSet;
use std::iter::once;
use std::sync::Arc;

use anyhow::Result;
use anyhow::bail;
use indexmap::IndexMap;
use itertools::Either;
use itertools::Itertools;
use schemars::JsonSchema;
use serde_yaml_ng::Mapping;
use serde_yaml_ng::Value;

mod assertions;

pub(crate) use assertions::Assertions;
pub(crate) use assertions::OutputAssertion;
pub(crate) use assertions::ParsedAssertions;

/// Collection of tests for an entire WDL document.
#[derive(serde::Deserialize, JsonSchema, Debug)]
#[schemars(
    title = "Sprocket test definitions",
    description = "JSON schema for `sprocket` test definition YAML files."
)]
pub(crate) struct DocumentTests {
    /// Tasks or Workflows with test definitions.
    ///
    /// Each task or workflow may have one or more test definitions.
    #[serde(flatten)]
    pub targets: IndexMap<String, Vec<TestDefinition>>,
}

/// A test definition, defining one or more executions.
#[derive(serde::Deserialize, JsonSchema, Debug)]
#[schemars(title = "Test definition")]
pub(crate) struct TestDefinition {
    /// Name for the test.
    pub name: Arc<str>,
    /// Any tags associated with the test.
    #[serde(default)]
    pub tags: HashSet<String>,
    /// Matrix of inputs to combinatorially execute.
    #[serde(default)]
    #[schemars(schema_with = "inputs_pattern_schema")]
    inputs: Mapping,
    /// Assertions (shared for all executions).
    ///
    /// If no assertions defined, it is assumed that failing execution for any
    /// reason is considered a test fail.
    #[serde(default)]
    pub assertions: Assertions,
}

/// The schema for test inputs.
fn inputs_pattern_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    let json_schema = serde_json::json!({
        "type": "object",
        "description": "Inputs defined as a mapping of input names to sequences of values.",
        "patternProperties": {
            "^[$].*": {
                "type": "object",
                "description": "A grouping of input sequences that will be iterated through together.",
                "additionalProperties": { "type": "array" }
            },
            "^[^$].*": {
                "type": "array",
                "description": "Standard inputs that will be combinatorially expanded."
            }
        },
        "additionalProperties": false
    });

    serde_json::from_value(json_schema).unwrap()
}

impl TestDefinition {
    /// Parse the user-defined input matrix
    ///
    /// Each [`Mapping`] in `inputs` represents a set of input keys whose values
    /// should be iterated through together. The trivial case is a single
    /// input key with a set of possible values. Groups of inputs that
    /// should be iterated through together are designated by a YAML map key
    /// starting with `$`.
    pub fn parse_inputs(&self) -> Result<InputMatrix> {
        let mut keys = HashSet::new();
        let result = self
            .inputs
            .iter()
            .map(|(key, val)| {
                let Value::String(key) = key else {
                    bail!("expected a YAML `String`: `{key:?}`");
                };
                if key.starts_with('$') {
                    // group of inputs
                    let Value::Mapping(map) = val else {
                        bail!("expected a YAML `Mapping`: `{val:?}`");
                    };
                    let mut group_len = None;
                    let group = map
                        .iter()
                        .map(|(nested_key, nested_val)| {
                            let Value::String(k) = nested_key else {
                                bail!("expected a YAML `String`: `{nested_key:?}`");
                            };
                            if !keys.insert(k) {
                                bail!("input `{key}` provided more than once");
                            }
                            let Value::Sequence(vals) = nested_val else {
                                bail!("expected a YAML `Sequence`: `{nested_val:?}`");
                            };
                            if let Some(len) = group_len
                                && len != vals.len()
                            {
                                bail!("sequences within `{key}` are of unequal length");
                            } else {
                                group_len = Some(vals.len());
                            }
                            Ok(InputSequence {
                                name: k.to_string(),
                                values: vals.clone(),
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(InputMapping::Group(group))
                } else {
                    // sequence of inputs
                    if !keys.insert(key) {
                        bail!("input `{key}` provided more than once");
                    }
                    let Value::Sequence(vals) = val else {
                        bail!("expected a YAML `Sequence`: `{val:?}`");
                    };
                    Ok(InputMapping::Sequence(InputSequence {
                        name: key.to_string(),
                        values: vals.clone(),
                    }))
                }
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(InputMatrix(result))
    }
}

/// A grouping of values to a WDL input.
#[derive(Debug)]
struct InputSequence {
    /// The name of the input.
    name: String,
    /// The values of the sequence.
    values: Vec<Value>,
}

/// Represents an input mapping.
#[derive(Debug)]
enum InputMapping {
    /// The mapping is a sequence of values.
    Sequence(InputSequence),
    /// Represents a grouping of input sequences that must be iterated through
    /// together.
    Group(Vec<InputSequence>),
}

impl InputMapping {
    /// Gets the nth sequence of the mapping.
    ///
    /// If the mapping is a sequence, this returns an iterator that yields a
    /// single name-value pair for the given index.
    ///
    /// If the mapping is a group, this returns an iterator that effectively
    /// zips the inner sequences at the given index.
    ///
    /// # Panics
    ///
    /// Panics if the given index is out of range.
    fn nth(&self, index: usize) -> impl Iterator<Item = (&str, &Value)> + Clone {
        match self {
            Self::Sequence(InputSequence { name, values }) => {
                Either::Left(once((name.as_str(), &values[index])))
            }
            Self::Group(group) => Either::Right(
                group
                    .iter()
                    .map(move |InputSequence { name, values }| (name.as_str(), &values[index])),
            ),
        }
    }

    /// Gets the number of values in the input mapping.
    fn len(&self) -> usize {
        match self {
            Self::Sequence(InputSequence { values, .. }) => values.len(),
            Self::Group(group) => {
                // Assumption: all inner sequences are the same length
                group
                    .first()
                    .map(|InputSequence { values, .. }| values.len())
                    .unwrap_or(0)
            }
        }
    }

    /// Gets an iterator over every sequence of key-value pairs in the mapping.
    fn iter(&self) -> impl Iterator<Item = impl Iterator<Item = (&str, &Value)> + Clone> + Clone {
        (0..self.len()).map(|i| self.nth(i))
    }
}

/// Represent a test input matrix.
#[derive(Default, Debug)]
pub(crate) struct InputMatrix(Vec<InputMapping>);

impl InputMatrix {
    /// Gets the cartesian product of the inputs.
    ///
    /// Returns an iterator that yields iterators of (name, value) pairs making
    /// up a set of inputs for a single execution.
    pub fn cartesian_product(&self) -> impl Iterator<Item = impl Iterator<Item = (&str, &Value)>> {
        // `multi_cartesian_product` returns a `Vec` of iterators of iterators.
        // here we flatten each element in the set so that we produce a single
        // iterator over the name value pairs that make up the set
        self.0
            .iter()
            .map(InputMapping::iter)
            .multi_cartesian_product()
            .map(|s| s.into_iter().flatten())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use schemars::schema_for;

    use super::*;

    #[test]
    fn public_schema_up_to_date() {
        let current_schema = schema_for!(DocumentTests);
        let current_schema_pretty = serde_json::to_string_pretty(&current_schema).unwrap();

        let public_schema_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("jsonschemas")
            .join("sprocket-test.json");
        let public_schema = std::fs::read_to_string(&public_schema_path).unwrap();
        pretty_assertions::assert_eq!(
            current_schema_pretty,
            public_schema.trim(),
            "The test YAML schema at `{}` is out of date! Update it with the output of `sprocket \
             dev test schema`.",
            public_schema_path.display()
        );
    }
}
