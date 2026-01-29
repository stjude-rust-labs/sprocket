//! Facilities for making assertions about WDL executions.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use indexmap::IndexMap;
use regex::Regex;
use tracing::warn;
use wdl::analysis::document::Output;
use wdl::analysis::types::CompoundType;
use wdl::analysis::types::PrimitiveType;
use wdl::analysis::types::Type;

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
    /// Unpacks the first element of an `Array` and applies the inner assertion.
    First(Box<OutputAssertion>),
    /// Unpacks the last element of an `Array` and applies the inner assertion.
    Last(Box<OutputAssertion>),
    /// Does the WDL `String`, `Array`, or `Map` have this length?
    Length(usize),
    /// Unpacks the left element of a `Pair` and applies the inner assertion.
    Left(Box<OutputAssertion>),
    /// Unpacks the right element of a `Pair` and applies the inner assertion.
    Right(Box<OutputAssertion>),
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
            Self::Left(_) => write!(f, "Left")?,
            Self::Right(_) => write!(f, "Right")?,
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
                    CompoundType::Map(_map_ty) => match self {
                        Self::Length(_) => {}
                        _ => {
                            valid = false;
                        }
                    },
                    CompoundType::Pair(pair_ty) => match self {
                        Self::Left(inner) => {
                            inner.validate_type_congruence(pair_ty.left_type())?;
                        }
                        Self::Right(inner) => {
                            inner.validate_type_congruence(pair_ty.right_type())?;
                        }
                        _ => {
                            valid = false;
                        }
                    },
                    CompoundType::Custom(_) => {
                        bail!("custom WDL types (structs and enums) are not supported")
                    }
                }
                if !valid {
                    bail!("`{self}` assertion cannot be used on `{comp_ty}` WDL type")
                }
            }
            Type::TypeNameRef(_custom_ty) => {
                bail!("custom WDL types (structs and enums) are not supported")                        
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
                } else if let Some(m) = output.as_map() {
                    m.len()
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
            Self::Left(inner) => {
                let o = output.as_pair().expect("type should be validated");
                inner.evaluate(o.left())?;
            }
            Self::Right(inner) => {
                let o = output.as_pair().expect("type should be validated");
                inner.evaluate(o.right())?;
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
