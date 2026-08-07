//! Facilities for making assertions about WDL executions.

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use anyhow::bail;
use regex::Regex;
use schemars::JsonSchema;
use serde_json::Value;
use wdl_analysis::Diagnostics;
use wdl_analysis::document::Callable;
use wdl_analysis::types::CompoundType;
use wdl_analysis::types::PrimitiveType;
use wdl_analysis::types::Type;
use wdl_ast::Diagnostic;

use crate::convert_yaml_span;
use crate::expected_mapping;
use crate::unknown_field;
use crate::yaml::MaybeMap;
use crate::yaml::MaybeSequence;
use crate::yaml::Spanned;
use crate::yaml::SpannedField;
use crate::yaml::spanned_fields;

/// Type mismatch for a field value.
fn invalid_type(expected: &str, field: &Spanned<String>, value: &Value) -> Diagnostic {
    let found = match value {
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
        Value::Null => "null",
    };

    Diagnostic::error("invalid type")
        .with_help(format!(
            "`{}` must be a `{expected}`, found `{found:?}`",
            field.0.value
        ))
        .with_highlight(convert_yaml_span(field.0.defined.span()))
}

/// A specified assertion has no effect on workflow targets.
fn useless_workflow_assertion(field: &Spanned<String>) -> Diagnostic {
    Diagnostic::warning(format!("useless `{}` assertion", field.0.value))
        .with_help(format!(
            "`{}` assertions have no effect for workflows",
            field.0.value
        ))
        .with_highlight(convert_yaml_span(field.0.defined.span()))
}

/// A regex failed to compile.
fn invalid_regex(re: &str, field: &Spanned<Value>) -> Diagnostic {
    Diagnostic::error(format!("invalid regex: `{re}`"))
        .with_help("regexes must follow the syntax defined here: <https://docs.rs/regex/latest/regex/#syntax>")
        .with_highlight(convert_yaml_span(field.0.defined.span()))
}

/// A specified output is missing in the WDL document.
fn missing_output(name: &str, span: serde_saphyr::Span) -> Diagnostic {
    Diagnostic::error(format!("no output named `{name}`")).with_highlight(convert_yaml_span(span))
}

/// The type of an output is invalid.
fn invalid_output_type(name: &str, span: serde_saphyr::Span) -> Diagnostic {
    Diagnostic::error(format!(
        "failed to validate type congruence of `{name}` assertions"
    ))
    .with_highlight(convert_yaml_span(span))
}

/// Possible assertions for a test.
#[derive(Clone, Default, Debug, JsonSchema)]
pub struct Assertions {
    /// The expected exit code of the task (ignored when testing workflows).
    ///
    /// Defaults to `0` if not specified. Cannot be combined with
    /// `should_fail`.
    #[schemars(with = "i32", default = "i32::default")]
    pub exit_code: Option<SpannedField<i32>>,
    /// Whether the test is expected to fail.
    ///
    /// For workflows, any failure is expected.
    /// For tasks, any nonzero exit code is expected.
    ///
    /// Cannot be combined with `exit_code` (any value, including `exit_code:
    /// 0`).
    #[schemars(with = "bool", default = "bool::default")]
    pub should_fail: Option<SpannedField<bool>>,
    /// Regular expressions that should match within STDOUT of the task (ignored
    /// when testing workflows).
    #[schemars(default, with = "Vec<String>")]
    pub stdout: Option<SpannedField<Vec<Regex>>>,
    /// Regular expressions that should match within STDERR of the task (ignored
    /// when testing workflows).
    #[schemars(default, with = "Vec<String>")]
    pub stderr: Option<SpannedField<Vec<Regex>>>,
    /// Assertions about WDL outputs.
    #[schemars(default)]
    pub outputs: HashMap<Spanned<String>, Vec<OutputAssertion>>,
    /// A custom command to execute.
    // TODO(Ari): implement this assertion.
    #[allow(unused)]
    pub custom: Option<String>,
}

spanned_fields! {
    #[derive(Debug)]
    pub(crate) struct RawAssertions {
        pub exit_code: Option<SpannedField<Value>>,
        pub should_fail: Option<SpannedField<Value>>,
        pub stdout: Option<SpannedField<MaybeSequence<Value>>>,
        pub stderr: Option<SpannedField<MaybeSequence<Value>>>,
        pub outputs: Option<SpannedField<MaybeMap<Value>>>,
        pub custom: Option<SpannedField<Value>>,
    }
}

impl Assertions {
    /// Whether the test is expected to fail.
    pub fn should_fail(&self) -> bool {
        self.should_fail
            .as_ref()
            .is_some_and(|should_fail| should_fail.value)
    }

    /// The expected exit code of the task.
    ///
    /// If undefined, this defaults to `0`.
    pub fn exit_code(&self) -> i32 {
        self.exit_code
            .as_ref()
            .map_or(0, |exit_code| exit_code.value)
    }

    /// Parse the assertions from the serde definitions.
    pub(crate) fn parse(raw: RawAssertions) -> Result<Self, Diagnostics> {
        let mut diagnostics = Diagnostics::default();

        let mut exit_code = None;
        if let Some(raw_exit) = raw.exit_code {
            if let Some(code) = raw_exit.value.as_i64() {
                exit_code = Some(SpannedField {
                    key: raw_exit.key,
                    value: code as i32,
                });
            } else {
                diagnostics.add(invalid_type("integer", &raw_exit.key, &raw_exit.value));
            }
        }

        let mut should_fail = None;
        if let Some(raw_fail) = raw.should_fail {
            if let Some(fail_val) = raw_fail.value.as_bool() {
                should_fail = Some(SpannedField {
                    key: raw_fail.key,
                    value: fail_val,
                });
            } else {
                diagnostics.add(invalid_type("boolean", &raw_fail.key, &raw_fail.value));
            }
        }

        let mut outputs = HashMap::new();
        if let Some(raw_outputs) = raw.outputs {
            match raw_outputs.value {
                MaybeMap::Map(out_map) => {
                    for (name, assertions) in out_map {
                        let parsed_assertions = match serde_json::from_value::<Vec<OutputAssertion>>(
                            assertions.0.value,
                        ) {
                            Ok(a) => a,
                            Err(e) => {
                                diagnostics.add(
                                    invalid_output_type(&name.0.value, name.0.defined.span())
                                        .with_help(e.to_string()),
                                );
                                continue;
                            }
                        };

                        outputs.insert(name, parsed_assertions);
                    }
                }
                MaybeMap::Other(_) => {
                    diagnostics.add(expected_mapping(&raw_outputs.key));
                }
            }
        }

        let stdout = raw
            .stdout
            .and_then(|f| Self::parse_stdio(f, &mut diagnostics));
        let stderr = raw
            .stderr
            .and_then(|f| Self::parse_stdio(f, &mut diagnostics));

        let mut custom = None;
        if let Some(raw_custom) = raw.custom {
            match raw_custom.value {
                Value::String(custom_val) => {
                    custom = Some(custom_val);
                }
                other => {
                    diagnostics.add(invalid_type("string", &raw_custom.key, &other));
                }
            }
        }

        for (unknown_key, _) in raw.unknown_fields {
            diagnostics.add(unknown_field(
                unknown_key.0.defined.span(),
                &unknown_key.0.value,
            ));
        }

        if let (Some(should_fail), Some(exit_code)) = (&should_fail, &exit_code) {
            diagnostics.add(
                Diagnostic::error("cannot use `should_fail` with `exit_code`")
                    .with_label(
                        "`should_fail` specified here",
                        convert_yaml_span(should_fail.key.0.defined.span()),
                    )
                    .with_label(
                        "`exit_code` specified here",
                        convert_yaml_span(exit_code.key.0.defined.span()),
                    ),
            );
        }

        if diagnostics.is_empty() {
            Ok(Assertions {
                exit_code,
                should_fail,
                stdout,
                stderr,
                outputs,
                custom,
            })
        } else {
            Err(diagnostics)
        }
    }

    /// Parse an stdio (stdout/stderr) assertion.
    fn parse_stdio(
        field: SpannedField<MaybeSequence<Value>>,
        diagnostics: &mut Diagnostics,
    ) -> Option<SpannedField<Vec<Regex>>> {
        let raw_stdio = match field.value {
            MaybeSequence::Seq(raw_stdio) => raw_stdio,
            MaybeSequence::Other(other) => {
                diagnostics.add(invalid_type("array", &field.key, &other));
                return None;
            }
        };

        let mut regexes = Vec::with_capacity(raw_stdio.len());
        for raw_re in &raw_stdio {
            let re = match &raw_re.0.value {
                Value::String(re) => re,
                other => {
                    diagnostics.add(invalid_type("string", &field.key, other));
                    continue;
                }
            };

            let Ok(re) = Regex::new(re) else {
                diagnostics.add(invalid_regex(re, raw_re));
                continue;
            };
            regexes.push(re);
        }

        Some(SpannedField {
            key: field.key,
            value: regexes,
        })
    }

    /// Validate the assertions against the target [`Callable`].
    pub(crate) fn validate(&self, target: Callable<'_>, diagnostics: &mut Diagnostics) {
        let Assertions {
            exit_code,
            should_fail: _,
            stdout,
            stderr,
            outputs,
            custom: _,
        } = self;

        if target.is_workflow() {
            if let Some(exit_code) = exit_code {
                diagnostics.add(useless_workflow_assertion(&exit_code.key));
            }
            if let Some(stdout) = stdout {
                diagnostics.add(useless_workflow_assertion(&stdout.key));
            }
            if let Some(stderr) = stderr {
                diagnostics.add(useless_workflow_assertion(&stderr.key));
            }
        }

        for (output, assertions) in outputs {
            let Some(ty) = target.outputs().get(&output.0.value).map(|o| o.ty()) else {
                diagnostics.add(missing_output(&output.0.value, output.0.defined.span()));
                continue;
            };

            for assertion in assertions {
                if let Err(e) = assertion.validate_type_congruence(ty) {
                    diagnostics.add(
                        invalid_output_type(&output.0.value, output.0.defined.span())
                            .with_help(e.to_string()),
                    );
                }
            }
        }
    }
}

/// Possible assertions on a WDL output.
#[derive(Clone, Debug, serde::Deserialize, JsonSchema)]
pub enum OutputAssertion {
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
    /// Does the WDL `String` contain this substring?
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
    /// Is this WDL `Array`, `Map`, or `String` empty?
    Empty(bool),
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
            Self::Empty(_) => write!(f, "Empty")?,
        }
        Ok(())
    }
}

impl OutputAssertion {
    /// Ensure this assertion supports the expected [`Type`] of the output.
    pub fn validate_type_congruence(&self, ty: &Type) -> Result<()> {
        match ty {
            Type::Primitive(prim_ty, optional) => {
                if matches!(self, Self::Defined(_)) {
                    if !*optional {
                        bail!("`{self}` assertion can only be used on an optional WDL type")
                    } else {
                        return Ok(());
                    }
                }
                match (self, prim_ty) {
                    (Self::BoolEquals(_), PrimitiveType::Boolean)
                    | (Self::Name(_), PrimitiveType::Directory | PrimitiveType::File)
                    | (Self::FloatEquals(_), PrimitiveType::Float)
                    | (Self::IntEquals(_), PrimitiveType::Integer)
                    | (
                        Self::Contains(_) | Self::StrEquals(_) | Self::Length(_) | Self::Empty(_),
                        PrimitiveType::String,
                    ) => Ok(()),
                    _ => bail!("`{self}` assertion cannot be used on `{prim_ty}` WDL type"),
                }
            }
            Type::Compound(comp_ty, optional) => {
                if matches!(self, Self::Defined(_)) {
                    if !*optional {
                        bail!("`{self}` assertion can only be used on an optional WDL type")
                    } else {
                        return Ok(());
                    }
                }
                let mut valid = false;
                match comp_ty {
                    CompoundType::Array(arr_ty) => match self {
                        Self::Length(_) | Self::Empty(_) => {
                            valid = true;
                        }
                        Self::First(inner) | Self::Last(inner) => {
                            inner.validate_type_congruence(arr_ty.element_type())?;
                            valid = true;
                        }
                        _ => {}
                    },
                    #[allow(clippy::single_match)]
                    CompoundType::Map(_map_ty) => match self {
                        Self::Length(_) | Self::Empty(_) => {
                            valid = true;
                        }
                        _ => {}
                    },
                    CompoundType::Pair(pair_ty) => match self {
                        Self::Left(inner) => {
                            inner.validate_type_congruence(pair_ty.left_type())?;
                            valid = true;
                        }
                        Self::Right(inner) => {
                            inner.validate_type_congruence(pair_ty.right_type())?;
                            valid = true;
                        }
                        _ => {}
                    },
                    CompoundType::Custom(_) => {
                        bail!("custom WDL types (structs and enums) are not currently supported")
                    }
                }
                if !valid {
                    bail!("`{self}` assertion cannot be used on `{comp_ty}` WDL type")
                } else {
                    Ok(())
                }
            }
            Type::TypeNameRef(_custom_ty) => {
                bail!("custom WDL types (structs and enums) are not currently supported")
            }
            _ => {
                unreachable!("unexpected type for an output")
            }
        }
    }

    /// Evaluate this assertion for the given WDL engine output.
    ///
    /// # Panics
    ///
    /// Panics if the output's type is not supported by this assertion's
    /// variant. See [`validate_type_congruence`].
    pub fn evaluate(&self, output: &wdl_engine::Value) -> Result<()> {
        if output.is_none() && !matches!(self, Self::Defined(_)) {
            bail!("output is `None` but `{self}` assertion expects a defined WDL value");
        }
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
                if (*should_equal - o).abs() > 1e-3 {
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
            Self::Empty(should_be_empty) => {
                let is_empty = if let Some(s) = output.as_string() {
                    s.is_empty()
                } else if let Some(a) = output.as_array() {
                    a.is_empty()
                } else if let Some(m) = output.as_map() {
                    m.is_empty()
                } else {
                    unreachable!("type should be validated");
                };
                match (*should_be_empty, is_empty) {
                    (true, true) => {}
                    (false, false) => {}
                    (true, false) => {
                        bail!("output should be empty, but is not")
                    }
                    (false, true) => {
                        bail!("output should not be empty, but is")
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use wdl_analysis::types::ArrayType;
    use wdl_analysis::types::MapType;
    use wdl_analysis::types::PairType;
    use wdl_engine::Array;
    use wdl_engine::CompoundValue;
    use wdl_engine::HostPath;
    use wdl_engine::Map;
    use wdl_engine::Pair;
    use wdl_engine::PrimitiveValue;
    use wdl_engine::Value as EngineValue;

    use super::*;

    /// This struct is necessary to get `enum OutputAssertion` to parse
    /// consistently between tests and prod code. There is an upstream bug
    /// in the `serde` crates; using `serde(flatten)` in a parent struct
    /// changes which syntax is expected from nested externally tagged enums.
    /// Without `FlattenedWrapper`, `serde_yaml_ng` would expect
    /// OutputAssertions to be defined with YAML tag syntax instead of the map
    /// syntax used in these tests. See https://github.com/acatton/serde-yaml-ng/issues/14
    #[derive(Debug, serde::Deserialize)]
    struct FlattenedWrapper {
        #[serde(flatten)]
        inner: OutputAssertion,
    }

    #[test]
    fn defined_type_congruence() {
        let assertion: FlattenedWrapper = serde_saphyr::from_str("{ Defined: true }").unwrap();
        let assertion = assertion.inner;

        let ty = Type::Primitive(PrimitiveType::Boolean, true);
        assert!(assertion.validate_type_congruence(&ty).is_ok());
        let ty = Type::Primitive(PrimitiveType::Boolean, false);
        assert!(assertion.validate_type_congruence(&ty).is_err());
        let ty = Type::Compound(
            CompoundType::Array(ArrayType::new(Type::Primitive(
                PrimitiveType::Boolean,
                false,
            ))),
            true,
        );
        assert!(assertion.validate_type_congruence(&ty).is_ok());
        let ty = Type::Compound(
            CompoundType::Array(ArrayType::new(Type::Primitive(
                PrimitiveType::Boolean,
                false,
            ))),
            false,
        );
        assert!(assertion.validate_type_congruence(&ty).is_err());
    }

    #[test]
    fn bool_equals_type_congruence() {
        let assertion: FlattenedWrapper = serde_saphyr::from_str("{ BoolEquals: true }").unwrap();
        let assertion = assertion.inner;

        let ty = Type::Primitive(PrimitiveType::Boolean, true);
        assert!(assertion.validate_type_congruence(&ty).is_ok());
        let ty = Type::Primitive(PrimitiveType::Boolean, false);
        assert!(assertion.validate_type_congruence(&ty).is_ok());
        let ty = Type::Primitive(PrimitiveType::Directory, false);
        assert!(assertion.validate_type_congruence(&ty).is_err());
        let ty = Type::Compound(
            CompoundType::Array(ArrayType::new(Type::Primitive(
                PrimitiveType::Boolean,
                false,
            ))),
            true,
        );
        assert!(assertion.validate_type_congruence(&ty).is_err());
    }

    #[test]
    fn str_equals_type_congruence() {
        let assertion: FlattenedWrapper = serde_saphyr::from_str("{ StrEquals: foo }").unwrap();
        let assertion = assertion.inner;

        let ty = Type::Primitive(PrimitiveType::String, true);
        assert!(assertion.validate_type_congruence(&ty).is_ok());
        let ty = Type::Primitive(PrimitiveType::String, false);
        assert!(assertion.validate_type_congruence(&ty).is_ok());
        let ty = Type::Primitive(PrimitiveType::Integer, false);
        assert!(assertion.validate_type_congruence(&ty).is_err());
        let ty = Type::Primitive(PrimitiveType::File, true);
        assert!(assertion.validate_type_congruence(&ty).is_err());
        let ty = Type::Compound(
            CompoundType::Array(ArrayType::new(Type::Primitive(
                PrimitiveType::String,
                false,
            ))),
            true,
        );
        assert!(assertion.validate_type_congruence(&ty).is_err());
    }

    #[test]
    fn int_equals_type_congruence() {
        let assertion: FlattenedWrapper = serde_saphyr::from_str("{ IntEquals: 42 }").unwrap();
        let assertion = assertion.inner;

        let ty = Type::Primitive(PrimitiveType::Integer, true);
        assert!(assertion.validate_type_congruence(&ty).is_ok());
        let ty = Type::Primitive(PrimitiveType::Integer, false);
        assert!(assertion.validate_type_congruence(&ty).is_ok());
        let ty = Type::Primitive(PrimitiveType::String, false);
        assert!(assertion.validate_type_congruence(&ty).is_err());
        let ty = Type::Primitive(PrimitiveType::Float, true);
        assert!(assertion.validate_type_congruence(&ty).is_err());
        let ty = Type::Compound(
            CompoundType::Array(ArrayType::new(Type::Primitive(
                PrimitiveType::Integer,
                false,
            ))),
            true,
        );
        assert!(assertion.validate_type_congruence(&ty).is_err());
    }

    #[test]
    fn float_equals_type_congruence() {
        let assertion: FlattenedWrapper = serde_saphyr::from_str("{ FloatEquals: 42.42 }").unwrap();
        let assertion = assertion.inner;

        let ty = Type::Primitive(PrimitiveType::Float, true);
        assert!(assertion.validate_type_congruence(&ty).is_ok());
        let ty = Type::Primitive(PrimitiveType::Float, false);
        assert!(assertion.validate_type_congruence(&ty).is_ok());
        let ty = Type::Primitive(PrimitiveType::Integer, false);
        assert!(assertion.validate_type_congruence(&ty).is_err());
        let ty = Type::Primitive(PrimitiveType::File, true);
        assert!(assertion.validate_type_congruence(&ty).is_err());
        let ty = Type::Compound(
            CompoundType::Array(ArrayType::new(Type::Primitive(PrimitiveType::Float, false))),
            true,
        );
        assert!(assertion.validate_type_congruence(&ty).is_err());
    }

    #[test]
    fn contains_type_congruence() {
        let assertion: FlattenedWrapper = serde_saphyr::from_str("{ Contains: foobar }").unwrap();
        let assertion = assertion.inner;

        let ty = Type::Primitive(PrimitiveType::String, true);
        assert!(assertion.validate_type_congruence(&ty).is_ok());
        let ty = Type::Primitive(PrimitiveType::String, false);
        assert!(assertion.validate_type_congruence(&ty).is_ok());
        let ty = Type::Primitive(PrimitiveType::Integer, false);
        assert!(assertion.validate_type_congruence(&ty).is_err());
        let ty = Type::Primitive(PrimitiveType::File, true);
        assert!(assertion.validate_type_congruence(&ty).is_err());
        let ty = Type::Compound(
            CompoundType::Array(ArrayType::new(Type::Primitive(
                PrimitiveType::String,
                false,
            ))),
            true,
        );
        assert!(assertion.validate_type_congruence(&ty).is_err());
    }

    #[test]
    fn name_type_congruence() {
        let assertion: FlattenedWrapper = serde_saphyr::from_str("{ Name: foobar }").unwrap();
        let assertion = assertion.inner;

        let ty = Type::Primitive(PrimitiveType::File, true);
        assert!(assertion.validate_type_congruence(&ty).is_ok());
        let ty = Type::Primitive(PrimitiveType::Directory, false);
        assert!(assertion.validate_type_congruence(&ty).is_ok());
        let ty = Type::Primitive(PrimitiveType::String, false);
        assert!(assertion.validate_type_congruence(&ty).is_err());
        let ty = Type::Primitive(PrimitiveType::Integer, true);
        assert!(assertion.validate_type_congruence(&ty).is_err());
        let ty = Type::Compound(
            CompoundType::Array(ArrayType::new(Type::Primitive(PrimitiveType::File, false))),
            true,
        );
        assert!(assertion.validate_type_congruence(&ty).is_err());
    }

    #[test]
    fn first_last_type_congruence() {
        let first: FlattenedWrapper =
            serde_saphyr::from_str("{ First: { StrEquals: foobar } }").unwrap();
        let first = first.inner;
        let last: FlattenedWrapper = serde_saphyr::from_str("{ Last: { Contains: bar } }").unwrap();
        let last = last.inner;

        let ty = Type::Compound(
            CompoundType::Array(ArrayType::new(Type::Primitive(
                PrimitiveType::String,
                false,
            ))),
            false,
        );
        assert!(first.validate_type_congruence(&ty).is_ok());
        assert!(last.validate_type_congruence(&ty).is_ok());

        let ty = Type::Compound(
            CompoundType::Array(ArrayType::new(Type::Primitive(PrimitiveType::Float, true))),
            true,
        );
        assert!(first.validate_type_congruence(&ty).is_err());
        assert!(last.validate_type_congruence(&ty).is_err());

        let ty = Type::Compound(
            CompoundType::Map(MapType::new(PrimitiveType::Integer, PrimitiveType::Boolean)),
            true,
        );
        assert!(first.validate_type_congruence(&ty).is_err());
        assert!(last.validate_type_congruence(&ty).is_err());

        let ty = Type::Primitive(PrimitiveType::String, false);
        assert!(first.validate_type_congruence(&ty).is_err());
        assert!(last.validate_type_congruence(&ty).is_err());
    }

    #[test]
    fn length_type_congruence() {
        let assertion: FlattenedWrapper = serde_saphyr::from_str("{ Length: 7 }").unwrap();
        let assertion = assertion.inner;

        let ty = Type::Compound(
            CompoundType::Array(ArrayType::new(Type::Primitive(
                PrimitiveType::Directory,
                true,
            ))),
            false,
        );
        assert!(assertion.validate_type_congruence(&ty).is_ok());

        let ty = Type::Compound(
            CompoundType::Map(MapType::new(PrimitiveType::Integer, PrimitiveType::Boolean)),
            true,
        );
        assert!(assertion.validate_type_congruence(&ty).is_ok());

        let ty = Type::Primitive(PrimitiveType::String, false);
        assert!(assertion.validate_type_congruence(&ty).is_ok());

        let ty = Type::Primitive(PrimitiveType::Float, false);
        assert!(assertion.validate_type_congruence(&ty).is_err());
    }

    #[test]
    fn left_right_type_congruence() {
        let left: FlattenedWrapper =
            serde_saphyr::from_str("{ Left: { StrEquals: foobar } }").unwrap();
        let left = left.inner;
        let right: FlattenedWrapper =
            serde_saphyr::from_str("{ Right: { Contains: bar } }").unwrap();
        let right = right.inner;

        let ty = Type::Compound(
            CompoundType::Pair(PairType::new(
                PrimitiveType::String,
                Type::Primitive(PrimitiveType::String, true),
            )),
            true,
        );
        assert!(left.validate_type_congruence(&ty).is_ok());
        assert!(right.validate_type_congruence(&ty).is_ok());

        let ty = Type::Compound(
            CompoundType::Pair(PairType::new(PrimitiveType::Float, PrimitiveType::Integer)),
            false,
        );
        assert!(left.validate_type_congruence(&ty).is_err());
        assert!(right.validate_type_congruence(&ty).is_err());

        let ty = Type::Compound(
            CompoundType::Map(MapType::new(PrimitiveType::Integer, PrimitiveType::Boolean)),
            true,
        );
        assert!(left.validate_type_congruence(&ty).is_err());
        assert!(right.validate_type_congruence(&ty).is_err());

        let ty = Type::Primitive(PrimitiveType::String, false);
        assert!(left.validate_type_congruence(&ty).is_err());
        assert!(right.validate_type_congruence(&ty).is_err());
    }

    #[test]
    fn empty_type_congruence() {
        let assertion: FlattenedWrapper = serde_saphyr::from_str("{ Empty: true }").unwrap();
        let assertion = assertion.inner;

        let ty = Type::Compound(
            CompoundType::Array(ArrayType::new(Type::Primitive(
                PrimitiveType::Directory,
                true,
            ))),
            false,
        );
        assert!(assertion.validate_type_congruence(&ty).is_ok());

        let ty = Type::Compound(
            CompoundType::Map(MapType::new(PrimitiveType::Integer, PrimitiveType::Boolean)),
            true,
        );
        assert!(assertion.validate_type_congruence(&ty).is_ok());

        let ty = Type::Primitive(PrimitiveType::String, false);
        assert!(assertion.validate_type_congruence(&ty).is_ok());

        let ty = Type::Primitive(PrimitiveType::Float, false);
        assert!(assertion.validate_type_congruence(&ty).is_err());

        let ty = Type::Compound(
            CompoundType::Pair(PairType::new(PrimitiveType::Float, PrimitiveType::Integer)),
            false,
        );
        assert!(assertion.validate_type_congruence(&ty).is_err());
    }

    #[test]
    fn evaluate_defined() {
        let assertion: FlattenedWrapper = serde_saphyr::from_str("{ Defined: false }").unwrap();
        let is_none = assertion.inner;
        let assertion: FlattenedWrapper = serde_saphyr::from_str("{ Defined: true }").unwrap();
        let is_defined = assertion.inner;

        let o = EngineValue::new_none(Type::Primitive(PrimitiveType::Boolean, true));
        assert!(is_none.evaluate(&o).is_ok());
        assert!(is_defined.evaluate(&o).is_err());
        let o = EngineValue::Primitive(PrimitiveValue::Boolean(true));
        assert!(is_none.evaluate(&o).is_err());
        assert!(is_defined.evaluate(&o).is_ok());
        let o = EngineValue::new_none(Type::Compound(
            CompoundType::Array(ArrayType::new(Type::Primitive(
                PrimitiveType::Boolean,
                false,
            ))),
            true,
        ));
        assert!(is_none.evaluate(&o).is_ok());
        assert!(is_defined.evaluate(&o).is_err());
        let o = EngineValue::Compound(CompoundValue::Array(
            Array::new(
                ArrayType::new(Type::Primitive(PrimitiveType::Boolean, false)),
                vec![EngineValue::Primitive(PrimitiveValue::Boolean(true))],
            )
            .unwrap(),
        ));
        assert!(is_none.evaluate(&o).is_err());
        assert!(is_defined.evaluate(&o).is_ok());
    }

    #[test]
    fn evaluate_bool_equals() {
        let assertion: FlattenedWrapper = serde_saphyr::from_str("{ BoolEquals: true }").unwrap();
        let is_true = assertion.inner;
        let assertion: FlattenedWrapper = serde_saphyr::from_str("{ BoolEquals: false }").unwrap();
        let is_false = assertion.inner;

        let o = EngineValue::Primitive(PrimitiveValue::Boolean(true));
        assert!(is_true.evaluate(&o).is_ok());
        assert!(is_false.evaluate(&o).is_err());
        let o = EngineValue::Primitive(PrimitiveValue::Boolean(false));
        assert!(is_true.evaluate(&o).is_err());
        assert!(is_false.evaluate(&o).is_ok());

        let o = EngineValue::new_none(Type::Primitive(PrimitiveType::Boolean, true));
        assert!(is_true.evaluate(&o).is_err());
        assert!(is_false.evaluate(&o).is_err());
    }

    #[test]
    fn evaluate_str_equals() {
        let assertion: FlattenedWrapper = serde_saphyr::from_str("{ StrEquals: foo }").unwrap();
        let is_foo = assertion.inner;

        let o = EngineValue::Primitive(PrimitiveValue::new_string("foo"));
        assert!(is_foo.evaluate(&o).is_ok());
        let o = EngineValue::Primitive(PrimitiveValue::new_string("not foo"));
        assert!(is_foo.evaluate(&o).is_err());
    }

    #[test]
    fn evaluate_int_equals() {
        let assertion: FlattenedWrapper = serde_saphyr::from_str("{ IntEquals: 42 }").unwrap();
        let assertion = assertion.inner;

        let o = EngineValue::Primitive(PrimitiveValue::Integer(42));
        assert!(assertion.evaluate(&o).is_ok());
        let o = EngineValue::Primitive(PrimitiveValue::Integer(0));
        assert!(assertion.evaluate(&o).is_err());
    }

    #[test]
    fn evaluate_float_equals() {
        let assertion: FlattenedWrapper = serde_saphyr::from_str("{ FloatEquals: 42.42 }").unwrap();
        let assertion = assertion.inner;

        let o = EngineValue::Primitive(PrimitiveValue::Float(42.42.into()));
        assert!(assertion.evaluate(&o).is_ok());
        let o = EngineValue::Primitive(PrimitiveValue::Float(0.into()));
        assert!(assertion.evaluate(&o).is_err());
    }

    #[test]
    fn evaluate_contains() {
        let assertion: FlattenedWrapper = serde_saphyr::from_str("{ Contains: foo }").unwrap();
        let assertion = assertion.inner;

        let o = EngineValue::Primitive(PrimitiveValue::new_string("foo"));
        assert!(assertion.evaluate(&o).is_ok());
        let o = EngineValue::Primitive(PrimitiveValue::new_string("has foo"));
        assert!(assertion.evaluate(&o).is_ok());
        let o = EngineValue::Primitive(PrimitiveValue::new_string("bar"));
        assert!(assertion.evaluate(&o).is_err());
    }

    #[test]
    fn evaluate_name() {
        let assertion: FlattenedWrapper = serde_saphyr::from_str("{ Name: foobar }").unwrap();
        let assertion = assertion.inner;

        let o = EngineValue::Primitive(PrimitiveValue::File(HostPath::new("foobar")));
        assert!(assertion.evaluate(&o).is_ok());
        let o = EngineValue::Primitive(PrimitiveValue::Directory(HostPath::new("foobar")));
        assert!(assertion.evaluate(&o).is_ok());
        let o = EngineValue::Primitive(PrimitiveValue::File(HostPath::new("not_foobar")));
        assert!(assertion.evaluate(&o).is_err());
        let o = EngineValue::Primitive(PrimitiveValue::Directory(HostPath::new("not_foobar")));
        assert!(assertion.evaluate(&o).is_err());
    }

    #[test]
    fn evaluate_length() {
        let assertion: FlattenedWrapper = serde_saphyr::from_str("{ Length: 2 }").unwrap();
        let assertion = assertion.inner;

        let o = EngineValue::Primitive(PrimitiveValue::new_string("sh"));
        assert!(assertion.evaluate(&o).is_ok());
        let o = EngineValue::Primitive(PrimitiveValue::new_string("too long"));
        assert!(assertion.evaluate(&o).is_err());

        let o = EngineValue::Compound(CompoundValue::Array(
            Array::new(
                ArrayType::new(Type::Primitive(PrimitiveType::Boolean, false)),
                vec![
                    EngineValue::Primitive(PrimitiveValue::Boolean(true)),
                    EngineValue::Primitive(PrimitiveValue::Boolean(true)),
                ],
            )
            .unwrap(),
        ));
        assert!(assertion.evaluate(&o).is_ok());
        let o = EngineValue::Compound(CompoundValue::Array(
            Array::new(
                ArrayType::new(Type::Primitive(PrimitiveType::Boolean, false)),
                vec![EngineValue::Primitive(PrimitiveValue::Boolean(true))],
            )
            .unwrap(),
        ));
        assert!(assertion.evaluate(&o).is_err());

        let o = EngineValue::Compound(CompoundValue::Map(
            Map::new(
                MapType::new(
                    Type::Primitive(PrimitiveType::Integer, false),
                    Type::Primitive(PrimitiveType::Boolean, false),
                ),
                vec![
                    (
                        PrimitiveValue::Integer(0),
                        EngineValue::Primitive(PrimitiveValue::Boolean(true)),
                    ),
                    (
                        PrimitiveValue::Integer(1),
                        EngineValue::Primitive(PrimitiveValue::Boolean(false)),
                    ),
                ],
            )
            .unwrap(),
        ));
        assert!(assertion.evaluate(&o).is_ok());
        let o = EngineValue::Compound(CompoundValue::Map(
            Map::new(
                MapType::new(
                    Type::Primitive(PrimitiveType::Integer, false),
                    Type::Primitive(PrimitiveType::Boolean, false),
                ),
                vec![(PrimitiveValue::Integer(0), PrimitiveValue::Boolean(true))],
            )
            .unwrap(),
        ));
        assert!(assertion.evaluate(&o).is_err());
    }

    #[test]
    fn evaluate_first_last() {
        let assertion: FlattenedWrapper =
            serde_saphyr::from_str("{ First: { IntEquals: 42 } }").unwrap();
        let first = assertion.inner;
        let assertion: FlattenedWrapper =
            serde_saphyr::from_str("{ Last: { IntEquals: 2 } }").unwrap();
        let last = assertion.inner;

        let o = EngineValue::Compound(CompoundValue::Array(
            Array::new(
                ArrayType::new(Type::Primitive(PrimitiveType::Integer, false)),
                vec![
                    EngineValue::Primitive(PrimitiveValue::Integer(42)),
                    EngineValue::Primitive(PrimitiveValue::Integer(2)),
                ],
            )
            .unwrap(),
        ));
        assert!(first.evaluate(&o).is_ok());
        assert!(last.evaluate(&o).is_ok());

        let o = EngineValue::Compound(CompoundValue::Array(
            Array::new(
                ArrayType::new(Type::Primitive(PrimitiveType::Integer, false)),
                vec![EngineValue::Primitive(PrimitiveValue::Integer(42))],
            )
            .unwrap(),
        ));
        assert!(first.evaluate(&o).is_ok());
        assert!(last.evaluate(&o).is_err());

        let o = EngineValue::Compound(CompoundValue::Array(
            Array::new(
                ArrayType::new(Type::Primitive(PrimitiveType::Integer, false)),
                None::<EngineValue>,
            )
            .unwrap(),
        ));
        assert!(first.evaluate(&o).is_err());
        assert!(last.evaluate(&o).is_err());
    }

    #[test]
    fn evaluate_left_right() {
        let assertion: FlattenedWrapper =
            serde_saphyr::from_str("{ Left: { Contains: foo } }").unwrap();
        let left = assertion.inner;
        let assertion: FlattenedWrapper =
            serde_saphyr::from_str("{ Right: { Length: 6 } }").unwrap();
        let right = assertion.inner;

        let o = EngineValue::Compound(CompoundValue::Pair(
            Pair::new(
                PairType::new(
                    Type::Primitive(PrimitiveType::String, true),
                    Type::Primitive(PrimitiveType::String, true),
                ),
                PrimitiveValue::new_string("foobar quzwack"),
                PrimitiveValue::new_string("foobar"),
            )
            .unwrap(),
        ));
        assert!(left.evaluate(&o).is_ok());
        assert!(right.evaluate(&o).is_ok());

        let o = EngineValue::Compound(CompoundValue::Pair(
            Pair::new(
                PairType::new(
                    Type::Primitive(PrimitiveType::String, true),
                    Type::Primitive(PrimitiveType::String, true),
                ),
                None,
                None,
            )
            .unwrap(),
        ));
        assert!(left.evaluate(&o).is_err());
        assert!(right.evaluate(&o).is_err());
    }

    #[test]
    fn evaluate_empty() {
        let assertion: FlattenedWrapper = serde_saphyr::from_str("{ Empty: true }").unwrap();
        let assertion = assertion.inner;

        let o = EngineValue::Primitive(PrimitiveValue::new_string(""));
        assert!(assertion.evaluate(&o).is_ok());
        let o = EngineValue::Primitive(PrimitiveValue::new_string("not empty"));
        assert!(assertion.evaluate(&o).is_err());

        let o = EngineValue::Compound(CompoundValue::Array(
            Array::new(
                ArrayType::new(Type::Primitive(PrimitiveType::Integer, false)),
                None::<EngineValue>,
            )
            .unwrap(),
        ));
        assert!(assertion.evaluate(&o).is_ok());

        let o = EngineValue::Compound(CompoundValue::Array(
            Array::new(
                ArrayType::new(Type::Primitive(PrimitiveType::Integer, false)),
                vec![EngineValue::Primitive(PrimitiveValue::Integer(42))],
            )
            .unwrap(),
        ));
        assert!(assertion.evaluate(&o).is_err());

        let o = EngineValue::Compound(CompoundValue::Map(
            Map::new(
                MapType::new(
                    Type::Primitive(PrimitiveType::Integer, false),
                    Type::Primitive(PrimitiveType::Boolean, false),
                ),
                None::<(PrimitiveValue, EngineValue)>,
            )
            .unwrap(),
        ));
        assert!(assertion.evaluate(&o).is_ok());

        let o = EngineValue::Compound(CompoundValue::Map(
            Map::new(
                MapType::new(
                    Type::Primitive(PrimitiveType::Integer, false),
                    Type::Primitive(PrimitiveType::Boolean, false),
                ),
                vec![(PrimitiveValue::Integer(0), PrimitiveValue::Boolean(true))],
            )
            .unwrap(),
        ));
        assert!(assertion.evaluate(&o).is_err());
    }
}
