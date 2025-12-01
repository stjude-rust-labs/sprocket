//! Facilities for unit testing WDL documents.

use indexmap::IndexMap;
use serde_yaml_ng::Mapping;
use serde_yaml_ng::Value;
use tracing::warn;

/// Collection of tests for an entire WDL document.
#[derive(serde::Deserialize, Debug)]
pub(crate) struct DocumentTests {
    /// Tasks or Workflows with test definitions.
    ///
    /// Each task or workflow may have one or more test definitions.
    #[serde(flatten)]
    pub entrypoints: IndexMap<String, Vec<TestDefinition>>,
}

pub(crate) type InputMapping = IndexMap<String, Vec<Value>>;

/// A test definition. Defines at least a single execution, but may define many
/// executions.
#[derive(serde::Deserialize, Debug)]
pub(crate) struct TestDefinition {
    /// Name for the test.
    pub name: String,
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
    /// Parse the user-defined input matrix into a collection of ordered maps of
    /// input names to values.
    ///
    /// Each map represents a set of input keys whose values should be iterated
    /// through together. The trivial case is a single input key with a set
    /// of possible values. Groups of inputs that should be iterated through
    /// together are designated by a YAML map key starting with `$`.
    pub fn parse_inputs(&self) -> Vec<InputMapping> {
        let mut results = vec![];

        for (key, val) in &self.inputs {
            let Value::String(k) = key else {
                panic!("expected string, got `{key:?}`");
            };
            if k.starts_with('$') {
                let Value::Mapping(map) = val else {
                    panic!("expected mapping, got `{val:?}`");
                };
                let mut new_map = IndexMap::new();
                for (nested_key, nested_val) in map {
                    let Value::String(k) = nested_key else {
                        panic!("expected string, got `{nested_key:?}`");
                    };
                    let Value::Sequence(vals) = nested_val else {
                        panic!("expected sequence, got `{nested_val:?}`");
                    };
                    new_map.insert(k.to_string(), vals.to_vec());
                }
                results.push(new_map);
            } else {
                let Value::Sequence(vals) = val else {
                    panic!("expected sequence, got `{val:?}`");
                };
                results.push(IndexMap::from_iter(vec![(k.to_string(), vals.to_vec())]));
            }
        }
        results
    }

    /// Parse the defined assertions into an ordered map.
    pub fn parse_assertions(&self) -> IndexMap<String, Value> {
        self.assertions
            .iter()
            .filter_map(|(k, v)| {
                if !k.is_string() {
                    warn!("skipping non-string key: `{:?}`", k);
                    None
                } else {
                    let key = k.as_str().unwrap().to_string();
                    Some((key, v.clone()))
                }
            })
            .collect()
    }
}
