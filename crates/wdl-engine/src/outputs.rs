//! Implementation of workflow and task outputs.

use std::cmp::Ordering;

use indexmap::IndexMap;
use serde::Serialize;

use crate::Scope;
use crate::Value;

/// Represents outputs of a WDL workflow or task.
#[derive(Default, Debug)]
pub struct Outputs {
    /// The name of the outputs.
    ///
    /// This may be set to the name of the call in a workflow or the task name
    /// for a direct task execution.
    name: Option<String>,
    /// The map of output name to value.
    values: IndexMap<String, Value>,
}

impl Outputs {
    /// Constructs a new outputs collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the name of the outputs collection.
    ///
    /// Typically this is the name of the call in a workflow.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Iterates over the outputs in the collection.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> + use<'_> {
        self.values.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Gets an output of the collection by name.
    ///
    /// Returns `None` if an output with the given name doesn't exist.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.values.get(name)
    }

    /// Sorts the outputs according to a callback.
    pub(crate) fn sort_by(&mut self, mut cmp: impl FnMut(&str, &str) -> Ordering) {
        // We can sort unstable as none of the keys are equivalent in ordering; thus the
        // resulting sort is still considered to be stable
        self.values.sort_unstable_by(move |a, _, b, _| {
            let ordering = cmp(a, b);
            assert!(ordering != Ordering::Equal);
            ordering
        });
    }
}

impl Serialize for Outputs {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;

        let mut s = serializer.serialize_map(Some(self.values.len()))?;
        for (k, v) in &self.values {
            let v = crate::ValueSerializer::new(v, true);
            match &self.name {
                Some(prefix) => s.serialize_entry(&format!("{prefix}.{k}"), &v)?,
                None => s.serialize_entry(k, &v)?,
            }
        }

        s.end()
    }
}

impl From<Scope> for Outputs {
    fn from(scope: Scope) -> Self {
        Self {
            name: None,
            values: scope.into(),
        }
    }
}

impl FromIterator<(String, Value)> for Outputs {
    fn from_iter<T: IntoIterator<Item = (String, Value)>>(iter: T) -> Self {
        Self {
            name: None,
            values: iter.into_iter().collect(),
        }
    }
}
