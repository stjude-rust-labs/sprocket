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

/// Matrix of inputs to combinatorially execute.
pub(crate) type InputMatrix = Vec<Mapping>;
pub(crate) type InputMapping = IndexMap<String, Vec<Value>>;

/// A test definition. Defines at least a single execution, but may define many
/// executions.
#[derive(serde::Deserialize, Debug)]
pub(crate) struct TestDefinition {
    /// Name for the test.
    pub name: String,
    /// Matrix of inputs to combinatorially execute.
    #[serde(default)]
    inputs: InputMatrix,
    /// Assertions (shared for all executions).
    ///
    /// If no assertions defined, it is assumed that failing execution for any
    /// reason is considered a test fail.
    #[serde(default)]
    assertions: Mapping,
}

impl TestDefinition {
    /// Parse the defined [`InputMatrix`] into an ordered map of input names to
    /// values.
    pub fn parse_inputs(&self) -> Vec<InputMapping> {
        let mut results = vec![];

        for mapping in &self.inputs {
            let kvs = mapping
                .iter()
                .map(|(k, v)| {
                    let Value::String(k) = k else {
                        panic!("expected string, got `{k:?}`");
                    };
                    let Value::Sequence(vs) = v else {
                        panic!("expected sequence, got `{k:?}`");
                    };
                    (k.to_string(), vs.to_vec())
                })
                .collect::<IndexMap<_, _>>();
            results.push(kvs);
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
