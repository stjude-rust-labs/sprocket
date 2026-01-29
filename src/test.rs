//! Facilities for unit testing WDL documents.

use std::collections::HashMap;
use std::collections::HashSet;
use std::iter::once;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use indexmap::IndexMap;
use itertools::Either;
use itertools::Itertools;
use regex::Regex;
use serde_yaml_ng::Mapping;
use serde_yaml_ng::Value;
use tracing::warn;
use wdl::analysis::document::Output;
use wdl::analysis::types::CompoundType;
use wdl::analysis::types::PrimitiveType;
use wdl::analysis::types::Type;

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
    pub assertions: Assertions,
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
}

/// Possible assertions for a test.
#[derive(Default, serde::Deserialize, Debug)]
pub(crate) struct Assertions {
    /// The expected exit code of the task (ignored when testing workflows).
    #[serde(default)]
    pub exit_code: i32,
    /// Whether a workflow should fail or not (ignored when testing tasks).
    #[serde(default)]
    pub should_fail: bool,
    /// Regular expressions that should match within STDOUT of the task (ignored
    /// when testing workflows).
    #[serde(default)]
    pub stdout: Vec<String>,
    /// Regular expressions that should match within STDERR of the task (ignored
    /// when testing workflows).
    #[serde(default)]
    pub stderr: Vec<String>,
    /// Assertions about WDL outputs.
    #[serde(default)]
    pub outputs: HashMap<String, Vec<OutputAssertion>>,
    /// A custom command to execute.
    // TODO(Ari): implement this assertion.
    #[allow(unused)]
    pub custom: Option<String>,
}

impl Assertions {
    /// Parse the assertions from the serde definitions.
    pub fn parse(
        self,
        is_workflow: bool,
        outputs: &IndexMap<String, Output>,
    ) -> Result<ParsedAssertions> {
        let mut stdout = None;
        let mut stderr = None;
        if is_workflow {
            if self.exit_code != 0 {
                warn!("ignoring `exit_code` assertion for workflow");
            }
            if !self.stdout.is_empty() {
                warn!("ignoring `stdout` assertion for workflow");
            }
            if !self.stderr.is_empty() {
                warn!("ignoring `stderr` assertion for workflow");
            }
        } else {
            if self.should_fail {
                warn!("ignoring `should_fail` assertion for task");
            }

            let stdout_regexs = self
                .stdout
                .iter()
                .map(|re| Regex::new(re).with_context(|| format!("compiling user regex: `{re}`")))
                .collect::<Result<Vec<_>>>()?;
            let stderr_regexs = self
                .stdout
                .iter()
                .map(|re| Regex::new(re).with_context(|| format!("compiling user regex: `{re}`")))
                .collect::<Result<Vec<_>>>()?;
            if !stdout_regexs.is_empty() {
                stdout = Some(stdout_regexs);
            }
            if !stderr_regexs.is_empty() {
                stderr = Some(stderr_regexs);
            }
        }

        for (name, assertions) in &self.outputs {
            let ty = outputs
                .get(name)
                .map(|o| o.ty())
                .ok_or(anyhow!("no output named `{}`", name))?;
            for assertion in assertions {
                assertion.validate_type_congruence(ty).with_context(|| {
                    format!("validating type congruence of `{name}` assertions")
                })?;
            }
        }

        Ok(ParsedAssertions {
            exit_code: self.exit_code,
            should_fail: self.should_fail,
            stdout,
            stderr,
            outputs: self.outputs,
            custom: self.custom,
        })
    }
}

/// Possible assertions on a WDL output.
#[derive(Debug, serde::Deserialize)]
pub(crate) enum OutputAssertion {
    /// Is the WDL value defined?
    ///
    /// Only supported for optional WDL types.
    Defined(bool),
    /// Is the WDL `Boolean` equal to this?
    BoolEquals(bool),
    /// Is the WDL `String` equal to this?
    // TODO(Ari): compile this as an RE?
    StrEquals(String),
    /// Is the WDL `Int` equal to this?
    IntEquals(i64),
    /// Is the WDL `Float` equal to this?
    FloatEquals(f64),
    /// Does the WDL `String` contiain this substring?
    // TODO(Ari): add `File` support
    // TODO(Ari): compile this as an RE?
    Contains(String),
    /// Does the WDL `File` or `Directory` have this basename?
    // TODO(Ari): should this support glob patterns?
    Name(String),
    /// Unpacks the first element of an `Array` and applies the inner assertion
    /// on that element.
    First(Box<OutputAssertion>),
    /// Unpacks the first element of an `Array` and applies the inner assertion
    /// on that element.
    Last(Box<OutputAssertion>),
    /// Does the WDL `String` or `Array` have this length?
    Length(usize),
}

impl std::fmt::Display for OutputAssertion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Defined(_) => write!(f, "Defined")?,
            Self::BoolEquals(_) => write!(f, "BoolEquals")?,
            Self::StrEquals(_) => write!(f, "StrEquals")?,
            Self::IntEquals(_) => write!(f, "IntEquals")?,
            Self::FloatEquals(_) => write!(f, "FloatEquals")?,
            Self::Contains(_) => write!(f, "Contains")?,
            Self::Name(_) => write!(f, "Name")?,
            Self::First(_) => write!(f, "First")?,
            Self::Last(_) => write!(f, "Last")?,
            Self::Length(_) => write!(f, "Length")?,
        }
        Ok(())
    }
}

impl OutputAssertion {
    /// Ensure this assertion supports the expected [`Type`] of the output.
    pub fn validate_type_congruence(&self, ty: &Type) -> Result<()> {
        match ty {
            Type::Primitive(prim_ty, optional) => {
                if matches!(self, Self::Defined(_)) && !*optional {
                    bail!("`{self}` assertion can only be used on an optional WDL type")
                }
                let mut valid = true;
                match prim_ty {
                    PrimitiveType::Boolean => {
                        if !matches!(self, Self::BoolEquals(_)) {
                            valid = false;
                        }
                    }
                    PrimitiveType::Directory => {
                        if !matches!(self, Self::Name(_)) {
                            valid = false;
                        }
                    }
                    PrimitiveType::File => {
                        if !matches!(self, Self::Name(_)) {
                            valid = false;
                        }
                    }
                    PrimitiveType::Float => {
                        if !matches!(self, Self::FloatEquals(_)) {
                            valid = false;
                        }
                    }
                    PrimitiveType::Integer => {
                        if !matches!(self, Self::IntEquals(_)) {
                            valid = false;
                        }
                    }
                    PrimitiveType::String => {
                        if !matches!(
                            self,
                            Self::Contains(_) | Self::StrEquals(_) | Self::Length(_)
                        ) {
                            valid = false;
                        }
                    }
                }
                if !valid {
                    bail!("`{self}` assertion cannot be used on `{prim_ty}` WDL type")
                }
            }
            Type::Compound(comp_ty, optional) => {
                if matches!(self, Self::Defined(_)) && !*optional {
                    bail!("`{self}` assertion can only be used on an optional WDL type")
                }
                let mut valid = true;
                match comp_ty {
                    CompoundType::Array(arr_ty) => match self {
                        Self::Length(_) => {}
                        Self::First(inner) | Self::Last(inner) => {
                            inner.validate_type_congruence(arr_ty.element_type())?;
                        }
                        _ => {
                            valid = false;
                        }
                    },
                    _ => {
                        todo!("other compound types")
                    }
                }
                if !valid {
                    bail!("`{self}` assertion cannot be used on `{comp_ty}` WDL type")
                }
            }
            Type::TypeNameRef(_custom_ty) => {
                todo!("struct/enum assertions")
            }
            _ => {
                unreachable!("unexpected type for an output")
            }
        }
        Ok(())
    }

    /// Evaluate this assertion for the given WDL engine output.
    ///
    /// # Panics
    ///
    /// Panics if the output's type is not supported by this assertion's
    /// variant. See [`validate_type_congruence`].
    pub fn evaluate(&self, output: &wdl::engine::Value) -> Result<()> {
        match self {
            Self::Defined(should_exist) => match (*should_exist, !output.is_none()) {
                (true, true) => {}
                (false, false) => {}
                (true, false) => bail!("output should be defined but is `None`"),
                (false, true) => bail!("output should be `None` but is defined"),
            },
            Self::BoolEquals(should_equal) => {
                let o = output.as_boolean().expect("type should be validated");
                if *should_equal != o {
                    bail!("output `{o}` does not equal assertion `{should_equal}`")
                }
            }
            Self::StrEquals(should_equal) => {
                let o = output.as_string().expect("type should be validated");
                if *should_equal != **o {
                    bail!("output `{o}` does not equal assertion `{should_equal}`")
                }
            }
            Self::IntEquals(should_equal) => {
                let o = output.as_integer().expect("type should be validated");
                if *should_equal != o {
                    bail!("output `{o}` does not equal assertion `{should_equal}`")
                }
            }
            Self::FloatEquals(should_equal) => {
                let o = output.as_float().expect("type should be validated");
                if *should_equal != o {
                    bail!("output `{o}` does not equal assertion `{should_equal}`")
                }
            }
            Self::Contains(should_contain) => {
                let o = output.as_string().expect("type should be validated");
                if !o.contains(should_contain) {
                    bail!("output `{o}` does not contain `{should_contain}`")
                }
            }
            Self::Name(expected_name) => {
                let path = if let Some(f) = output.as_file() {
                    f.as_str()
                } else if let Some(d) = output.as_directory() {
                    d.as_str()
                } else {
                    unreachable!("type should be validated")
                };
                let Some(real_name) = Path::new(path).file_name() else {
                    bail!("couldn't resolve filename from `{path}`")
                };
                let real_name = real_name.to_string_lossy();
                if *expected_name != real_name {
                    bail!("output has name `{real_name}`, not `{expected_name}`")
                }
            }
            Self::Length(expected_len) => {
                let real_len = if let Some(a) = output.as_array() {
                    a.len()
                } else if let Some(s) = output.as_string() {
                    s.chars().count()
                } else {
                    unreachable!("type should be validated")
                };
                if *expected_len != real_len {
                    bail!("output has length `{real_len}`, not `{expected_len}`")
                }
            }
            Self::First(inner) => {
                let o = output.as_array().expect("type should be validated");
                let first = if let Some(f) = o.as_slice().first() {
                    f
                } else {
                    bail!("can't take `{self}` of an empty `Array`")
                };
                inner.evaluate(first)?;
            }
            Self::Last(inner) => {
                let o = output.as_array().expect("type should be validated");
                let last = if let Some(l) = o.as_slice().last() {
                    l
                } else {
                    bail!("can't take `{self}` of an empty `Array`")
                };
                inner.evaluate(last)?;
            }
        }
        Ok(())
    }
}

/// Parsed assertions for a test.
#[derive(Debug)]
pub(crate) struct ParsedAssertions {
    /// The expected exit code of the task (ignored when testing workflows).
    pub exit_code: i32,
    /// Whether a workflow should fail or not (ignored when testing tasks).
    pub should_fail: bool,
    /// Regular expressions that should match within STDOUT of the task (ignored
    /// when testing workflows).
    pub stdout: Option<Vec<Regex>>,
    /// Regular expressions that should match within STDERR of the task (ignored
    /// when testing workflows).
    pub stderr: Option<Vec<Regex>>,
    /// Assertions about WDL outputs.
    pub outputs: HashMap<String, Vec<OutputAssertion>>,
    /// A custom command to execute.
    // TODO(Ari): implement this assertion.
    #[allow(unused)]
    pub custom: Option<String>,
}
