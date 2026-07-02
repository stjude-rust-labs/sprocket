//! Input parsing utilities.

use std::collections::HashSet;
use std::iter::once;

use itertools::Either;
use itertools::Itertools;
use schemars::JsonSchema;
use serde_json::Value;
use wdl_analysis::Diagnostics;
use wdl_ast::Diagnostic;

use crate::convert_yaml_span;
use crate::expected_mapping;
use crate::yaml::MaybeMap;
use crate::yaml::Spanned;
use crate::yaml::SpannedField;

/// A key is defined multiple times.
fn duplicate_key(first: &Spanned<String>, duplicate: &Spanned<String>) -> Diagnostic {
    let key = &first.0.value;
    Diagnostic::error(format!("the key `{key}` is defined multiple times"))
        .with_label(
            "first definition here",
            convert_yaml_span(first.0.defined.span()),
        )
        .with_label(
            format!("`{key}` redefined here"),
            convert_yaml_span(duplicate.0.defined.span()),
        )
}

/// The parser expected a YAML sequence.
fn expected_sequence(field: &Spanned<String>) -> Diagnostic {
    Diagnostic::error(format!(
        "expected a sequence of values in field `{}`",
        field.0.value
    ))
    .with_highlight(convert_yaml_span(field.0.defined.span()))
}

/// The entries in an input group are different lengths.
fn group_length_mismatch(
    first_group: &Spanned<String>,
    first_group_length: usize,
    group: &Spanned<String>,
    group_length: usize,
) -> Diagnostic {
    Diagnostic::error(format!("uneven length for group `{}`", group.0.value))
        .with_label(
            format!("`{}` has {first_group_length} entries", first_group.0.value),
            convert_yaml_span(first_group.0.defined.span()),
        )
        .with_label(
            format!("`{}` has {group_length} entries", group.0.value),
            convert_yaml_span(group.0.defined.span()),
        )
        .with_help(
            "all sequences in a group must have the same length, with the first group setting the \
             precedent",
        )
}

#[expect(dead_code, reason = "Only used for schema generation.")]
#[allow(clippy::missing_docs_in_private_items)]
mod json_schema {
    use std::borrow::Cow;
    use std::collections::HashMap;

    use schemars::JsonSchema;
    use schemars::Schema;
    use schemars::SchemaGenerator;
    use schemars::json_schema;
    use serde_json::Value;

    type InputSequence = Vec<Value>;
    type Group = HashMap<String, InputSequence>;

    pub(super) struct InputMatrix;

    impl JsonSchema for InputMatrix {
        fn inline_schema() -> bool {
            true
        }

        fn schema_name() -> Cow<'static, str> {
            Cow::Borrowed("InputMatrix")
        }

        fn json_schema(generator: &mut SchemaGenerator) -> Schema {
            let group_schema = generator.subschema_for::<Group>();
            let sequence_schema = generator.subschema_for::<InputSequence>();

            json_schema!({
                "type": "object",
                "patternProperties": {
                    "^[$].*": group_schema,
                    "^[^$].*": sequence_schema,
                },
                "additionalProperties": false,
            })
        }
    }
}

/// Represents a grouping of input sequences that must be iterated through
/// together.
#[derive(Clone, Debug)]
pub struct Group(Vec<InputSequence>);

impl Group {
    /// Gets the nth zipped sequence of the group.
    ///
    /// # Panics
    ///
    /// Panics if the given index is out of range for any inner sequence.
    pub fn nth(&self, index: usize) -> impl Iterator<Item = (&str, &Value)> + Clone {
        self.0
            .iter()
            .map(move |InputSequence { name, values }| (name.as_str(), &values[index]))
    }

    /// Gets the number of values in the group.
    pub fn len(&self) -> usize {
        // Assumption: all inner sequences are the same length
        self.0
            .first()
            .map(|InputSequence { values, .. }| values.len())
            .unwrap_or(0)
    }
}

/// A sequence of values for a single input.
#[derive(Clone, Debug)]
pub struct InputSequence {
    /// The name of the input in the WDL target.
    name: String,
    /// The values for the input.
    values: Vec<Value>,
}

/// Represents an input mapping.
#[derive(Clone, Debug)]
pub enum InputMapping {
    /// The mapping is a sequence of values.
    Sequence(InputSequence),
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
    pub fn nth(&self, index: usize) -> impl Iterator<Item = (&str, &Value)> + Clone {
        match self {
            Self::Sequence(InputSequence { name, values }) => {
                Either::Left(once((name.as_str(), &values[index])))
            }
            Self::Group(group) => Either::Right(group.nth(index)),
        }
    }

    /// Gets the number of values in the input mapping.
    pub fn len(&self) -> usize {
        match self {
            Self::Sequence(InputSequence { values, .. }) => values.len(),
            Self::Group(group) => group.len(),
        }
    }

    /// Gets an iterator over every sequence of key-value pairs in the mapping.
    pub fn iter(
        &self,
    ) -> impl Iterator<Item = impl Iterator<Item = (&str, &Value)> + Clone> + Clone {
        (0..self.len()).map(|i| self.nth(i))
    }
}

/// Represent a test input matrix.
#[derive(Clone, Default, Debug, JsonSchema)]
#[schemars(inline, with = "json_schema::InputMatrix")]
pub struct InputMatrix(Vec<InputMapping>);

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

    /// Parse the user-defined input matrix
    ///
    /// Each [`Mapping`] in `inputs` represents a set of input keys whose values
    /// should be iterated through together. The trivial case is a single
    /// input key with a set of possible values. Groups of inputs that
    /// should be iterated through together are designated by a YAML map key
    /// starting with `$`.
    pub(crate) fn parse(
        raw: SpannedField<MaybeMap<MaybeMap<Value>>>,
    ) -> anyhow::Result<InputMatrix, Diagnostics> {
        let mut keys = HashSet::new();
        let mut diagnostics = Diagnostics::default();

        let MaybeMap::Map(raw_map) = raw.value else {
            diagnostics.add(expected_mapping(&raw.key));
            return Err(diagnostics);
        };

        let mut results = Vec::new();
        for (key, val) in raw_map {
            if key.0.value.starts_with('$') {
                // group of inputs
                let map = match val.0.value {
                    MaybeMap::Map(map) => map,
                    MaybeMap::Other(_) => {
                        diagnostics.add(expected_mapping(&key));
                        continue;
                    }
                };
                let mut first_group = None;

                let mut group = Vec::new();
                for (nested_key, nested_val) in map {
                    if !keys.insert(nested_key.clone()) {
                        diagnostics.add(duplicate_key(
                            keys.get(&nested_key).expect("just verified it exists"),
                            &nested_key,
                        ));
                        continue;
                    }
                    let Value::Array(vals) = nested_val.0.value else {
                        diagnostics.add(expected_sequence(&key));
                        continue;
                    };

                    if let Some((ref first_span, first_len)) = first_group
                        && first_len != vals.len()
                    {
                        diagnostics.add(group_length_mismatch(
                            first_span,
                            first_len,
                            &nested_key,
                            vals.len(),
                        ));
                        continue;
                    } else {
                        first_group = Some((nested_key.clone(), vals.len()));
                    }

                    group.push(InputSequence {
                        name: nested_key.0.value,
                        values: vals.clone(),
                    });
                }

                results.push(InputMapping::Group(Group(group)));
            } else {
                // sequence of inputs
                if !keys.insert(key.clone()) {
                    diagnostics.add(duplicate_key(
                        keys.get(&key).expect("just verified it exists"),
                        &key,
                    ));
                    continue;
                }

                let MaybeMap::Other(value) = val.0.value else {
                    diagnostics.add(expected_sequence(&key));
                    continue;
                };

                let Value::Array(values) = value else {
                    diagnostics.add(expected_sequence(&key));
                    continue;
                };

                results.push(InputMapping::Sequence(InputSequence {
                    name: key.0.value,
                    values,
                }));
            }
        }

        if diagnostics.is_empty() {
            Ok(InputMatrix(results))
        } else {
            Err(diagnostics)
        }
    }
}
