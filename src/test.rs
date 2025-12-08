//! Facilities for unit testing WDL documents.

use std::collections::HashSet;
use std::iter::once;

use anyhow::Result;
use anyhow::bail;
use indexmap::IndexMap;
use itertools::Either;
use itertools::Itertools;
use serde_yaml_ng::Mapping;
use serde_yaml_ng::Value;

/// Represents a grouping of input sequences that must be iterated through
/// together.
struct Group(Vec<(String, Vec<Value>)>);

impl Group {
    /// Gets the nth zipped sequence of the group.
    ///
    /// # Panics
    ///
    /// Panics if the given index is out of range for any inner sequence.
    fn nth(&self, index: usize) -> impl Iterator<Item = (&str, &Value)> + Clone {
        self.0.iter().map(move |(n, s)| (n.as_str(), &s[index]))
    }

    /// Gets the number of values in the group.
    fn len(&self) -> usize {
        // Assumption: all inner sequences are the same length
        self.0.first().map(|(_, s)| s.len()).unwrap_or(0)
    }
}

/// Represents an input mapping.
enum InputMapping {
    /// The mapping is a sequence of values.
    Sequence(String, Vec<Value>),
    /// The mapping is a group.
    Group(Group),
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
            Self::Sequence(name, values) => Either::Left(once((name.as_str(), &values[index]))),
            Self::Group(group) => Either::Right(group.nth(index)),
        }
    }

    /// Gets the number of values in the input mapping.
    fn len(&self) -> usize {
        match self {
            Self::Sequence(_, values) => values.len(),
            Self::Group(group) => group.len(),
        }
    }

    /// Gets an iterator over every sequence of key-value pairs in the mapping.
    fn iter(&self) -> impl Iterator<Item = impl Iterator<Item = (&str, &Value)> + Clone> + Clone {
        (0..self.len()).map(|i| self.nth(i))
    }
}

/// Represent a test input matrix.
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

/// Collection of tests for an entire WDL document.
#[derive(serde::Deserialize, Debug)]
pub(crate) struct DocumentTests {
    /// Tasks or Workflows with test definitions.
    ///
    /// Each task or workflow may have one or more test definitions.
    #[serde(flatten)]
    pub entrypoints: IndexMap<String, Vec<TestDefinition>>,
}

/// A test definition. Defines at least a single execution, but may define many
/// executions.
#[derive(serde::Deserialize, Debug)]
pub(crate) struct TestDefinition {
    /// Name for the test.
    pub name: String,
    /// Any tags associated with the test.
    #[serde(default)]
    pub tags: HashSet<String>,
    /// Matrix of inputs to combinatorially execute.
    #[serde(default)]
    inputs: Mapping,
    /// Assertions (shared for all executions).
    ///
    /// If no assertions defined, it is assumed that failing execution for any
    /// reason is considered a test fail.
    #[serde(default)]
    assertions: Mapping,
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
                            Ok((k.to_string(), vals.clone()))
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(InputMapping::Group(Group(group)))
                } else {
                    // sequence of inputs
                    if !keys.insert(key) {
                        bail!("input `{key}` provided more than once");
                    }
                    let Value::Sequence(vals) = val else {
                        bail!("expected a YAML `Sequence`: `{val:?}`");
                    };
                    Ok(InputMapping::Sequence(key.to_string(), vals.clone()))
                }
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(InputMatrix(result))
    }

    /// Parse the defined assertions into an ordered map.
    pub fn parse_assertions(&self) -> Result<IndexMap<String, Value>> {
        self.assertions
            .iter()
            .map(|(key, val)| {
                let Value::String(key) = key else {
                    bail!("expected a YAML `String`: `{key:?}`");
                };
                Ok((key.clone(), val.clone()))
            })
            .collect()
    }
}
