//! Implementation of the WDL runtime and values.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::fmt;
use std::hash::Hash;
use std::hash::Hasher;
use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use futures::FutureExt;
use futures::StreamExt as _;
use futures::TryStreamExt as _;
use futures::future::BoxFuture;
use indexmap::IndexMap;
use itertools::Either;
use ordered_float::OrderedFloat;
use serde::ser::SerializeMap;
use serde::ser::SerializeSeq;
use wdl_analysis::stdlib::STDLIB as ANALYSIS_STDLIB;
use wdl_analysis::types::ArrayType;
use wdl_analysis::types::CallType;
use wdl_analysis::types::Coercible as _;
use wdl_analysis::types::CompoundType;
use wdl_analysis::types::CustomType;
use wdl_analysis::types::EnumType;
use wdl_analysis::types::HiddenType;
use wdl_analysis::types::MapType;
use wdl_analysis::types::Optional;
use wdl_analysis::types::PrimitiveType;
use wdl_analysis::types::Type;
use wdl_analysis::types::v1::task_member_type_post_evaluation;
use wdl_ast::AstToken;
use wdl_ast::SupportedVersion;
use wdl_ast::TreeNode;
use wdl_ast::v1;
use wdl_ast::v1::TASK_FIELD_ATTEMPT;
use wdl_ast::v1::TASK_FIELD_CONTAINER;
use wdl_ast::v1::TASK_FIELD_CPU;
use wdl_ast::v1::TASK_FIELD_DISKS;
use wdl_ast::v1::TASK_FIELD_END_TIME;
use wdl_ast::v1::TASK_FIELD_EXT;
use wdl_ast::v1::TASK_FIELD_FPGA;
use wdl_ast::v1::TASK_FIELD_GPU;
use wdl_ast::v1::TASK_FIELD_ID;
use wdl_ast::v1::TASK_FIELD_MAX_RETRIES;
use wdl_ast::v1::TASK_FIELD_MEMORY;
use wdl_ast::v1::TASK_FIELD_META;
use wdl_ast::v1::TASK_FIELD_NAME;
use wdl_ast::v1::TASK_FIELD_PARAMETER_META;
use wdl_ast::v1::TASK_FIELD_PREVIOUS;
use wdl_ast::v1::TASK_FIELD_RETURN_CODE;
use wdl_ast::version::V1;

use crate::EvaluationContext;
use crate::GuestPath;
use crate::HostPath;
use crate::Outputs;
use crate::backend::TaskExecutionConstraints;
use crate::http::Transferer;
use crate::path;

/// Implemented on coercible values.
pub trait Coercible: Sized {
    /// Coerces the value into the given type.
    ///
    /// If the provided evaluation context is `None`, host to guest and guest to
    /// host translation is not performed; `File` and `Directory` values will
    /// coerce directly to string.
    ///
    /// Returns an error if the coercion is not supported.
    fn coerce(&self, context: Option<&dyn EvaluationContext>, target: &Type) -> Result<Self>;
}

/// Represents a WDL runtime value.
///
/// Values are cheap to clone.
#[derive(Debug, Clone)]
pub enum Value {
    /// The value is a literal `None` value.
    ///
    /// The contained type is expected to be an optional type.
    None(Type),
    /// The value is a primitive value.
    Primitive(PrimitiveValue),
    /// The value is a compound value.
    Compound(CompoundValue),
    /// The value is a hidden value.
    ///
    /// A hidden value is one that has a hidden (i.e. not expressible in WDL
    /// source) type.
    Hidden(HiddenValue),
    /// The value is the outputs of a call.
    Call(CallValue),
    /// The value is a reference to a user-defined type.
    TypeNameRef(Type),
}

impl Value {
    /// Creates an object from an iterator of V1 AST metadata items.
    ///
    /// # Panics
    ///
    /// Panics if the metadata value contains an invalid numeric value.
    pub fn from_v1_metadata<N: TreeNode>(value: &v1::MetadataValue<N>) -> Self {
        match value {
            v1::MetadataValue::Boolean(v) => v.value().into(),
            v1::MetadataValue::Integer(v) => v.value().expect("number should be in range").into(),
            v1::MetadataValue::Float(v) => v.value().expect("number should be in range").into(),
            v1::MetadataValue::String(v) => PrimitiveValue::new_string(
                v.text()
                    .expect("metadata strings shouldn't have placeholders")
                    .text(),
            )
            .into(),
            v1::MetadataValue::Null(_) => Self::new_none(Type::None),
            v1::MetadataValue::Object(o) => Object::from_v1_metadata(o.items()).into(),
            v1::MetadataValue::Array(a) => Array::new_unchecked(
                ANALYSIS_STDLIB.array_object_type().clone(),
                a.elements().map(|v| Value::from_v1_metadata(&v)).collect(),
            )
            .into(),
        }
    }

    /// Constructs a new `None` value with the given type.
    ///
    /// # Panics
    ///
    /// Panics if the provided type is not optional.
    pub fn new_none(ty: Type) -> Self {
        assert!(ty.is_optional(), "the provided `None` type is not optional");
        Self::None(ty)
    }

    /// Gets the type of the value.
    pub fn ty(&self) -> Type {
        match self {
            Self::None(ty) => ty.clone(),
            Self::Primitive(v) => v.ty(),
            Self::Compound(v) => v.ty(),
            Self::Hidden(v) => v.ty(),
            Self::Call(v) => Type::Call(v.ty.clone()),
            Self::TypeNameRef(ty) => ty.clone(),
        }
    }

    /// Determines if the value is `None`.
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None(_))
    }

    /// Gets the value as a primitive value.
    ///
    /// Returns `None` if the value is not a primitive value.
    pub fn as_primitive(&self) -> Option<&PrimitiveValue> {
        match self {
            Self::Primitive(v) => Some(v),
            _ => None,
        }
    }

    /// Gets the value as a compound value.
    ///
    /// Returns `None` if the value is not a compound value.
    pub fn as_compound(&self) -> Option<&CompoundValue> {
        match self {
            Self::Compound(v) => Some(v),
            _ => None,
        }
    }

    /// Gets the value as a `Boolean`.
    ///
    /// Returns `None` if the value is not a `Boolean`.
    pub fn as_boolean(&self) -> Option<bool> {
        match self {
            Self::Primitive(PrimitiveValue::Boolean(v)) => Some(*v),
            _ => None,
        }
    }

    /// Unwraps the value into a `Boolean`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `Boolean`.
    pub fn unwrap_boolean(self) -> bool {
        match self {
            Self::Primitive(PrimitiveValue::Boolean(v)) => v,
            _ => panic!("value is not a boolean"),
        }
    }

    /// Gets the value as an `Int`.
    ///
    /// Returns `None` if the value is not an `Int`.
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            Self::Primitive(PrimitiveValue::Integer(v)) => Some(*v),
            _ => None,
        }
    }

    /// Unwraps the value into an integer.
    ///
    /// # Panics
    ///
    /// Panics if the value is not an integer.
    pub fn unwrap_integer(self) -> i64 {
        match self {
            Self::Primitive(PrimitiveValue::Integer(v)) => v,
            _ => panic!("value is not an integer"),
        }
    }

    /// Gets the value as a `Float`.
    ///
    /// Returns `None` if the value is not a `Float`.
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Self::Primitive(PrimitiveValue::Float(v)) => Some((*v).into()),
            _ => None,
        }
    }

    /// Unwraps the value into a `Float`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `Float`.
    pub fn unwrap_float(self) -> f64 {
        match self {
            Self::Primitive(PrimitiveValue::Float(v)) => v.into(),
            _ => panic!("value is not a float"),
        }
    }

    /// Gets the value as a `String`.
    ///
    /// Returns `None` if the value is not a `String`.
    pub fn as_string(&self) -> Option<&Arc<String>> {
        match self {
            Self::Primitive(PrimitiveValue::String(s)) => Some(s),
            _ => None,
        }
    }

    /// Unwraps the value into a `String`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `String`.
    pub fn unwrap_string(self) -> Arc<String> {
        match self {
            Self::Primitive(PrimitiveValue::String(s)) => s,
            _ => panic!("value is not a string"),
        }
    }

    /// Gets the value as a `File`.
    ///
    /// Returns `None` if the value is not a `File`.
    pub fn as_file(&self) -> Option<&HostPath> {
        match self {
            Self::Primitive(PrimitiveValue::File(p)) => Some(p),
            _ => None,
        }
    }

    /// Unwraps the value into a `File`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `File`.
    pub fn unwrap_file(self) -> HostPath {
        match self {
            Self::Primitive(PrimitiveValue::File(p)) => p,
            _ => panic!("value is not a file"),
        }
    }

    /// Gets the value as a `Directory`.
    ///
    /// Returns `None` if the value is not a `Directory`.
    pub fn as_directory(&self) -> Option<&HostPath> {
        match self {
            Self::Primitive(PrimitiveValue::Directory(p)) => Some(p),
            _ => None,
        }
    }

    /// Unwraps the value into a `Directory`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `Directory`.
    pub fn unwrap_directory(self) -> HostPath {
        match self {
            Self::Primitive(PrimitiveValue::Directory(p)) => p,
            _ => panic!("value is not a directory"),
        }
    }

    /// Gets the value as a `Pair`.
    ///
    /// Returns `None` if the value is not a `Pair`.
    pub fn as_pair(&self) -> Option<&Pair> {
        match self {
            Self::Compound(CompoundValue::Pair(v)) => Some(v),
            _ => None,
        }
    }

    /// Unwraps the value into a `Pair`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `Pair`.
    pub fn unwrap_pair(self) -> Pair {
        match self {
            Self::Compound(CompoundValue::Pair(v)) => v,
            _ => panic!("value is not a pair"),
        }
    }

    /// Gets the value as an `Array`.
    ///
    /// Returns `None` if the value is not an `Array`.
    pub fn as_array(&self) -> Option<&Array> {
        match self {
            Self::Compound(CompoundValue::Array(v)) => Some(v),
            _ => None,
        }
    }

    /// Unwraps the value into an `Array`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not an `Array`.
    pub fn unwrap_array(self) -> Array {
        match self {
            Self::Compound(CompoundValue::Array(v)) => v,
            _ => panic!("value is not an array"),
        }
    }

    /// Gets the value as a `Map`.
    ///
    /// Returns `None` if the value is not a `Map`.
    pub fn as_map(&self) -> Option<&Map> {
        match self {
            Self::Compound(CompoundValue::Map(v)) => Some(v),
            _ => None,
        }
    }

    /// Unwraps the value into a `Map`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `Map`.
    pub fn unwrap_map(self) -> Map {
        match self {
            Self::Compound(CompoundValue::Map(v)) => v,
            _ => panic!("value is not a map"),
        }
    }

    /// Gets the value as an `Object`.
    ///
    /// Returns `None` if the value is not an `Object`.
    pub fn as_object(&self) -> Option<&Object> {
        match self {
            Self::Compound(CompoundValue::Object(v)) => Some(v),
            _ => None,
        }
    }

    /// Unwraps the value into an `Object`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not an `Object`.
    pub fn unwrap_object(self) -> Object {
        match self {
            Self::Compound(CompoundValue::Object(v)) => v,
            _ => panic!("value is not an object"),
        }
    }

    /// Gets the value as a `Struct`.
    ///
    /// Returns `None` if the value is not a `Struct`.
    pub fn as_struct(&self) -> Option<&Struct> {
        match self {
            Self::Compound(CompoundValue::Struct(v)) => Some(v),
            _ => None,
        }
    }

    /// Unwraps the value into a `Struct`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `Map`.
    pub fn unwrap_struct(self) -> Struct {
        match self {
            Self::Compound(CompoundValue::Struct(v)) => v,
            _ => panic!("value is not a struct"),
        }
    }

    /// Gets the value as a pre-evaluation task.
    ///
    /// Returns `None` if the value is not a pre-evaluation task.
    pub fn as_task_pre_evaluation(&self) -> Option<&TaskPreEvaluationValue> {
        match self {
            Self::Hidden(HiddenValue::TaskPreEvaluation(v)) => Some(v),
            _ => None,
        }
    }

    /// Unwraps the value into a pre-evaluation task.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a pre-evaluation task.
    pub fn unwrap_task_pre_evaluation(self) -> TaskPreEvaluationValue {
        match self {
            Self::Hidden(HiddenValue::TaskPreEvaluation(v)) => v,
            _ => panic!("value is not a pre-evaluation task"),
        }
    }

    /// Gets the value as a post-evaluation task.
    ///
    /// Returns `None` if the value is not a post-evaluation task.
    pub fn as_task_post_evaluation(&self) -> Option<&TaskPostEvaluationValue> {
        match self {
            Self::Hidden(HiddenValue::TaskPostEvaluation(v)) => Some(v),
            _ => None,
        }
    }

    /// Gets a mutable reference to the value as a post-evaluation task.
    ///
    /// Returns `None` if the value is not a post-evaluation task.
    pub(crate) fn as_task_post_evaluation_mut(&mut self) -> Option<&mut TaskPostEvaluationValue> {
        match self {
            Self::Hidden(HiddenValue::TaskPostEvaluation(v)) => Some(v),
            _ => None,
        }
    }

    /// Unwraps the value into a post-evaluation task.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a post-evaluation task.
    pub fn unwrap_task_post_evaluation(self) -> TaskPostEvaluationValue {
        match self {
            Self::Hidden(HiddenValue::TaskPostEvaluation(v)) => v,
            _ => panic!("value is not a post-evaluation task"),
        }
    }

    /// Gets the value as a hints value.
    ///
    /// Returns `None` if the value is not a hints value.
    pub fn as_hints(&self) -> Option<&HintsValue> {
        match self {
            Self::Hidden(HiddenValue::Hints(v)) => Some(v),
            _ => None,
        }
    }

    /// Unwraps the value into a hints value.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a hints value.
    pub fn unwrap_hints(self) -> HintsValue {
        match self {
            Self::Hidden(HiddenValue::Hints(v)) => v,
            _ => panic!("value is not a hints value"),
        }
    }

    /// Gets the value as a call value.
    ///
    /// Returns `None` if the value is not a call value.
    pub fn as_call(&self) -> Option<&CallValue> {
        match self {
            Self::Call(v) => Some(v),
            _ => None,
        }
    }

    /// Unwraps the value into a call value.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a call value.
    pub fn unwrap_call(self) -> CallValue {
        match self {
            Self::Call(v) => v,
            _ => panic!("value is not a call value"),
        }
    }

    /// Visits any paths referenced by this value.
    ///
    /// The callback is invoked for each `File` and `Directory` value referenced
    /// by this value.
    pub(crate) fn visit_paths<F>(&self, cb: &mut F) -> Result<()>
    where
        F: FnMut(bool, &HostPath) -> Result<()> + Send + Sync,
    {
        match self {
            Self::Primitive(PrimitiveValue::File(path)) => cb(true, path),
            Self::Primitive(PrimitiveValue::Directory(path)) => cb(false, path),
            Self::Compound(v) => v.visit_paths(cb),
            _ => Ok(()),
        }
    }

    /// Check that any paths referenced by a `File` or `Directory` value within
    /// this value exist, and return a new value with any relevant host
    /// paths transformed by the given `translate()` function.
    ///
    /// If a `File` or `Directory` value is optional and the path does not
    /// exist, it is replaced with a WDL `None` value.
    ///
    /// If a `File` or `Directory` value is required and the path does not
    /// exist, an error is returned.
    ///
    /// If a local base directory is provided, it will be joined with any
    /// relative local paths prior to checking for existence.
    ///
    /// The provided transferer is used for checking remote URL existence.
    ///
    /// TODO ACF 2025-11-10: this function is an intermediate step on the way to
    /// more thoroughly refactoring the code between `sprocket` and
    /// `wdl_engine`. Expect this interface to change soon!
    pub(crate) async fn resolve_paths<F>(
        &self,
        optional: bool,
        base_dir: Option<&Path>,
        transferer: Option<&dyn Transferer>,
        translate: &F,
    ) -> Result<Self>
    where
        F: Fn(&HostPath) -> Result<HostPath> + Send + Sync,
    {
        match self {
            Self::Primitive(v @ PrimitiveValue::File(path))
            | Self::Primitive(v @ PrimitiveValue::Directory(path)) => {
                // We treat file and directory paths almost entirely the same, other than when
                // reporting errors and choosing which variant to return in the result
                let is_file = v.as_file().is_some();
                let path = translate(path)?;

                if path::is_file_url(path.as_str()) {
                    // File URLs must be absolute paths, so we just check whether it exists without
                    // performing any joining
                    let exists = path::parse_supported_url(path.as_str())
                        .and_then(|url| url.to_file_path().ok())
                        .map(|p| p.exists())
                        .unwrap_or(false);
                    if exists {
                        let v = PrimitiveValue::new_file_or_directory(is_file, path);
                        return Ok(Self::Primitive(v));
                    }

                    if optional && !exists {
                        return Ok(Value::new_none(self.ty().optional()));
                    }

                    bail!("path `{path}` does not exist");
                } else if path::is_supported_url(path.as_str()) {
                    match transferer {
                        Some(transferer) => {
                            let exists = transferer
                                .exists(
                                    &path
                                        .as_str()
                                        .parse()
                                        .with_context(|| format!("invalid URL `{path}`"))?,
                                )
                                .await?;
                            if exists {
                                let v = PrimitiveValue::new_file_or_directory(is_file, path);
                                return Ok(Self::Primitive(v));
                            }

                            if optional && !exists {
                                return Ok(Value::new_none(self.ty().optional()));
                            }

                            bail!("URL `{path}` does not exist");
                        }
                        None => {
                            // Assume the URL exists
                            let v = PrimitiveValue::new_file_or_directory(is_file, path);
                            return Ok(Self::Primitive(v));
                        }
                    }
                }

                // Check for existence
                let exists_path: Cow<'_, Path> = base_dir
                    .map(|d| d.join(path.as_str()).into())
                    .unwrap_or_else(|| Path::new(path.as_str()).into());
                if is_file && !exists_path.is_file() {
                    if optional {
                        return Ok(Value::new_none(self.ty().optional()));
                    } else {
                        bail!("file `{}` does not exist", exists_path.display());
                    }
                } else if !is_file && !exists_path.is_dir() {
                    if optional {
                        return Ok(Value::new_none(self.ty().optional()));
                    } else {
                        bail!("directory `{}` does not exist", exists_path.display())
                    }
                }

                let v = PrimitiveValue::new_file_or_directory(is_file, path);
                Ok(Self::Primitive(v))
            }
            Self::Compound(v) => Ok(Self::Compound(
                v.resolve_paths(base_dir, transferer, translate)
                    .boxed()
                    .await?,
            )),
            v => Ok(v.clone()),
        }
    }

    /// Determines if two values have equality according to the WDL
    /// specification.
    ///
    /// Returns `None` if the two values cannot be compared for equality.
    pub fn equals(left: &Self, right: &Self) -> Option<bool> {
        match (left, right) {
            (Value::None(_), Value::None(_)) => Some(true),
            (Value::None(_), _) | (_, Value::None(_)) => Some(false),
            (Value::Primitive(left), Value::Primitive(right)) => {
                Some(PrimitiveValue::compare(left, right)? == Ordering::Equal)
            }
            (Value::Compound(left), Value::Compound(right)) => CompoundValue::equals(left, right),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None(_) => write!(f, "None"),
            Self::Primitive(v) => v.fmt(f),
            Self::Compound(v) => v.fmt(f),
            Self::Hidden(v) => v.fmt(f),
            Self::Call(c) => c.fmt(f),
            Self::TypeNameRef(ty) => ty.fmt(f),
        }
    }
}

impl Coercible for Value {
    fn coerce(&self, context: Option<&dyn EvaluationContext>, target: &Type) -> Result<Self> {
        if target.is_union() || target.is_none() || self.ty().eq(target) {
            return Ok(self.clone());
        }

        match self {
            Self::None(_) => {
                if target.is_optional() {
                    Ok(Self::new_none(target.clone()))
                } else {
                    bail!("cannot coerce `None` to non-optional type `{target}`");
                }
            }
            // String -> Enum Variant
            Self::Primitive(PrimitiveValue::String(s)) if target.as_enum().is_some() => {
                // SAFETY: we just checked above that this is an enum type.
                let enum_ty = target.as_enum().unwrap();

                if enum_ty
                    .variants()
                    .iter()
                    .any(|variant_name| variant_name == s.as_str())
                {
                    if let Some(context) = context {
                        if let Ok(value) = context.enum_variant_value(enum_ty.name(), s) {
                            return Ok(Value::Compound(CompoundValue::EnumVariant(
                                EnumVariant::new(enum_ty.clone(), s.as_str(), value),
                            )));
                        } else {
                            bail!(
                                "enum variant value lookup failed for variant `{s}` in enum `{}`",
                                enum_ty.name()
                            );
                        }
                    } else {
                        bail!(
                            "context does not exist when creating enum variant value `{s}` in \
                             enum `{}`",
                            enum_ty.name()
                        );
                    }
                }

                let variants = if enum_ty.variants().is_empty() {
                    None
                } else {
                    let mut variant_names = enum_ty.variants().to_vec();
                    variant_names.sort();
                    Some(format!(" (variants: `{}`)", variant_names.join("`, `")))
                }
                .unwrap_or_default();

                bail!(
                    "cannot coerce type `String` to type `{target}`: variant `{s}` not found in \
                     enum `{}`{variants}",
                    enum_ty.name()
                );
            }
            // Enum Variant -> String
            Self::Compound(CompoundValue::EnumVariant(e))
                if target
                    .as_primitive()
                    .map(|t| matches!(t, PrimitiveType::String))
                    .unwrap_or(false) =>
            {
                Ok(Value::Primitive(PrimitiveValue::new_string(e.name())))
            }
            Self::Primitive(v) => v.coerce(context, target).map(Self::Primitive),
            Self::Compound(v) => v.coerce(context, target).map(Self::Compound),
            Self::Hidden(v) => v.coerce(context, target).map(Self::Hidden),
            Self::Call(_) => {
                bail!("call values cannot be coerced to any other type");
            }
            Self::TypeNameRef(_) => {
                bail!("type name references cannot be coerced to any other type");
            }
        }
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self::Primitive(value.into())
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Self::Primitive(value.into())
    }
}

impl TryFrom<u64> for Value {
    type Error = std::num::TryFromIntError;

    fn try_from(value: u64) -> std::result::Result<Self, Self::Error> {
        let value: i64 = value.try_into()?;
        Ok(value.into())
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Self::Primitive(value.into())
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Self::Primitive(value.into())
    }
}

impl From<PrimitiveValue> for Value {
    fn from(value: PrimitiveValue) -> Self {
        Self::Primitive(value)
    }
}

impl From<Option<PrimitiveValue>> for Value {
    fn from(value: Option<PrimitiveValue>) -> Self {
        match value {
            Some(v) => v.into(),
            None => Self::new_none(Type::None),
        }
    }
}

impl From<CompoundValue> for Value {
    fn from(value: CompoundValue) -> Self {
        Self::Compound(value)
    }
}

impl From<HiddenValue> for Value {
    fn from(value: HiddenValue) -> Self {
        Self::Hidden(value)
    }
}

impl From<Pair> for Value {
    fn from(value: Pair) -> Self {
        Self::Compound(value.into())
    }
}

impl From<Array> for Value {
    fn from(value: Array) -> Self {
        Self::Compound(value.into())
    }
}

impl From<Map> for Value {
    fn from(value: Map) -> Self {
        Self::Compound(value.into())
    }
}

impl From<Object> for Value {
    fn from(value: Object) -> Self {
        Self::Compound(value.into())
    }
}

impl From<Struct> for Value {
    fn from(value: Struct) -> Self {
        Self::Compound(value.into())
    }
}

impl From<CallValue> for Value {
    fn from(value: CallValue) -> Self {
        Self::Call(value)
    }
}

impl<'de> serde::Deserialize<'de> for Value {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::Deserialize as _;

        /// Visitor for deserialization.
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Value;

            fn visit_unit<E>(self) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Value::new_none(Type::None))
            }

            fn visit_none<E>(self) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Value::new_none(Type::None))
            }

            fn visit_some<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                Value::deserialize(deserializer)
            }

            fn visit_bool<E>(self, v: bool) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Value::Primitive(PrimitiveValue::Boolean(v)))
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Value::Primitive(PrimitiveValue::Integer(v)))
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Value::Primitive(PrimitiveValue::Integer(
                    v.try_into().map_err(|_| {
                        E::custom("integer not in range for a 64-bit signed integer")
                    })?,
                )))
            }

            fn visit_f64<E>(self, v: f64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Value::Primitive(PrimitiveValue::Float(v.into())))
            }

            fn visit_str<E>(self, v: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Value::Primitive(PrimitiveValue::new_string(v)))
            }

            fn visit_string<E>(self, v: String) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Value::Primitive(PrimitiveValue::new_string(v)))
            }

            fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                use serde::de::Error as _;

                let mut elements = vec![];
                while let Some(element) = seq.next_element::<Value>()? {
                    elements.push(element);
                }

                // Try to find a mutually-agreeable common type for the elements of the array.
                let mut candidate_ty = None;
                for element in elements.iter() {
                    let new_candidate_ty = element.ty();
                    let old_candidate_ty =
                        candidate_ty.get_or_insert_with(|| new_candidate_ty.clone());
                    let Some(new_common_ty) = old_candidate_ty.common_type(&new_candidate_ty)
                    else {
                        return Err(A::Error::custom(format!(
                            "a common element type does not exist between `{old_candidate_ty}` \
                             and `{new_candidate_ty}`"
                        )));
                    };
                    candidate_ty = Some(new_common_ty);
                }
                // An empty array's elements have the `Union` type.
                let array_ty: Type = ArrayType::new(candidate_ty.unwrap_or(Type::Union)).into();
                Ok(Array::new(None, array_ty.clone(), elements)
                    .map_err(|e| {
                        A::Error::custom(format!("cannot coerce value to `{array_ty}`: {e:#}"))
                    })?
                    .into())
            }

            fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut members = IndexMap::new();
                while let Some(key) = map.next_key::<String>()? {
                    members.insert(key, map.next_value()?);
                }

                Ok(Value::Compound(CompoundValue::Object(Object::new(members))))
            }

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "a WDL value")
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

/// Represents a primitive WDL value.
///
/// Primitive values are cheap to clone.
#[derive(Debug, Clone)]
pub enum PrimitiveValue {
    /// The value is a `Boolean`.
    Boolean(bool),
    /// The value is an `Int`.
    Integer(i64),
    /// The value is a `Float`.
    Float(OrderedFloat<f64>),
    /// The value is a `String`.
    String(Arc<String>),
    /// The value is a `File`.
    File(HostPath),
    /// The value is a `Directory`.
    Directory(HostPath),
}

impl PrimitiveValue {
    /// Creates a new `String` value.
    pub fn new_string(s: impl Into<String>) -> Self {
        Self::String(Arc::new(s.into()))
    }

    /// Creates a new `File` value.
    pub fn new_file(path: impl Into<HostPath>) -> Self {
        Self::File(path.into())
    }

    /// Creates a new `Directory` value.
    pub fn new_directory(path: impl Into<HostPath>) -> Self {
        Self::Directory(path.into())
    }

    /// Create either a new `File` or `Directory` value, depending on whether
    /// the `is_file` argument is `true`.
    ///
    /// This is a bit awkward, but can save a lot of repetition in code that
    /// treats files and directories largely the same until having to
    /// remember which enum variant the path needs to be stuffed back into.
    fn new_file_or_directory(is_file: bool, path: impl Into<HostPath>) -> Self {
        if is_file {
            Self::File(path.into())
        } else {
            Self::Directory(path.into())
        }
    }

    /// Gets the type of the value.
    pub fn ty(&self) -> Type {
        match self {
            Self::Boolean(_) => PrimitiveType::Boolean.into(),
            Self::Integer(_) => PrimitiveType::Integer.into(),
            Self::Float(_) => PrimitiveType::Float.into(),
            Self::String(_) => PrimitiveType::String.into(),
            Self::File(_) => PrimitiveType::File.into(),
            Self::Directory(_) => PrimitiveType::Directory.into(),
        }
    }

    /// Gets the value as a `Boolean`.
    ///
    /// Returns `None` if the value is not a `Boolean`.
    pub fn as_boolean(&self) -> Option<bool> {
        match self {
            Self::Boolean(v) => Some(*v),
            _ => None,
        }
    }

    /// Unwraps the value into a `Boolean`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `Boolean`.
    pub fn unwrap_boolean(self) -> bool {
        match self {
            Self::Boolean(v) => v,
            _ => panic!("value is not a boolean"),
        }
    }

    /// Gets the value as an `Int`.
    ///
    /// Returns `None` if the value is not an `Int`.
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            Self::Integer(v) => Some(*v),
            _ => None,
        }
    }

    /// Unwraps the value into an integer.
    ///
    /// # Panics
    ///
    /// Panics if the value is not an integer.
    pub fn unwrap_integer(self) -> i64 {
        match self {
            Self::Integer(v) => v,
            _ => panic!("value is not an integer"),
        }
    }

    /// Gets the value as a `Float`.
    ///
    /// Returns `None` if the value is not a `Float`.
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Self::Float(v) => Some((*v).into()),
            _ => None,
        }
    }

    /// Unwraps the value into a `Float`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `Float`.
    pub fn unwrap_float(self) -> f64 {
        match self {
            Self::Float(v) => v.into(),
            _ => panic!("value is not a float"),
        }
    }

    /// Gets the value as a `String`.
    ///
    /// Returns `None` if the value is not a `String`.
    pub fn as_string(&self) -> Option<&Arc<String>> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    /// Unwraps the value into a `String`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `String`.
    pub fn unwrap_string(self) -> Arc<String> {
        match self {
            Self::String(s) => s,
            _ => panic!("value is not a string"),
        }
    }

    /// Gets the value as a `File`.
    ///
    /// Returns `None` if the value is not a `File`.
    pub fn as_file(&self) -> Option<&HostPath> {
        match self {
            Self::File(p) => Some(p),
            _ => None,
        }
    }

    /// Unwraps the value into a `File`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `File`.
    pub fn unwrap_file(self) -> HostPath {
        match self {
            Self::File(p) => p,
            _ => panic!("value is not a file"),
        }
    }

    /// Gets the value as a `Directory`.
    ///
    /// Returns `None` if the value is not a `Directory`.
    pub fn as_directory(&self) -> Option<&HostPath> {
        match self {
            Self::Directory(p) => Some(p),
            _ => None,
        }
    }

    /// Unwraps the value into a `Directory`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `Directory`.
    pub fn unwrap_directory(self) -> HostPath {
        match self {
            Self::Directory(p) => p,
            _ => panic!("value is not a directory"),
        }
    }

    /// Compares two values for an ordering according to the WDL specification.
    ///
    /// Unlike a `PartialOrd` implementation, this takes into account automatic
    /// coercions.
    ///
    /// Returns `None` if the values cannot be compared based on their types.
    pub fn compare(left: &Self, right: &Self) -> Option<Ordering> {
        match (left, right) {
            (Self::Boolean(left), Self::Boolean(right)) => Some(left.cmp(right)),
            (Self::Integer(left), Self::Integer(right)) => Some(left.cmp(right)),
            (Self::Integer(left), Self::Float(right)) => {
                Some(OrderedFloat(*left as f64).cmp(right))
            }
            (Self::Float(left), Self::Integer(right)) => {
                Some(left.cmp(&OrderedFloat(*right as f64)))
            }
            (Self::Float(left), Self::Float(right)) => Some(left.cmp(right)),
            (Self::String(left), Self::String(right))
            | (Self::String(left), Self::File(HostPath(right)))
            | (Self::String(left), Self::Directory(HostPath(right)))
            | (Self::File(HostPath(left)), Self::File(HostPath(right)))
            | (Self::File(HostPath(left)), Self::String(right))
            | (Self::Directory(HostPath(left)), Self::Directory(HostPath(right)))
            | (Self::Directory(HostPath(left)), Self::String(right)) => Some(left.cmp(right)),
            _ => None,
        }
    }

    /// Gets a raw display of the value.
    ///
    /// This differs from the [Display][fmt::Display] implementation in that
    /// strings, files, and directories are not quoted and not escaped.
    ///
    /// The provided coercion context is used to translate host paths to guest
    /// paths; if `None`, `File` and `Directory` values are displayed as-is.
    pub fn raw<'a>(
        &'a self,
        context: Option<&'a dyn EvaluationContext>,
    ) -> impl fmt::Display + use<'a> {
        /// Helper for displaying a raw value.
        struct Display<'a> {
            /// The value to display.
            value: &'a PrimitiveValue,
            /// The coercion context.
            context: Option<&'a dyn EvaluationContext>,
        }

        impl fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self.value {
                    PrimitiveValue::Boolean(v) => write!(f, "{v}"),
                    PrimitiveValue::Integer(v) => write!(f, "{v}"),
                    PrimitiveValue::Float(v) => write!(f, "{v:.6?}"),
                    PrimitiveValue::String(v) => write!(f, "{v}"),
                    PrimitiveValue::File(v) => {
                        write!(
                            f,
                            "{v}",
                            v = self
                                .context
                                .and_then(|c| c.guest_path(v).map(|p| Cow::Owned(p.0)))
                                .unwrap_or(Cow::Borrowed(&v.0))
                        )
                    }
                    PrimitiveValue::Directory(v) => {
                        write!(
                            f,
                            "{v}",
                            v = self
                                .context
                                .and_then(|c| c.guest_path(v).map(|p| Cow::Owned(p.0)))
                                .unwrap_or(Cow::Borrowed(&v.0))
                        )
                    }
                }
            }
        }

        Display {
            value: self,
            context,
        }
    }
}

impl fmt::Display for PrimitiveValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Boolean(v) => write!(f, "{v}"),
            Self::Integer(v) => write!(f, "{v}"),
            Self::Float(v) => write!(f, "{v:.6?}"),
            Self::String(s) | Self::File(HostPath(s)) | Self::Directory(HostPath(s)) => {
                // TODO: handle necessary escape sequences
                write!(f, "\"{s}\"")
            }
        }
    }
}

impl PartialEq for PrimitiveValue {
    fn eq(&self, other: &Self) -> bool {
        Self::compare(self, other) == Some(Ordering::Equal)
    }
}

impl Eq for PrimitiveValue {}

impl Hash for PrimitiveValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::Boolean(v) => {
                0.hash(state);
                v.hash(state);
            }
            Self::Integer(v) => {
                1.hash(state);
                v.hash(state);
            }
            Self::Float(v) => {
                // Hash this with the same discriminant as integer; this allows coercion from
                // int to float.
                1.hash(state);
                v.hash(state);
            }
            Self::String(v) | Self::File(HostPath(v)) | Self::Directory(HostPath(v)) => {
                // Hash these with the same discriminant; this allows coercion from file and
                // directory to string
                2.hash(state);
                v.hash(state);
            }
        }
    }
}

impl From<bool> for PrimitiveValue {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}

impl From<i64> for PrimitiveValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl From<f64> for PrimitiveValue {
    fn from(value: f64) -> Self {
        Self::Float(value.into())
    }
}

impl From<String> for PrimitiveValue {
    fn from(value: String) -> Self {
        Self::String(value.into())
    }
}

impl Coercible for PrimitiveValue {
    fn coerce(&self, context: Option<&dyn EvaluationContext>, target: &Type) -> Result<Self> {
        if target.is_union() || target.is_none() || self.ty().eq(target) {
            return Ok(self.clone());
        }

        match self {
            Self::Boolean(v) => {
                target
                    .as_primitive()
                    .and_then(|ty| match ty {
                        // Boolean -> Boolean
                        PrimitiveType::Boolean => Some(Self::Boolean(*v)),
                        _ => None,
                    })
                    .with_context(|| format!("cannot coerce type `Boolean` to type `{target}`"))
            }
            Self::Integer(v) => {
                target
                    .as_primitive()
                    .and_then(|ty| match ty {
                        // Int -> Int
                        PrimitiveType::Integer => Some(Self::Integer(*v)),
                        // Int -> Float
                        PrimitiveType::Float => Some(Self::Float((*v as f64).into())),
                        _ => None,
                    })
                    .with_context(|| format!("cannot coerce type `Int` to type `{target}`"))
            }
            Self::Float(v) => {
                target
                    .as_primitive()
                    .and_then(|ty| match ty {
                        // Float -> Float
                        PrimitiveType::Float => Some(Self::Float(*v)),
                        _ => None,
                    })
                    .with_context(|| format!("cannot coerce type `Float` to type `{target}`"))
            }
            Self::String(s) => {
                target
                    .as_primitive()
                    .and_then(|ty| match ty {
                        // String -> String
                        PrimitiveType::String => Some(Self::String(s.clone())),
                        // String -> File
                        PrimitiveType::File => Some(Self::File(
                            context
                                .and_then(|c| c.host_path(&GuestPath(s.clone())))
                                .unwrap_or_else(|| s.clone().into()),
                        )),
                        // String -> Directory
                        PrimitiveType::Directory => Some(Self::Directory(
                            context
                                .and_then(|c| c.host_path(&GuestPath(s.clone())))
                                .unwrap_or_else(|| s.clone().into()),
                        )),
                        _ => None,
                    })
                    .with_context(|| format!("cannot coerce type `String` to type `{target}`"))
            }
            Self::File(p) => {
                target
                    .as_primitive()
                    .and_then(|ty| match ty {
                        // File -> File
                        PrimitiveType::File => Some(Self::File(p.clone())),
                        // File -> String
                        PrimitiveType::String => Some(Self::String(
                            context
                                .and_then(|c| c.guest_path(p).map(Into::into))
                                .unwrap_or_else(|| p.clone().into()),
                        )),
                        _ => None,
                    })
                    .with_context(|| format!("cannot coerce type `File` to type `{target}`"))
            }
            Self::Directory(p) => {
                target
                    .as_primitive()
                    .and_then(|ty| match ty {
                        // Directory -> Directory
                        PrimitiveType::Directory => Some(Self::Directory(p.clone())),
                        // Directory -> String
                        PrimitiveType::String => Some(Self::String(
                            context
                                .and_then(|c| c.guest_path(p).map(Into::into))
                                .unwrap_or_else(|| p.clone().into()),
                        )),
                        _ => None,
                    })
                    .with_context(|| format!("cannot coerce type `Directory` to type `{target}`"))
            }
        }
    }
}

impl serde::Serialize for PrimitiveValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Boolean(v) => v.serialize(serializer),
            Self::Integer(v) => v.serialize(serializer),
            Self::Float(v) => v.serialize(serializer),
            Self::String(s) | Self::File(HostPath(s)) | Self::Directory(HostPath(s)) => {
                s.serialize(serializer)
            }
        }
    }
}

/// Represents a `Pair` value.
///
/// Pairs are cheap to clone.
#[derive(Debug, Clone)]
pub struct Pair {
    /// The type of the pair.
    ty: Type,
    /// The left and right values of the pair.
    values: Arc<(Value, Value)>,
}

impl Pair {
    /// Creates a new `Pair` value.
    ///
    /// Returns an error if either the `left` value or the `right` value did not
    /// coerce to the pair's `left` type or `right` type, respectively.
    ///
    /// # Panics
    ///
    /// Panics if the given type is not a pair type.
    pub fn new(
        context: Option<&dyn EvaluationContext>,
        ty: impl Into<Type>,
        left: impl Into<Value>,
        right: impl Into<Value>,
    ) -> Result<Self> {
        let ty = ty.into();
        if let Type::Compound(CompoundType::Pair(ty), _) = ty {
            let left = left
                .into()
                .coerce(context, ty.left_type())
                .context("failed to coerce pair's left value")?;
            let right = right
                .into()
                .coerce(context, ty.right_type())
                .context("failed to coerce pair's right value")?;
            return Ok(Self::new_unchecked(
                Type::Compound(CompoundType::Pair(ty), false),
                left,
                right,
            ));
        }

        panic!("type `{ty}` is not a pair type");
    }

    /// Constructs a new pair without checking the given left and right conform
    /// to the given type.
    pub(crate) fn new_unchecked(ty: Type, left: Value, right: Value) -> Self {
        assert!(ty.as_pair().is_some());
        Self {
            ty: ty.require(),
            values: Arc::new((left, right)),
        }
    }

    /// Gets the type of the `Pair`.
    pub fn ty(&self) -> Type {
        self.ty.clone()
    }

    /// Gets the left value of the `Pair`.
    pub fn left(&self) -> &Value {
        &self.values.0
    }

    /// Gets the right value of the `Pair`.
    pub fn right(&self) -> &Value {
        &self.values.1
    }
}

impl fmt::Display for Pair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "({left}, {right})",
            left = self.values.0,
            right = self.values.1
        )
    }
}

/// Represents an `Array` value.
///
/// Arrays are cheap to clone.
#[derive(Debug, Clone)]
pub struct Array {
    /// The type of the array.
    ty: Type,
    /// The array's elements.
    ///
    /// A value of `None` indicates an empty array.
    elements: Option<Arc<Vec<Value>>>,
}

impl Array {
    /// Creates a new `Array` value for the given array type.
    ///
    /// Returns an error if an element did not coerce to the array's element
    /// type.
    ///
    /// # Panics
    ///
    /// Panics if the given type is not an array type.
    pub fn new<V>(
        context: Option<&dyn EvaluationContext>,
        ty: impl Into<Type>,
        elements: impl IntoIterator<Item = V>,
    ) -> Result<Self>
    where
        V: Into<Value>,
    {
        let ty = ty.into();
        if let Type::Compound(CompoundType::Array(ty), _) = ty {
            let element_type = ty.element_type();
            let elements = elements
                .into_iter()
                .enumerate()
                .map(|(i, v)| {
                    let v = v.into();
                    v.coerce(context, element_type)
                        .with_context(|| format!("failed to coerce array element at index {i}"))
                })
                .collect::<Result<Vec<_>>>()?;

            return Ok(Self::new_unchecked(
                Type::Compound(CompoundType::Array(ty.unqualified()), false),
                elements,
            ));
        }

        panic!("type `{ty}` is not an array type");
    }

    /// Constructs a new array without checking the given elements conform to
    /// the given type.
    pub(crate) fn new_unchecked(ty: Type, elements: Vec<Value>) -> Self {
        let ty = if let Type::Compound(CompoundType::Array(ty), _) = ty {
            Type::Compound(CompoundType::Array(ty.unqualified()), false)
        } else {
            panic!("type is not an array type");
        };

        Self {
            ty,
            elements: if elements.is_empty() {
                None
            } else {
                Some(Arc::new(elements))
            },
        }
    }

    /// Gets the type of the `Array` value.
    pub fn ty(&self) -> Type {
        self.ty.clone()
    }

    /// Converts the array value to a slice of values.
    pub fn as_slice(&self) -> &[Value] {
        self.elements.as_ref().map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Returns the number of elements in the array.
    pub fn len(&self) -> usize {
        self.elements.as_ref().map(|v| v.len()).unwrap_or(0)
    }

    /// Returns `true` if the array has no elements.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl fmt::Display for Array {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[")?;

        if let Some(elements) = &self.elements {
            for (i, element) in elements.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }

                write!(f, "{element}")?;
            }
        }

        write!(f, "]")
    }
}

/// Represents a `Map` value.
///
/// Maps are cheap to clone.
#[derive(Debug, Clone)]
pub struct Map {
    /// The type of the map value.
    ty: Type,
    /// The elements of the map value.
    ///
    /// A value of `None` indicates an empty map.
    elements: Option<Arc<IndexMap<Option<PrimitiveValue>, Value>>>,
}

impl Map {
    /// Creates a new `Map` value.
    ///
    /// Returns an error if a key or value did not coerce to the map's key or
    /// value type, respectively.
    ///
    /// # Panics
    ///
    /// Panics if the given type is not a map type.
    pub fn new<K, V>(
        context: Option<&dyn EvaluationContext>,
        ty: impl Into<Type>,
        elements: impl IntoIterator<Item = (K, V)>,
    ) -> Result<Self>
    where
        K: Into<Value>,
        V: Into<Value>,
    {
        let ty = ty.into();
        if let Type::Compound(CompoundType::Map(ty), _) = ty {
            let key_type = ty.key_type();
            let value_type = ty.value_type();

            let elements = elements
                .into_iter()
                .enumerate()
                .map(|(i, (k, v))| {
                    let k = k.into();
                    let v = v.into();
                    Ok((
                        if k.is_none() {
                            None
                        } else {
                            match k.coerce(context, key_type).with_context(|| {
                                format!("failed to coerce map key for element at index {i}")
                            })? {
                                Value::None(_) => None,
                                Value::Primitive(v) => Some(v),
                                _ => {
                                    bail!("not all key values are primitive")
                                }
                            }
                        },
                        v.coerce(context, value_type).with_context(|| {
                            format!("failed to coerce map value for element at index {i}")
                        })?,
                    ))
                })
                .collect::<Result<_>>()?;

            return Ok(Self::new_unchecked(
                Type::Compound(CompoundType::Map(ty), false),
                elements,
            ));
        }

        panic!("type `{ty}` is not a map type");
    }

    /// Constructs a new map without checking the given elements conform to the
    /// given type.
    pub(crate) fn new_unchecked(
        ty: Type,
        elements: IndexMap<Option<PrimitiveValue>, Value>,
    ) -> Self {
        assert!(ty.as_map().is_some());
        Self {
            ty: ty.require(),
            elements: if elements.is_empty() {
                None
            } else {
                Some(Arc::new(elements))
            },
        }
    }

    /// Gets the type of the `Map` value.
    pub fn ty(&self) -> Type {
        self.ty.clone()
    }

    /// Iterates the elements of the map.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&Option<PrimitiveValue>, &Value)> {
        self.elements
            .as_ref()
            .map(|m| Either::Left(m.iter()))
            .unwrap_or(Either::Right(std::iter::empty()))
    }

    /// Iterates the keys of the map.
    pub fn keys(&self) -> impl ExactSizeIterator<Item = &Option<PrimitiveValue>> {
        self.elements
            .as_ref()
            .map(|m| Either::Left(m.keys()))
            .unwrap_or(Either::Right(std::iter::empty()))
    }

    /// Iterates the values of the map.
    pub fn values(&self) -> impl ExactSizeIterator<Item = &Value> {
        self.elements
            .as_ref()
            .map(|m| Either::Left(m.values()))
            .unwrap_or(Either::Right(std::iter::empty()))
    }

    /// Determines if the map contains the given key.
    pub fn contains_key(&self, key: &Option<PrimitiveValue>) -> bool {
        self.elements
            .as_ref()
            .map(|m| m.contains_key(key))
            .unwrap_or(false)
    }

    /// Gets a value from the map by key.
    pub fn get(&self, key: &Option<PrimitiveValue>) -> Option<&Value> {
        self.elements.as_ref().and_then(|m| m.get(key))
    }

    /// Returns the number of elements in the map.
    pub fn len(&self) -> usize {
        self.elements.as_ref().map(|m| m.len()).unwrap_or(0)
    }

    /// Returns `true` if the map has no elements.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl fmt::Display for Map {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{")?;

        for (i, (k, v)) in self.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }

            match k {
                Some(k) => write!(f, "{k}: {v}")?,
                None => write!(f, "None: {v}")?,
            }
        }

        write!(f, "}}")
    }
}

/// Represents an `Object` value.
///
/// Objects are cheap to clone.
#[derive(Debug, Clone)]
pub struct Object {
    /// The members of the object.
    ///
    /// A value of `None` indicates an empty object.
    pub(crate) members: Option<Arc<IndexMap<String, Value>>>,
}

impl Object {
    /// Creates a new `Object` value.
    ///
    /// Keys **must** be known WDL identifiers checked by the caller.
    pub(crate) fn new(members: IndexMap<String, Value>) -> Self {
        Self {
            members: if members.is_empty() {
                None
            } else {
                Some(Arc::new(members))
            },
        }
    }

    /// Returns an empty object.
    pub fn empty() -> Self {
        Self::new(IndexMap::default())
    }

    /// Creates an object from an iterator of V1 AST metadata items.
    pub fn from_v1_metadata<N: TreeNode>(
        items: impl Iterator<Item = v1::MetadataObjectItem<N>>,
    ) -> Self {
        Object::new(
            items
                .map(|i| {
                    (
                        i.name().text().to_string(),
                        Value::from_v1_metadata(&i.value()),
                    )
                })
                .collect::<IndexMap<_, _>>(),
        )
    }

    /// Gets the type of the `Object` value.
    pub fn ty(&self) -> Type {
        Type::Object
    }

    /// Iterates the members of the object.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &Value)> {
        self.members
            .as_ref()
            .map(|m| Either::Left(m.iter().map(|(k, v)| (k.as_str(), v))))
            .unwrap_or(Either::Right(std::iter::empty()))
    }

    /// Iterates the keys of the object.
    pub fn keys(&self) -> impl ExactSizeIterator<Item = &str> {
        self.members
            .as_ref()
            .map(|m| Either::Left(m.keys().map(|k| k.as_str())))
            .unwrap_or(Either::Right(std::iter::empty()))
    }

    /// Iterates the values of the object.
    pub fn values(&self) -> impl ExactSizeIterator<Item = &Value> {
        self.members
            .as_ref()
            .map(|m| Either::Left(m.values()))
            .unwrap_or(Either::Right(std::iter::empty()))
    }

    /// Determines if the object contains the given key.
    pub fn contains_key(&self, key: &str) -> bool {
        self.members
            .as_ref()
            .map(|m| m.contains_key(key))
            .unwrap_or(false)
    }

    /// Gets a value from the object by key.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.members.as_ref().and_then(|m| m.get(key))
    }

    /// Returns the number of members in the object.
    pub fn len(&self) -> usize {
        self.members.as_ref().map(|m| m.len()).unwrap_or(0)
    }

    /// Returns `true` if the object has no members.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl fmt::Display for Object {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "object {{")?;

        for (i, (k, v)) in self.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }

            write!(f, "{k}: {v}")?;
        }

        write!(f, "}}")
    }
}

/// Represents a `Struct` value.
///
/// Structs are cheap to clone.
#[derive(Debug, Clone)]
pub struct Struct {
    /// The type of the struct value.
    ty: Type,
    /// The name of the struct.
    name: Arc<String>,
    /// The members of the struct value.
    pub(crate) members: Arc<IndexMap<String, Value>>,
}

impl Struct {
    /// Creates a new struct value.
    ///
    /// Returns an error if the struct type does not contain a member of a given
    /// name or if a value does not coerce to the corresponding member's type.
    ///
    /// # Panics
    ///
    /// Panics if the given type is not a struct type.
    pub fn new<S, V>(
        context: Option<&dyn EvaluationContext>,
        ty: impl Into<Type>,
        members: impl IntoIterator<Item = (S, V)>,
    ) -> Result<Self>
    where
        S: Into<String>,
        V: Into<Value>,
    {
        let ty = ty.into();
        if let Type::Compound(CompoundType::Custom(CustomType::Struct(ty)), optional) = ty {
            let mut members = members
                .into_iter()
                .map(|(n, v)| {
                    let n = n.into();
                    let v = v.into();
                    let v = v
                        .coerce(
                            context,
                            ty.members().get(&n).ok_or_else(|| {
                                anyhow!("struct does not contain a member named `{n}`")
                            })?,
                        )
                        .with_context(|| format!("failed to coerce struct member `{n}`"))?;
                    Ok((n, v))
                })
                .collect::<Result<IndexMap<_, _>>>()?;

            for (name, ty) in ty.members().iter() {
                // Check for optional members that should be set to `None`
                if ty.is_optional() {
                    if !members.contains_key(name) {
                        members.insert(name.clone(), Value::new_none(ty.clone()));
                    }
                } else {
                    // Check for a missing required member
                    if !members.contains_key(name) {
                        bail!("missing a value for struct member `{name}`");
                    }
                }
            }

            let name = ty.name().to_string();
            return Ok(Self {
                ty: Type::Compound(CompoundType::Custom(CustomType::Struct(ty)), optional),
                name: Arc::new(name),
                members: Arc::new(members),
            });
        }

        panic!("type `{ty}` is not a struct type");
    }

    /// Constructs a new struct without checking the given members conform to
    /// the given type.
    pub(crate) fn new_unchecked(
        ty: Type,
        name: Arc<String>,
        members: Arc<IndexMap<String, Value>>,
    ) -> Self {
        assert!(ty.as_struct().is_some());
        Self {
            ty: ty.require(),
            name,
            members,
        }
    }

    /// Gets the type of the `Struct` value.
    pub fn ty(&self) -> Type {
        self.ty.clone()
    }

    /// Gets the name of the struct.
    pub fn name(&self) -> &Arc<String> {
        &self.name
    }

    /// Iterates the members of the struct.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &Value)> {
        self.members.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Iterates the keys of the struct.
    pub fn keys(&self) -> impl ExactSizeIterator<Item = &str> {
        self.members.keys().map(|k| k.as_str())
    }

    /// Iterates the values of the struct.
    pub fn values(&self) -> impl ExactSizeIterator<Item = &Value> {
        self.members.values()
    }

    /// Determines if the struct contains the given member name.
    pub fn contains_key(&self, key: &str) -> bool {
        self.members.contains_key(key)
    }

    /// Gets a value from the struct by member name.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.members.get(key)
    }
}

impl fmt::Display for Struct {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{name} {{", name = self.name)?;

        for (i, (k, v)) in self.members.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }

            write!(f, "{k}: {v}")?;
        }

        write!(f, "}}")
    }
}

/// An enum variant value.
///
/// A variant enum is the name of the enum variant and the type of the enum from
/// which that variant can be looked up.
///
/// This type is cheaply clonable.
#[derive(Debug, Clone)]
pub struct EnumVariant {
    /// The type of the enum containing this variant.
    enum_ty: EnumType,
    /// The index of the variant in the enum type.
    variant_index: usize,
    /// The value of the variant.
    value: Arc<Value>,
}

impl PartialEq for EnumVariant {
    fn eq(&self, other: &Self) -> bool {
        self.enum_ty == other.enum_ty && self.variant_index == other.variant_index
    }
}

impl EnumVariant {
    /// Attempts to create a new enum variant from a enum type and variant name.
    ///
    /// This method returns [`None`] if the variant is not in the enum.
    ///
    /// # Panics
    ///
    /// Panics if the given variant name is not present in the enum type.
    pub fn new(
        enum_ty: impl Into<EnumType>,
        name: &str,
        value: impl Into<Value>,
    ) -> Self {
        let enum_ty = enum_ty.into();
        let value = Arc::new(value.into());

        let variant_index = enum_ty
            .variants()
            .iter()
            .position(|v| v == name)
            .expect("variant name must exist in enum type");

        Self {
            enum_ty,
            variant_index,
            value,
        }
    }

    /// Gets the type of the enum.
    pub fn enum_ty(&self) -> EnumType {
        self.enum_ty.clone()
    }

    /// Gets the name of the variant.
    pub fn name(&self) -> &str {
        &self.enum_ty.variants()[self.variant_index]
    }

    /// Gets the name of the variant.
    pub fn value(&self) -> &Value {
        &self.value
    }
}

/// Displays the variant name when an enum is used in string interpolation.
///
/// # Design Decision
///
/// When an enum variant is interpolated in a WDL string (e.g., `"~{Color.Red}"`
/// where `Red = "#FF0000"`), this implementation displays the **variant name**
/// (`"Red"`) rather than the underlying **value** (`"#FF0000"`).
///
/// This design choice treats enum variants as named identifiers, providing
/// stable, human-readable output that doesn't depend on the underlying value
/// representation. To access the underlying value explicitly, use the `value()`
/// standard library function.
///
/// # Example
///
/// ```wdl
/// enum Color {
///     Red = "#FF0000",
///     Green = "#00FF00"
/// }
///
/// String name = "~{Color.Red}"       # Produces "Red"
/// String hex_value = value(Color.Red)  # Produces "#FF0000"
/// ```
impl fmt::Display for EnumVariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Represents a compound value.
///
/// Compound values are cheap to clone.
#[derive(Debug, Clone)]
pub enum CompoundValue {
    /// The value is a `Pair` of values.
    Pair(Pair),
    /// The value is an `Array` of values.
    Array(Array),
    /// The value is a `Map` of values.
    Map(Map),
    /// The value is an `Object`.
    Object(Object),
    /// The value is a struct.
    Struct(Struct),
    /// The value is an enum variant.
    EnumVariant(EnumVariant),
}

impl CompoundValue {
    /// Gets the type of the compound value.
    pub fn ty(&self) -> Type {
        match self {
            CompoundValue::Pair(v) => v.ty(),
            CompoundValue::Array(v) => v.ty(),
            CompoundValue::Map(v) => v.ty(),
            CompoundValue::Object(v) => v.ty(),
            CompoundValue::Struct(v) => v.ty(),
            CompoundValue::EnumVariant(v) => v.enum_ty().into(),
        }
    }

    /// Gets the value as a `Pair`.
    ///
    /// Returns `None` if the value is not a `Pair`.
    pub fn as_pair(&self) -> Option<&Pair> {
        match self {
            Self::Pair(v) => Some(v),
            _ => None,
        }
    }

    /// Unwraps the value into a `Pair`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `Pair`.
    pub fn unwrap_pair(self) -> Pair {
        match self {
            Self::Pair(v) => v,
            _ => panic!("value is not a pair"),
        }
    }

    /// Gets the value as an `Array`.
    ///
    /// Returns `None` if the value is not an `Array`.
    pub fn as_array(&self) -> Option<&Array> {
        match self {
            Self::Array(v) => Some(v),
            _ => None,
        }
    }

    /// Unwraps the value into an `Array`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not an `Array`.
    pub fn unwrap_array(self) -> Array {
        match self {
            Self::Array(v) => v,
            _ => panic!("value is not an array"),
        }
    }

    /// Gets the value as a `Map`.
    ///
    /// Returns `None` if the value is not a `Map`.
    pub fn as_map(&self) -> Option<&Map> {
        match self {
            Self::Map(v) => Some(v),
            _ => None,
        }
    }

    /// Unwraps the value into a `Map`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `Map`.
    pub fn unwrap_map(self) -> Map {
        match self {
            Self::Map(v) => v,
            _ => panic!("value is not a map"),
        }
    }

    /// Gets the value as an `Object`.
    ///
    /// Returns `None` if the value is not an `Object`.
    pub fn as_object(&self) -> Option<&Object> {
        match self {
            Self::Object(v) => Some(v),
            _ => None,
        }
    }

    /// Unwraps the value into an `Object`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not an `Object`.
    pub fn unwrap_object(self) -> Object {
        match self {
            Self::Object(v) => v,
            _ => panic!("value is not an object"),
        }
    }

    /// Gets the value as a `Struct`.
    ///
    /// Returns `None` if the value is not a `Struct`.
    pub fn as_struct(&self) -> Option<&Struct> {
        match self {
            Self::Struct(v) => Some(v),
            _ => None,
        }
    }

    /// Unwraps the value into a `Struct`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `Map`.
    pub fn unwrap_struct(self) -> Struct {
        match self {
            Self::Struct(v) => v,
            _ => panic!("value is not a struct"),
        }
    }

    /// Gets the value as an `EnumVariant`.
    ///
    /// Returns `None` if the value is not an `EnumVariant`.
    pub fn as_enum_variant(&self) -> Option<&EnumVariant> {
        match self {
            Self::EnumVariant(v) => Some(v),
            _ => None,
        }
    }

    /// Unwraps the value into an `EnumVariant`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not an `EnumVariant`.
    pub fn unwrap_enum_variant(self) -> EnumVariant {
        match self {
            Self::EnumVariant(v) => v,
            _ => panic!("value is not an enum"),
        }
    }

    /// Compares two compound values for equality based on the WDL
    /// specification.
    ///
    /// Returns `None` if the two compound values cannot be compared for
    /// equality.
    pub fn equals(left: &Self, right: &Self) -> Option<bool> {
        // The values must have type equivalence to compare for compound values
        // Coercion doesn't take place for this check
        if left.ty() != right.ty() {
            return None;
        }

        match (left, right) {
            (Self::Pair(left), Self::Pair(right)) => Some(
                Value::equals(left.left(), right.left())?
                    && Value::equals(left.right(), right.right())?,
            ),
            (CompoundValue::Array(left), CompoundValue::Array(right)) => Some(
                left.len() == right.len()
                    && left
                        .as_slice()
                        .iter()
                        .zip(right.as_slice())
                        .all(|(l, r)| Value::equals(l, r).unwrap_or(false)),
            ),
            (CompoundValue::Map(left), CompoundValue::Map(right)) => Some(
                left.len() == right.len()
                    // Maps are ordered, so compare via iteration
                    && left.iter().zip(right.iter()).all(|((lk, lv), (rk, rv))| {
                        match (lk, rk) {
                            (None, None) => {},
                            (Some(lk), Some(rk)) if lk == rk => {},
                            _ => return false
                        }

                        Value::equals(lv, rv).unwrap_or(false)
                    }),
            ),
            (CompoundValue::Object(left), CompoundValue::Object(right)) => Some(
                left.len() == right.len()
                    && left.iter().all(|(k, left)| match right.get(k) {
                        Some(right) => Value::equals(left, right).unwrap_or(false),
                        None => false,
                    }),
            ),
            (
                CompoundValue::Struct(Struct { members: left, .. }),
                CompoundValue::Struct(Struct { members: right, .. }),
            ) => Some(
                left.len() == right.len()
                    && left.iter().all(|(k, left)| match right.get(k) {
                        Some(right) => Value::equals(left, right).unwrap_or(false),
                        None => false,
                    }),
            ),
            (CompoundValue::EnumVariant(left), CompoundValue::EnumVariant(right)) => {
                Some(left.enum_ty() == right.enum_ty() && left.name() == right.name())
            }
            _ => None,
        }
    }

    /// Visits any paths referenced by this value.
    ///
    /// The callback is invoked for each `File` and `Directory` value referenced
    /// by this value.
    fn visit_paths<F>(&self, cb: &mut F) -> Result<()>
    where
        F: FnMut(bool, &HostPath) -> Result<()> + Send + Sync,
    {
        match self {
            Self::Pair(pair) => {
                pair.left().visit_paths(cb)?;
                pair.right().visit_paths(cb)?;
            }
            Self::Array(array) => {
                for v in array.as_slice() {
                    v.visit_paths(cb)?;
                }
            }
            Self::Map(map) => {
                for (k, v) in map.iter() {
                    match k {
                        Some(PrimitiveValue::File(path)) => cb(true, path)?,
                        Some(PrimitiveValue::Directory(path)) => cb(false, path)?,
                        _ => {}
                    }

                    v.visit_paths(cb)?;
                }
            }
            Self::Object(object) => {
                for v in object.values() {
                    v.visit_paths(cb)?;
                }
            }
            Self::Struct(s) => {
                for v in s.values() {
                    v.visit_paths(cb)?;
                }
            }
            Self::EnumVariant(e) => {
                e.value().visit_paths(cb)?;
            }
        }

        Ok(())
    }

    /// Like [`Value::resolve_paths()`], but for recurring into
    /// [`CompoundValue`]s.
    fn resolve_paths<'a, F>(
        &'a self,
        base_dir: Option<&'a Path>,
        transferer: Option<&'a dyn Transferer>,
        translate: &'a F,
    ) -> BoxFuture<'a, Result<Self>>
    where
        F: Fn(&HostPath) -> Result<HostPath> + Send + Sync,
    {
        async move {
            match self {
                Self::Pair(pair) => {
                    let ty = pair.ty.as_pair().expect("should be a pair type");
                    let (left_optional, right_optional) =
                        (ty.left_type().is_optional(), ty.right_type().is_optional());
                    let (fst, snd) = pair.values.as_ref();
                    let fst = fst
                        .resolve_paths(left_optional, base_dir, transferer, translate)
                        .await?;
                    let snd = snd
                        .resolve_paths(right_optional, base_dir, transferer, translate)
                        .await?;
                    Ok(Self::Pair(Pair::new_unchecked(ty.clone().into(), fst, snd)))
                }
                Self::Array(array) => {
                    let ty = array.ty.as_array().expect("should be an array type");
                    let optional = ty.element_type().is_optional();
                    if let Some(elements) = &array.elements {
                        let resolved_elements = futures::stream::iter(elements.iter())
                            .then(|v| v.resolve_paths(optional, base_dir, transferer, translate))
                            .try_collect()
                            .await?;
                        Ok(Self::Array(Array::new_unchecked(
                            ty.clone().into(),
                            resolved_elements,
                        )))
                    } else {
                        Ok(self.clone())
                    }
                }
                Self::Map(map) => {
                    let ty = map.ty.as_map().expect("should be a map type").clone();
                    let (key_optional, value_optional) =
                        (ty.key_type().is_optional(), ty.value_type().is_optional());
                    if let Some(elements) = &map.elements {
                        let resolved_elements = futures::stream::iter(elements.iter())
                            .then(async |(k, v)| {
                                let resolved_key = if let Some(k) = k {
                                    Value::from(k.clone())
                                        .resolve_paths(
                                            key_optional,
                                            base_dir,
                                            transferer,
                                            translate,
                                        )
                                        .await?
                                        .as_primitive()
                                        .cloned()
                                } else {
                                    None
                                };
                                let resolved_value = v
                                    .resolve_paths(value_optional, base_dir, transferer, translate)
                                    .await?;
                                Ok::<_, anyhow::Error>((resolved_key, resolved_value))
                            })
                            .try_collect()
                            .await?;
                        Ok(Self::Map(Map::new_unchecked(ty.into(), resolved_elements)))
                    } else {
                        Ok(Self::Map(Map::new_unchecked(ty.into(), IndexMap::new())))
                    }
                }
                Self::Object(object) => {
                    if let Some(members) = &object.members {
                        let resolved_members = futures::stream::iter(members.iter())
                            .then(async |(n, v)| {
                                let resolved = v
                                    .resolve_paths(false, base_dir, transferer, translate)
                                    .await?;
                                Ok::<_, anyhow::Error>((n.to_string(), resolved))
                            })
                            .try_collect()
                            .await?;
                        Ok(Self::Object(Object::new(resolved_members)))
                    } else {
                        Ok(self.clone())
                    }
                }
                Self::Struct(s) => {
                    let ty = s.ty.as_struct().expect("should be a struct type");
                    let name = s.name();
                    let resolved_members = futures::stream::iter(s.iter())
                        .then(async |(n, v)| {
                            let resolved = v
                                .resolve_paths(
                                    ty.members()[n].is_optional(),
                                    base_dir,
                                    transferer,
                                    translate,
                                )
                                .await?;
                            Ok::<_, anyhow::Error>((n.to_string(), resolved))
                        })
                        .try_collect()
                        .await?;
                    Ok(Self::Struct(Struct::new_unchecked(
                        ty.clone().into(),
                        name.clone(),
                        Arc::new(resolved_members),
                    )))
                }
                Self::EnumVariant(e) => {
                    let optional = e.enum_ty().inner_value_type().is_optional();
                    let value = e
                        .value
                        .resolve_paths(optional, base_dir, transferer, translate)
                        .await?;
                    Ok(Self::EnumVariant(EnumVariant::new(
                        e.enum_ty.clone(),
                        e.name(),
                        value,
                    )))
                }
            }
        }
        .boxed()
    }
}

impl fmt::Display for CompoundValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pair(v) => v.fmt(f),
            Self::Array(v) => v.fmt(f),
            Self::Map(v) => v.fmt(f),
            Self::Object(v) => v.fmt(f),
            Self::Struct(v) => v.fmt(f),
            Self::EnumVariant(v) => v.fmt(f),
        }
    }
}

impl Coercible for CompoundValue {
    fn coerce(&self, context: Option<&dyn EvaluationContext>, target: &Type) -> Result<Self> {
        if target.is_union() || target.is_none() || self.ty().eq(target) {
            return Ok(self.clone());
        }

        if let Type::Compound(target_ty, _) = target {
            match (self, target_ty) {
                // Array[X] -> Array[Y](+) where X -> Y
                (Self::Array(v), CompoundType::Array(target_ty)) => {
                    // Don't allow coercion when the source is empty but the target has the
                    // non-empty qualifier
                    if v.is_empty() && target_ty.is_non_empty() {
                        bail!("cannot coerce empty array value to non-empty array type `{target}`",);
                    }

                    return Ok(Self::Array(Array::new(
                        context,
                        target.clone(),
                        v.as_slice().iter().cloned(),
                    )?));
                }
                // Map[W, Y] -> Map[X, Z] where W -> X and Y -> Z
                (Self::Map(v), CompoundType::Map(map_ty)) => {
                    return Ok(Self::Map(Map::new(
                        context,
                        target.clone(),
                        v.iter().map(|(k, v)| {
                            (
                                k.clone()
                                    .map(Into::into)
                                    .unwrap_or(Value::new_none(map_ty.key_type().optional())),
                                v.clone(),
                            )
                        }),
                    )?));
                }
                // Pair[W, Y] -> Pair[X, Z] where W -> X and Y -> Z
                (Self::Pair(v), CompoundType::Pair(_)) => {
                    return Ok(Self::Pair(Pair::new(
                        context,
                        target.clone(),
                        v.values.0.clone(),
                        v.values.1.clone(),
                    )?));
                }
                // Map[X, Y] -> Struct where: X -> String
                (Self::Map(v), CompoundType::Custom(CustomType::Struct(target_ty))) => {
                    let len = v.len();
                    let expected_len = target_ty.members().len();

                    if len != expected_len {
                        bail!(
                            "cannot coerce a map of {len} element{s1} to struct type `{target}` \
                             as the struct has {expected_len} member{s2}",
                            s1 = if len == 1 { "" } else { "s" },
                            s2 = if expected_len == 1 { "" } else { "s" }
                        );
                    }

                    return Ok(Self::Struct(Struct {
                        ty: target.clone(),
                        name: target_ty.name().clone(),
                        members: Arc::new(
                            v.iter()
                                .map(|(k, v)| {
                                    let k = k
                                        .as_ref()
                                        .and_then(|k| {
                                            k.coerce(context, &PrimitiveType::String.into()).ok()
                                        })
                                        .with_context(|| {
                                            format!(
                                                "cannot coerce a map of type `{map_type}` to \
                                                 struct type `{target}` as the key type cannot \
                                                 coerce to `String`",
                                                map_type = v.ty()
                                            )
                                        })?
                                        .unwrap_string();
                                    let ty =
                                        target_ty.members().get(k.as_ref()).with_context(|| {
                                            format!(
                                                "cannot coerce a map with key `{k}` to struct \
                                                 type `{target}` as the struct does not contain a \
                                                 member with that name"
                                            )
                                        })?;
                                    let v = v.coerce(context, ty).with_context(|| {
                                        format!("failed to coerce value of map key `{k}")
                                    })?;
                                    Ok((k.to_string(), v))
                                })
                                .collect::<Result<_>>()?,
                        ),
                    }));
                }
                // Struct -> Map[X, Y] where: String -> X
                (Self::Struct(Struct { members, .. }), CompoundType::Map(map_ty)) => {
                    let key_ty = map_ty.key_type();
                    if !Type::from(PrimitiveType::String).is_coercible_to(key_ty) {
                        bail!(
                            "cannot coerce a struct to type `{target}` as key type `{key_ty}` \
                             cannot be coerced from `String`"
                        );
                    }

                    let value_ty = map_ty.value_type();
                    return Ok(Self::Map(Map::new_unchecked(
                        target.clone(),
                        members
                            .iter()
                            .map(|(n, v)| {
                                let v = v
                                    .coerce(context, value_ty)
                                    .with_context(|| format!("failed to coerce member `{n}`"))?;
                                Ok((
                                    PrimitiveValue::new_string(n)
                                        .coerce(context, key_ty)
                                        .expect("should coerce")
                                        .into(),
                                    v,
                                ))
                            })
                            .collect::<Result<_>>()?,
                    )));
                }
                // Object -> Map[X, Y] where: String -> X
                (Self::Object(object), CompoundType::Map(map_ty)) => {
                    let key_ty = map_ty.key_type();
                    if !Type::from(PrimitiveType::String).is_coercible_to(key_ty) {
                        bail!(
                            "cannot coerce an object to type `{target}` as key type `{key_ty}` \
                             cannot be coerced from `String`"
                        );
                    }

                    let value_ty = map_ty.value_type();
                    return Ok(Self::Map(Map::new_unchecked(
                        target.clone(),
                        object
                            .iter()
                            .map(|(n, v)| {
                                let v = v
                                    .coerce(context, value_ty)
                                    .with_context(|| format!("failed to coerce member `{n}`"))?;
                                Ok((
                                    PrimitiveValue::new_string(n)
                                        .coerce(context, key_ty)
                                        .expect("should coerce")
                                        .into(),
                                    v,
                                ))
                            })
                            .collect::<Result<_>>()?,
                    )));
                }
                // Object -> Struct
                (Self::Object(v), CompoundType::Custom(CustomType::Struct(_))) => {
                    return Ok(Self::Struct(Struct::new(
                        context,
                        target.clone(),
                        v.iter().map(|(k, v)| (k, v.clone())),
                    )?));
                }
                // Struct -> Struct
                (Self::Struct(v), CompoundType::Custom(CustomType::Struct(struct_ty))) => {
                    let len = v.members.len();
                    let expected_len = struct_ty.members().len();

                    if len != expected_len {
                        bail!(
                            "cannot coerce a struct of {len} members{s1} to struct type \
                             `{target}` as the target struct has {expected_len} member{s2}",
                            s1 = if len == 1 { "" } else { "s" },
                            s2 = if expected_len == 1 { "" } else { "s" }
                        );
                    }

                    return Ok(Self::Struct(Struct {
                        ty: target.clone(),
                        name: struct_ty.name().clone(),
                        members: Arc::new(
                            v.members
                                .iter()
                                .map(|(k, v)| {
                                    let ty = struct_ty.members().get(k).ok_or_else(|| {
                                        anyhow!(
                                            "cannot coerce a struct with member `{k}` to struct \
                                             type `{target}` as the target struct does not \
                                             contain a member with that name",
                                        )
                                    })?;
                                    let v = v.coerce(context, ty).with_context(|| {
                                        format!("failed to coerce member `{k}`")
                                    })?;
                                    Ok((k.clone(), v))
                                })
                                .collect::<Result<_>>()?,
                        ),
                    }));
                }
                _ => {}
            }
        }

        if let Type::Object = target {
            match self {
                // Map[X, Y] -> Object where: X -> String
                Self::Map(v) => {
                    return Ok(Self::Object(Object::new(
                        v.iter()
                            .map(|(k, v)| {
                                let k = k
                                    .as_ref()
                                    .and_then(|k| {
                                        k.coerce(context, &PrimitiveType::String.into()).ok()
                                    })
                                    .with_context(|| {
                                        format!(
                                            "cannot coerce a map of type `{map_type}` to `Object` \
                                             as the key type cannot coerce to `String`",
                                            map_type = v.ty()
                                        )
                                    })?
                                    .unwrap_string();
                                Ok((k.to_string(), v.clone()))
                            })
                            .collect::<Result<IndexMap<_, _>>>()?,
                    )));
                }
                // Struct -> Object
                Self::Struct(v) => {
                    return Ok(Self::Object(Object {
                        members: Some(v.members.clone()),
                    }));
                }
                _ => {}
            };
        }

        bail!(
            "cannot coerce a value of type `{ty}` to type `{target}`",
            ty = self.ty()
        );
    }
}

impl From<Pair> for CompoundValue {
    fn from(value: Pair) -> Self {
        Self::Pair(value)
    }
}

impl From<Array> for CompoundValue {
    fn from(value: Array) -> Self {
        Self::Array(value)
    }
}

impl From<Map> for CompoundValue {
    fn from(value: Map) -> Self {
        Self::Map(value)
    }
}

impl From<Object> for CompoundValue {
    fn from(value: Object) -> Self {
        Self::Object(value)
    }
}

impl From<Struct> for CompoundValue {
    fn from(value: Struct) -> Self {
        Self::Struct(value)
    }
}

/// Represents a hidden value.
///
/// Hidden values are cheap to clone.
#[derive(Debug, Clone)]
pub enum HiddenValue {
    /// The value is a hints value.
    ///
    /// Hints values only appear in a task hints section in WDL 1.2.
    Hints(HintsValue),
    /// The value is an input value.
    ///
    /// Input values only appear in a task hints section in WDL 1.2.
    Input(InputValue),
    /// The value is an output value.
    ///
    /// Output values only appear in a task hints section in WDL 1.2.
    Output(OutputValue),
    /// The value is a task variable before evaluation.
    ///
    /// This value occurs during requirements, hints, and runtime section
    /// evaluation in WDL 1.3+ tasks.
    TaskPreEvaluation(TaskPreEvaluationValue),
    /// The value is a task variable after evaluation.
    ///
    /// This value occurs during command and output section evaluation in
    /// WDL 1.2+ tasks.
    TaskPostEvaluation(TaskPostEvaluationValue),
    /// The value is a previous requirements value.
    ///
    /// This value contains the previous attempt's requirements and is available
    /// in WDL 1.3+ via `task.previous`.
    PreviousTaskData(PreviousTaskDataValue),
}

impl HiddenValue {
    /// Gets the type of the value.
    pub fn ty(&self) -> Type {
        match self {
            Self::Hints(_) => Type::Hidden(HiddenType::Hints),
            Self::Input(_) => Type::Hidden(HiddenType::Input),
            Self::Output(_) => Type::Hidden(HiddenType::Output),
            Self::TaskPreEvaluation(_) => Type::Hidden(HiddenType::TaskPreEvaluation),
            Self::TaskPostEvaluation(_) => Type::Hidden(HiddenType::TaskPostEvaluation),
            Self::PreviousTaskData(_) => Type::Hidden(HiddenType::PreviousTaskData),
        }
    }
}

impl fmt::Display for HiddenValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hints(v) => v.fmt(f),
            Self::Input(v) => v.fmt(f),
            Self::Output(v) => v.fmt(f),
            Self::TaskPreEvaluation(_) | Self::TaskPostEvaluation(_) => write!(f, "task"),
            Self::PreviousTaskData(_) => write!(f, "task.previous"),
        }
    }
}

impl Coercible for HiddenValue {
    fn coerce(&self, _: Option<&dyn EvaluationContext>, target: &Type) -> Result<Self> {
        match self {
            Self::Hints(_) => {
                if matches!(target, Type::Hidden(HiddenType::Hints)) {
                    return Ok(self.clone());
                }

                bail!("hints values cannot be coerced to any other type");
            }
            Self::Input(_) => {
                if matches!(target, Type::Hidden(HiddenType::Input)) {
                    return Ok(self.clone());
                }

                bail!("input values cannot be coerced to any other type");
            }
            Self::Output(_) => {
                if matches!(target, Type::Hidden(HiddenType::Output)) {
                    return Ok(self.clone());
                }

                bail!("output values cannot be coerced to any other type");
            }
            Self::TaskPreEvaluation(_) | Self::TaskPostEvaluation(_) => {
                if matches!(
                    target,
                    Type::Hidden(HiddenType::TaskPreEvaluation)
                        | Type::Hidden(HiddenType::TaskPostEvaluation)
                ) {
                    return Ok(self.clone());
                }

                bail!("task variables cannot be coerced to any other type");
            }
            Self::PreviousTaskData(_) => {
                if matches!(target, Type::Hidden(HiddenType::PreviousTaskData)) {
                    return Ok(self.clone());
                }

                bail!("previous task data values cannot be coerced to any other type");
            }
        }
    }
}

/// Immutable data for task values after requirements evaluation (WDL 1.2+).
///
/// Contains all evaluated requirement fields.
#[derive(Debug, Clone)]
pub struct TaskPostEvaluationData {
    /// The container of the task.
    container: Option<Arc<String>>,
    /// The allocated number of cpus for the task.
    cpu: f64,
    /// The allocated memory (in bytes) for the task.
    memory: i64,
    /// The GPU allocations for the task.
    ///
    /// An array with one specification per allocated GPU; the specification is
    /// execution engine-specific.
    gpu: Array,
    /// The FPGA allocations for the task.
    ///
    /// An array with one specification per allocated FPGA; the specification is
    /// execution engine-specific.
    fpga: Array,
    /// The disk allocations for the task.
    ///
    /// A map with one entry for each disk mount point.
    ///
    /// The key is the mount point and the value is the initial amount of disk
    /// space allocated, in bytes.
    disks: Map,
    /// The maximum number of retries for the task.
    max_retries: i64,
}

/// Represents a `task.previous` value containing data from a previous attempt.
///
/// The data is stored in an `Arc<TaskPostEvaluationData>` for cheap cloning.
#[derive(Debug, Clone)]
pub struct PreviousTaskDataValue(Option<Arc<TaskPostEvaluationData>>);

impl PreviousTaskDataValue {
    /// Creates a new previous task data from task post-evaluation data.
    pub fn new(data: Arc<TaskPostEvaluationData>) -> Self {
        Self(Some(data))
    }

    /// Creates an empty previous task data (for first attempt).
    pub fn empty() -> Self {
        Self(None)
    }

    /// Gets the value of a field in the previous task data.
    ///
    /// Returns `None` if the field name is not valid for previous task data.
    /// Returns `Some(Value::None)` for valid fields when there is no previous
    /// data (first attempt).
    pub fn field(&self, name: &str) -> Option<Value> {
        match name {
            TASK_FIELD_MEMORY => Some(
                self.0
                    .as_ref()
                    .map(|data| Value::from(data.memory))
                    .unwrap_or_else(|| {
                        Value::new_none(Type::from(PrimitiveType::Integer).optional())
                    }),
            ),
            TASK_FIELD_CPU => Some(
                self.0
                    .as_ref()
                    .map(|data| Value::from(data.cpu))
                    .unwrap_or_else(|| {
                        Value::new_none(Type::from(PrimitiveType::Float).optional())
                    }),
            ),
            TASK_FIELD_CONTAINER => Some(
                self.0
                    .as_ref()
                    .and_then(|data| {
                        data.container
                            .as_ref()
                            .map(|c| PrimitiveValue::String(c.clone()).into())
                    })
                    .unwrap_or_else(|| {
                        Value::new_none(Type::from(PrimitiveType::String).optional())
                    }),
            ),
            TASK_FIELD_GPU => Some(
                self.0
                    .as_ref()
                    .map(|data| Value::from(data.gpu.clone()))
                    .unwrap_or_else(|| {
                        Value::new_none(Type::Compound(
                            CompoundType::Array(ArrayType::new(PrimitiveType::String)),
                            true,
                        ))
                    }),
            ),
            TASK_FIELD_FPGA => Some(
                self.0
                    .as_ref()
                    .map(|data| Value::from(data.fpga.clone()))
                    .unwrap_or_else(|| {
                        Value::new_none(Type::Compound(
                            CompoundType::Array(ArrayType::new(PrimitiveType::String)),
                            true,
                        ))
                    }),
            ),
            TASK_FIELD_DISKS => Some(
                self.0
                    .as_ref()
                    .map(|data| Value::from(data.disks.clone()))
                    .unwrap_or_else(|| {
                        Value::new_none(Type::Compound(
                            CompoundType::Map(Arc::new(MapType::new(
                                PrimitiveType::String,
                                PrimitiveType::Integer,
                            ))),
                            true,
                        ))
                    }),
            ),
            TASK_FIELD_MAX_RETRIES => Some(
                self.0
                    .as_ref()
                    .map(|data| Value::from(data.max_retries))
                    .unwrap_or_else(|| {
                        Value::new_none(Type::from(PrimitiveType::Integer).optional())
                    }),
            ),
            _ => None,
        }
    }
}

/// Represents a `task` variable value before requirements evaluation (WDL
/// 1.3+).
///
/// Only exposes `name`, `id`, `attempt`, `previous`, and metadata fields.
///
/// Task values are cheap to clone.
#[derive(Debug, Clone)]
pub struct TaskPreEvaluationValue {
    /// The task name.
    name: Arc<String>,
    /// The task id.
    id: Arc<String>,
    /// The current task attempt count.
    ///
    /// The value must be 0 the first time the task is executed and incremented
    /// by 1 each time the task is retried (if any).
    attempt: i64,
    /// The task's `meta` section as an object.
    meta: Object,
    /// The tasks's `parameter_meta` section as an object.
    parameter_meta: Object,
    /// The task's extension metadata.
    ext: Object,
    /// The previous attempt's task data (WDL 1.3+).
    ///
    /// Contains the evaluated task data from the previous attempt.
    ///
    /// On the first attempt, this is empty.
    previous: PreviousTaskDataValue,
}

impl TaskPreEvaluationValue {
    /// Constructs a new pre-evaluation task value with the given name and
    /// identifier.
    pub(crate) fn new(
        name: impl Into<String>,
        id: impl Into<String>,
        attempt: i64,
        meta: Object,
        parameter_meta: Object,
        ext: Object,
    ) -> Self {
        Self {
            name: Arc::new(name.into()),
            id: Arc::new(id.into()),
            meta,
            parameter_meta,
            ext,
            attempt,
            previous: PreviousTaskDataValue::empty(),
        }
    }

    /// Sets the previous task data for retry attempts.
    pub(crate) fn set_previous(&mut self, data: Arc<TaskPostEvaluationData>) {
        self.previous = PreviousTaskDataValue::new(data);
    }

    /// Gets the task name.
    pub fn name(&self) -> &Arc<String> {
        &self.name
    }

    /// Gets the unique ID of the task.
    pub fn id(&self) -> &Arc<String> {
        &self.id
    }

    /// Gets current task attempt count.
    pub fn attempt(&self) -> i64 {
        self.attempt
    }

    /// Accesses a field of the task value by name.
    ///
    /// Returns `None` if the name is not a known field name.
    pub fn field(&self, name: &str) -> Option<Value> {
        match name {
            TASK_FIELD_NAME => Some(PrimitiveValue::String(self.name.clone()).into()),
            TASK_FIELD_ID => Some(PrimitiveValue::String(self.id.clone()).into()),
            TASK_FIELD_ATTEMPT => Some(self.attempt.into()),
            TASK_FIELD_META => Some(self.meta.clone().into()),
            TASK_FIELD_PARAMETER_META => Some(self.parameter_meta.clone().into()),
            TASK_FIELD_EXT => Some(self.ext.clone().into()),
            TASK_FIELD_PREVIOUS => {
                Some(HiddenValue::PreviousTaskData(self.previous.clone()).into())
            }
            _ => None,
        }
    }
}

/// Represents a `task` variable value after requirements evaluation (WDL 1.2+).
///
/// Exposes all task fields including evaluated constraints.
///
/// Task values are cheap to clone.
#[derive(Debug, Clone)]
pub struct TaskPostEvaluationValue {
    /// The immutable data for task values including evaluated requirements.
    data: Arc<TaskPostEvaluationData>,
    /// The task name.
    name: Arc<String>,
    /// The task id.
    id: Arc<String>,
    /// The current task attempt count.
    ///
    /// The value must be 0 the first time the task is executed and incremented
    /// by 1 each time the task is retried (if any).
    attempt: i64,
    /// The task's `meta` section as an object.
    meta: Object,
    /// The tasks's `parameter_meta` section as an object.
    parameter_meta: Object,
    /// The task's extension metadata.
    ext: Object,
    /// The task's return code.
    ///
    /// Initially set to [`None`], but set after task execution completes.
    return_code: Option<i64>,
    /// The time by which the task must be completed, as a Unix time stamp.
    ///
    /// A value of `None` indicates there is no deadline.
    end_time: Option<i64>,
    /// The previous attempt's task data (WDL 1.3+).
    ///
    /// Contains the evaluated task data from the previous attempt.
    ///
    /// On the first attempt, this is empty.
    previous: PreviousTaskDataValue,
}

impl TaskPostEvaluationValue {
    /// Constructs a new post-evaluation task value with the given name,
    /// identifier, and constraints.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        name: impl Into<String>,
        id: impl Into<String>,
        constraints: TaskExecutionConstraints,
        max_retries: i64,
        attempt: i64,
        meta: Object,
        parameter_meta: Object,
        ext: Object,
    ) -> Self {
        Self {
            name: Arc::new(name.into()),
            id: Arc::new(id.into()),
            data: Arc::new(TaskPostEvaluationData {
                container: constraints.container.map(Into::into),
                cpu: constraints.cpu,
                memory: constraints.memory,
                gpu: Array::new_unchecked(
                    ANALYSIS_STDLIB.array_string_type().clone(),
                    constraints
                        .gpu
                        .into_iter()
                        .map(|v| PrimitiveValue::new_string(v).into())
                        .collect(),
                ),
                fpga: Array::new_unchecked(
                    ANALYSIS_STDLIB.array_string_type().clone(),
                    constraints
                        .fpga
                        .into_iter()
                        .map(|v| PrimitiveValue::new_string(v).into())
                        .collect(),
                ),
                disks: Map::new_unchecked(
                    ANALYSIS_STDLIB.map_string_int_type().clone(),
                    constraints
                        .disks
                        .into_iter()
                        .map(|(k, v)| (Some(PrimitiveValue::new_string(k)), v.into()))
                        .collect(),
                ),
                max_retries,
            }),
            attempt,
            meta,
            parameter_meta,
            ext,
            return_code: None,
            end_time: None,
            previous: PreviousTaskDataValue::empty(),
        }
    }

    /// Gets the task name.
    pub fn name(&self) -> &Arc<String> {
        &self.name
    }

    /// Gets the unique ID of the task.
    pub fn id(&self) -> &Arc<String> {
        &self.id
    }

    /// Gets the container in which the task is executing.
    pub fn container(&self) -> Option<&Arc<String>> {
        self.data.container.as_ref()
    }

    /// Gets the allocated number of cpus for the task.
    pub fn cpu(&self) -> f64 {
        self.data.cpu
    }

    /// Gets the allocated memory (in bytes) for the task.
    pub fn memory(&self) -> i64 {
        self.data.memory
    }

    /// Gets the GPU allocations for the task.
    ///
    /// An array with one specification per allocated GPU; the specification is
    /// execution engine-specific.
    pub fn gpu(&self) -> &Array {
        &self.data.gpu
    }

    /// Gets the FPGA allocations for the task.
    ///
    /// An array with one specification per allocated FPGA; the specification is
    /// execution engine-specific.
    pub fn fpga(&self) -> &Array {
        &self.data.fpga
    }

    /// Gets the disk allocations for the task.
    ///
    /// A map with one entry for each disk mount point.
    ///
    /// The key is the mount point and the value is the initial amount of disk
    /// space allocated, in bytes.
    pub fn disks(&self) -> &Map {
        &self.data.disks
    }

    /// Gets current task attempt count.
    ///
    /// The value must be 0 the first time the task is executed and incremented
    /// by 1 each time the task is retried (if any).
    pub fn attempt(&self) -> i64 {
        self.attempt
    }

    /// Gets the time by which the task must be completed, as a Unix time stamp.
    ///
    /// A value of `None` indicates there is no deadline.
    pub fn end_time(&self) -> Option<i64> {
        self.end_time
    }

    /// Gets the task's return code.
    ///
    /// Initially set to `None`, but set after task execution completes.
    pub fn return_code(&self) -> Option<i64> {
        self.return_code
    }

    /// Gets the task's `meta` section as an object.
    pub fn meta(&self) -> &Object {
        &self.meta
    }

    /// Gets the tasks's `parameter_meta` section as an object.
    pub fn parameter_meta(&self) -> &Object {
        &self.parameter_meta
    }

    /// Gets the task's extension metadata.
    pub fn ext(&self) -> &Object {
        &self.ext
    }

    /// Sets the return code after the task execution has completed.
    pub(crate) fn set_return_code(&mut self, code: i32) {
        self.return_code = Some(code as i64);
    }

    /// Sets the attempt number for the task.
    pub(crate) fn set_attempt(&mut self, attempt: i64) {
        self.attempt = attempt;
    }

    /// Sets the previous task data for retry attempts.
    pub(crate) fn set_previous(&mut self, data: Arc<TaskPostEvaluationData>) {
        self.previous = PreviousTaskDataValue::new(data);
    }

    /// Gets the task post-evaluation data.
    pub(crate) fn data(&self) -> &Arc<TaskPostEvaluationData> {
        &self.data
    }

    /// Accesses a field of the task value by name.
    ///
    /// Returns `None` if the name is not a known field name.
    pub fn field(&self, version: SupportedVersion, name: &str) -> Option<Value> {
        match name {
            TASK_FIELD_NAME => Some(PrimitiveValue::String(self.name.clone()).into()),
            TASK_FIELD_ID => Some(PrimitiveValue::String(self.id.clone()).into()),
            TASK_FIELD_ATTEMPT => Some(self.attempt.into()),
            TASK_FIELD_CONTAINER => Some(
                self.data
                    .container
                    .clone()
                    .map(|c| PrimitiveValue::String(c).into())
                    .unwrap_or_else(|| {
                        Value::new_none(
                            task_member_type_post_evaluation(version, TASK_FIELD_CONTAINER)
                                .expect("failed to get task field type"),
                        )
                    }),
            ),
            TASK_FIELD_CPU => Some(self.data.cpu.into()),
            TASK_FIELD_MEMORY => Some(self.data.memory.into()),
            TASK_FIELD_GPU => Some(self.data.gpu.clone().into()),
            TASK_FIELD_FPGA => Some(self.data.fpga.clone().into()),
            TASK_FIELD_DISKS => Some(self.data.disks.clone().into()),
            TASK_FIELD_END_TIME => Some(self.end_time.map(Into::into).unwrap_or_else(|| {
                Value::new_none(
                    task_member_type_post_evaluation(version, TASK_FIELD_END_TIME)
                        .expect("failed to get task field type"),
                )
            })),
            TASK_FIELD_RETURN_CODE => Some(self.return_code.map(Into::into).unwrap_or_else(|| {
                Value::new_none(
                    task_member_type_post_evaluation(version, TASK_FIELD_RETURN_CODE)
                        .expect("failed to get task field type"),
                )
            })),
            TASK_FIELD_META => Some(self.meta.clone().into()),
            TASK_FIELD_PARAMETER_META => Some(self.parameter_meta.clone().into()),
            TASK_FIELD_EXT => Some(self.ext.clone().into()),
            TASK_FIELD_MAX_RETRIES if version >= SupportedVersion::V1(V1::Three) => {
                Some(self.data.max_retries.into())
            }
            TASK_FIELD_PREVIOUS if version >= SupportedVersion::V1(V1::Three) => {
                Some(HiddenValue::PreviousTaskData(self.previous.clone()).into())
            }
            _ => None,
        }
    }
}

/// Represents a hints value from a WDL 1.2 hints section.
///
/// Hints values are cheap to clone.
#[derive(Debug, Clone)]
pub struct HintsValue(Object);

impl HintsValue {
    /// Converts the hints value to an object.
    pub fn as_object(&self) -> &Object {
        &self.0
    }
}

impl fmt::Display for HintsValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "hints {{")?;

        for (i, (k, v)) in self.0.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }

            write!(f, "{k}: {v}")?;
        }

        write!(f, "}}")
    }
}

impl From<Object> for HintsValue {
    fn from(value: Object) -> Self {
        Self(value)
    }
}

/// Represents an input value from a WDL 1.2 hints section.
///
/// Input values are cheap to clone.
#[derive(Debug, Clone)]
pub struct InputValue(Object);

impl InputValue {
    /// Converts the input value to an object.
    pub fn as_object(&self) -> &Object {
        &self.0
    }
}

impl fmt::Display for InputValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "input {{")?;

        for (i, (k, v)) in self.0.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }

            write!(f, "{k}: {v}")?;
        }

        write!(f, "}}")
    }
}

impl From<Object> for InputValue {
    fn from(value: Object) -> Self {
        Self(value)
    }
}

/// Represents an output value from a WDL 1.2 hints section.
///
/// Output values are cheap to clone.
#[derive(Debug, Clone)]
pub struct OutputValue(Object);

impl OutputValue {
    /// Converts the output value to an object.
    pub fn as_object(&self) -> &Object {
        &self.0
    }
}

impl fmt::Display for OutputValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "output {{")?;

        for (i, (k, v)) in self.0.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }

            write!(f, "{k}: {v}")?;
        }

        write!(f, "}}")
    }
}

impl From<Object> for OutputValue {
    fn from(value: Object) -> Self {
        Self(value)
    }
}

/// Represents the outputs of a call.
///
/// Call values are cheap to clone.
#[derive(Debug, Clone)]
pub struct CallValue {
    /// The type of the call.
    ty: CallType,
    /// The outputs of the call.
    outputs: Arc<Outputs>,
}

impl CallValue {
    /// Constructs a new call value without checking the outputs conform to the
    /// call type.
    pub(crate) fn new_unchecked(ty: CallType, outputs: Arc<Outputs>) -> Self {
        Self { ty, outputs }
    }

    /// Gets the type of the call.
    pub fn ty(&self) -> &CallType {
        &self.ty
    }

    /// Gets the outputs of the call.
    pub fn outputs(&self) -> &Outputs {
        self.outputs.as_ref()
    }
}

impl fmt::Display for CallValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "call output {{")?;

        for (i, (k, v)) in self.outputs.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }

            write!(f, "{k}: {v}")?;
        }

        write!(f, "}}")
    }
}

/// Serializes a value with optional serialization of pairs.
pub struct ValueSerializer<'a> {
    /// The value to serialize.
    value: &'a Value,
    /// Whether pairs should be serialized as a map with `left` and `right`
    /// keys.
    allow_pairs: bool,
}

impl<'a> ValueSerializer<'a> {
    /// Constructs a new `ValueSerializer`.
    pub fn new(value: &'a Value, allow_pairs: bool) -> Self {
        Self { value, allow_pairs }
    }
}

impl serde::Serialize for ValueSerializer<'_> {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::Error;

        match &self.value {
            Value::None(_) => serializer.serialize_none(),
            Value::Primitive(v) => v.serialize(serializer),
            Value::Compound(v) => {
                CompoundValueSerializer::new(v, self.allow_pairs).serialize(serializer)
            }
            Value::Call(_) | Value::Hidden(_) | Value::TypeNameRef(_) => {
                Err(S::Error::custom("value cannot be serialized"))
            }
        }
    }
}

/// Serializes a `CompoundValue` with optional serialization of pairs.
pub struct CompoundValueSerializer<'a> {
    /// The compound value to serialize.
    value: &'a CompoundValue,
    /// Whether pairs should be serialized as a map with `left` and `right`
    /// keys.
    allow_pairs: bool,
}

impl<'a> CompoundValueSerializer<'a> {
    /// Constructs a new `CompoundValueSerializer`.
    pub fn new(value: &'a CompoundValue, allow_pairs: bool) -> Self {
        Self { value, allow_pairs }
    }
}

impl serde::Serialize for CompoundValueSerializer<'_> {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::Error;

        match &self.value {
            CompoundValue::Pair(pair) if self.allow_pairs => {
                let mut state = serializer.serialize_map(Some(2))?;
                let left = ValueSerializer::new(pair.left(), self.allow_pairs);
                let right = ValueSerializer::new(pair.right(), self.allow_pairs);
                state.serialize_entry("left", &left)?;
                state.serialize_entry("right", &right)?;
                state.end()
            }
            CompoundValue::Pair(_) => Err(S::Error::custom("a pair cannot be serialized")),
            CompoundValue::Array(v) => {
                let mut s = serializer.serialize_seq(Some(v.len()))?;
                for v in v.as_slice() {
                    s.serialize_element(&ValueSerializer::new(v, self.allow_pairs))?;
                }

                s.end()
            }
            CompoundValue::Map(v) => {
                let ty = v.ty();
                let map_type = ty.as_map().expect("type should be a map");
                if !map_type
                    .key_type()
                    .is_coercible_to(&PrimitiveType::String.into())
                {
                    return Err(S::Error::custom(format!(
                        "cannot serialize a map of type `{ty}` as the key type cannot be coerced \
                         to `String`",
                    )));
                }

                let mut s = serializer.serialize_map(Some(v.len()))?;
                for (k, v) in v.iter() {
                    s.serialize_entry(k, &ValueSerializer::new(v, self.allow_pairs))?;
                }

                s.end()
            }
            CompoundValue::Object(object) => {
                let mut s = serializer.serialize_map(Some(object.len()))?;
                for (k, v) in object.iter() {
                    s.serialize_entry(k, &ValueSerializer::new(v, self.allow_pairs))?;
                }

                s.end()
            }
            CompoundValue::Struct(Struct { members, .. }) => {
                let mut s = serializer.serialize_map(Some(members.len()))?;
                for (k, v) in members.iter() {
                    s.serialize_entry(k, &ValueSerializer::new(v, self.allow_pairs))?;
                }

                s.end()
            }
            CompoundValue::EnumVariant(e) => serializer.serialize_str(e.name()),
        }
    }
}

#[cfg(test)]
mod test {
    use approx::assert_relative_eq;
    use pretty_assertions::assert_eq;
    use wdl_analysis::types::ArrayType;
    use wdl_analysis::types::MapType;
    use wdl_analysis::types::PairType;
    use wdl_analysis::types::StructType;
    use wdl_ast::Diagnostic;
    use wdl_ast::Span;
    use wdl_ast::SupportedVersion;

    use super::*;
    use crate::http::Transferer;
    use crate::path::EvaluationPath;

    #[test]
    fn boolean_coercion() {
        // Boolean -> Boolean
        assert_eq!(
            Value::from(false)
                .coerce(None, &PrimitiveType::Boolean.into())
                .expect("should coerce")
                .unwrap_boolean(),
            Value::from(false).unwrap_boolean()
        );
        // Boolean -> String (invalid)
        assert_eq!(
            format!(
                "{e:?}",
                e = Value::from(true)
                    .coerce(None, &PrimitiveType::String.into())
                    .unwrap_err()
            ),
            "cannot coerce type `Boolean` to type `String`"
        );
    }

    #[test]
    fn boolean_display() {
        assert_eq!(Value::from(false).to_string(), "false");
        assert_eq!(Value::from(true).to_string(), "true");
    }

    #[test]
    fn integer_coercion() {
        // Int -> Int
        assert_eq!(
            Value::from(12345)
                .coerce(None, &PrimitiveType::Integer.into())
                .expect("should coerce")
                .unwrap_integer(),
            Value::from(12345).unwrap_integer()
        );
        // Int -> Float
        assert_relative_eq!(
            Value::from(12345)
                .coerce(None, &PrimitiveType::Float.into())
                .expect("should coerce")
                .unwrap_float(),
            Value::from(12345.0).unwrap_float()
        );
        // Int -> Boolean (invalid)
        assert_eq!(
            format!(
                "{e:?}",
                e = Value::from(12345)
                    .coerce(None, &PrimitiveType::Boolean.into())
                    .unwrap_err()
            ),
            "cannot coerce type `Int` to type `Boolean`"
        );
    }

    #[test]
    fn integer_display() {
        assert_eq!(Value::from(12345).to_string(), "12345");
        assert_eq!(Value::from(-12345).to_string(), "-12345");
    }

    #[test]
    fn float_coercion() {
        // Float -> Float
        assert_relative_eq!(
            Value::from(12345.0)
                .coerce(None, &PrimitiveType::Float.into())
                .expect("should coerce")
                .unwrap_float(),
            Value::from(12345.0).unwrap_float()
        );
        // Float -> Int (invalid)
        assert_eq!(
            format!(
                "{e:?}",
                e = Value::from(12345.0)
                    .coerce(None, &PrimitiveType::Integer.into())
                    .unwrap_err()
            ),
            "cannot coerce type `Float` to type `Int`"
        );
    }

    #[test]
    fn float_display() {
        assert_eq!(Value::from(12345.12345).to_string(), "12345.123450");
        assert_eq!(Value::from(-12345.12345).to_string(), "-12345.123450");
    }

    #[test]
    fn string_coercion() {
        let value = PrimitiveValue::new_string("foo");
        // String -> String
        assert_eq!(
            value
                .coerce(None, &PrimitiveType::String.into())
                .expect("should coerce"),
            value
        );
        // String -> File
        assert_eq!(
            value
                .coerce(None, &PrimitiveType::File.into())
                .expect("should coerce"),
            PrimitiveValue::File(value.as_string().expect("should be string").clone().into())
        );
        // String -> Directory
        assert_eq!(
            value
                .coerce(None, &PrimitiveType::Directory.into())
                .expect("should coerce"),
            PrimitiveValue::Directory(value.as_string().expect("should be string").clone().into())
        );
        // String -> Boolean (invalid)
        assert_eq!(
            format!(
                "{e:?}",
                e = value
                    .coerce(None, &PrimitiveType::Boolean.into())
                    .unwrap_err()
            ),
            "cannot coerce type `String` to type `Boolean`"
        );

        struct Context;

        impl EvaluationContext for Context {
            fn version(&self) -> SupportedVersion {
                unimplemented!()
            }

            fn resolve_name(&self, _: &str, _: Span) -> Result<Value, Diagnostic> {
                unimplemented!()
            }

            fn resolve_type_name(&self, _: &str, _: Span) -> Result<Type, Diagnostic> {
                unimplemented!()
            }

            fn enum_variant_value(&self, _: &str, _: &str) -> Result<Value, Diagnostic> {
                unimplemented!()
            }

            fn base_dir(&self) -> &EvaluationPath {
                unimplemented!()
            }

            fn temp_dir(&self) -> &Path {
                unimplemented!()
            }

            fn transferer(&self) -> &dyn Transferer {
                unimplemented!()
            }

            fn host_path(&self, path: &GuestPath) -> Option<HostPath> {
                if path.as_str() == "/mnt/task/input/0/path" {
                    Some(HostPath::new("/some/host/path"))
                } else {
                    None
                }
            }
        }

        // String (guest path) -> File
        assert_eq!(
            PrimitiveValue::new_string("/mnt/task/input/0/path")
                .coerce(Some(&Context), &PrimitiveType::File.into())
                .expect("should coerce")
                .unwrap_file()
                .as_str(),
            "/some/host/path"
        );

        // String (not a guest path) -> File
        assert_eq!(
            value
                .coerce(Some(&Context), &PrimitiveType::File.into())
                .expect("should coerce")
                .unwrap_file()
                .as_str(),
            "foo"
        );

        // String (guest path) -> Directory
        assert_eq!(
            PrimitiveValue::new_string("/mnt/task/input/0/path")
                .coerce(Some(&Context), &PrimitiveType::Directory.into())
                .expect("should coerce")
                .unwrap_directory()
                .as_str(),
            "/some/host/path"
        );

        // String (not a guest path) -> Directory
        assert_eq!(
            value
                .coerce(Some(&Context), &PrimitiveType::Directory.into())
                .expect("should coerce")
                .unwrap_directory()
                .as_str(),
            "foo"
        );
    }

    #[test]
    fn string_display() {
        let value = PrimitiveValue::new_string("hello world!");
        assert_eq!(value.to_string(), "\"hello world!\"");
    }

    #[test]
    fn file_coercion() {
        let value = PrimitiveValue::new_file("foo");

        // File -> File
        assert_eq!(
            value
                .coerce(None, &PrimitiveType::File.into())
                .expect("should coerce"),
            value
        );
        // File -> String
        assert_eq!(
            value
                .coerce(None, &PrimitiveType::String.into())
                .expect("should coerce"),
            PrimitiveValue::String(value.as_file().expect("should be file").0.clone())
        );
        // File -> Directory (invalid)
        assert_eq!(
            format!(
                "{e:?}",
                e = value
                    .coerce(None, &PrimitiveType::Directory.into())
                    .unwrap_err()
            ),
            "cannot coerce type `File` to type `Directory`"
        );

        struct Context;

        impl EvaluationContext for Context {
            fn version(&self) -> SupportedVersion {
                unimplemented!()
            }

            fn resolve_name(&self, _: &str, _: Span) -> Result<Value, Diagnostic> {
                unimplemented!()
            }

            fn resolve_type_name(&self, _: &str, _: Span) -> Result<Type, Diagnostic> {
                unimplemented!()
            }

            fn enum_variant_value(&self, _: &str, _: &str) -> Result<Value, Diagnostic> {
                unimplemented!()
            }

            fn base_dir(&self) -> &EvaluationPath {
                unimplemented!()
            }

            fn temp_dir(&self) -> &Path {
                unimplemented!()
            }

            fn transferer(&self) -> &dyn Transferer {
                unimplemented!()
            }

            fn guest_path(&self, path: &HostPath) -> Option<GuestPath> {
                if path.as_str() == "/some/host/path" {
                    Some(GuestPath::new("/mnt/task/input/0/path"))
                } else {
                    None
                }
            }
        }

        // File (mapped) -> String
        assert_eq!(
            PrimitiveValue::new_file("/some/host/path")
                .coerce(Some(&Context), &PrimitiveType::String.into())
                .expect("should coerce")
                .unwrap_string()
                .as_str(),
            "/mnt/task/input/0/path"
        );

        // File (not mapped) -> String
        assert_eq!(
            value
                .coerce(Some(&Context), &PrimitiveType::String.into())
                .expect("should coerce")
                .unwrap_string()
                .as_str(),
            "foo"
        );
    }

    #[test]
    fn file_display() {
        let value = PrimitiveValue::new_file("/foo/bar/baz.txt");
        assert_eq!(value.to_string(), "\"/foo/bar/baz.txt\"");
    }

    #[test]
    fn directory_coercion() {
        let value = PrimitiveValue::new_directory("foo");

        // Directory -> Directory
        assert_eq!(
            value
                .coerce(None, &PrimitiveType::Directory.into())
                .expect("should coerce"),
            value
        );
        // Directory -> String
        assert_eq!(
            value
                .coerce(None, &PrimitiveType::String.into())
                .expect("should coerce"),
            PrimitiveValue::String(value.as_directory().expect("should be directory").0.clone())
        );
        // Directory -> File (invalid)
        assert_eq!(
            format!(
                "{e:?}",
                e = value.coerce(None, &PrimitiveType::File.into()).unwrap_err()
            ),
            "cannot coerce type `Directory` to type `File`"
        );

        struct Context;

        impl EvaluationContext for Context {
            fn version(&self) -> SupportedVersion {
                unimplemented!()
            }

            fn resolve_name(&self, _: &str, _: Span) -> Result<Value, Diagnostic> {
                unimplemented!()
            }

            fn resolve_type_name(&self, _: &str, _: Span) -> Result<Type, Diagnostic> {
                unimplemented!()
            }

            fn enum_variant_value(&self, _: &str, _: &str) -> Result<Value, Diagnostic> {
                unimplemented!()
            }

            fn base_dir(&self) -> &EvaluationPath {
                unimplemented!()
            }

            fn temp_dir(&self) -> &Path {
                unimplemented!()
            }

            fn transferer(&self) -> &dyn Transferer {
                unimplemented!()
            }

            fn guest_path(&self, path: &HostPath) -> Option<GuestPath> {
                if path.as_str() == "/some/host/path" {
                    Some(GuestPath::new("/mnt/task/input/0/path"))
                } else {
                    None
                }
            }
        }

        // Directory (mapped) -> String
        assert_eq!(
            PrimitiveValue::new_directory("/some/host/path")
                .coerce(Some(&Context), &PrimitiveType::String.into())
                .expect("should coerce")
                .unwrap_string()
                .as_str(),
            "/mnt/task/input/0/path"
        );

        // Directory (not mapped) -> String
        assert_eq!(
            value
                .coerce(Some(&Context), &PrimitiveType::String.into())
                .expect("should coerce")
                .unwrap_string()
                .as_str(),
            "foo"
        );
    }

    #[test]
    fn directory_display() {
        let value = PrimitiveValue::new_directory("/foo/bar/baz");
        assert_eq!(value.to_string(), "\"/foo/bar/baz\"");
    }

    #[test]
    fn none_coercion() {
        // None -> String?
        assert!(
            Value::new_none(Type::None)
                .coerce(None, &Type::from(PrimitiveType::String).optional())
                .expect("should coerce")
                .is_none(),
        );

        // None -> String (invalid)
        assert_eq!(
            format!(
                "{e:?}",
                e = Value::new_none(Type::None)
                    .coerce(None, &PrimitiveType::String.into())
                    .unwrap_err()
            ),
            "cannot coerce `None` to non-optional type `String`"
        );
    }

    #[test]
    fn none_display() {
        assert_eq!(Value::new_none(Type::None).to_string(), "None");
    }

    #[test]
    fn array_coercion() {
        let src_ty: Type = ArrayType::new(PrimitiveType::Integer).into();
        let target_ty: Type = ArrayType::new(PrimitiveType::Float).into();

        // Array[Int] -> Array[Float]
        let src: CompoundValue = Array::new(None, src_ty, [1, 2, 3])
            .expect("should create array value")
            .into();
        let target = src.coerce(None, &target_ty).expect("should coerce");
        assert_eq!(
            target.unwrap_array().to_string(),
            "[1.000000, 2.000000, 3.000000]"
        );

        // Array[Int] -> Array[String] (invalid)
        let target_ty: Type = ArrayType::new(PrimitiveType::String).into();
        assert_eq!(
            format!("{e:?}", e = src.coerce(None, &target_ty).unwrap_err()),
            r#"failed to coerce array element at index 0

Caused by:
    cannot coerce type `Int` to type `String`"#
        );
    }

    #[test]
    fn non_empty_array_coercion() {
        let ty: Type = ArrayType::new(PrimitiveType::String).into();
        let target_ty: Type = ArrayType::non_empty(PrimitiveType::String).into();

        // Array[String] (non-empty) -> Array[String]+
        let string = PrimitiveValue::new_string("foo");
        let value: Value = Array::new(None, ty.clone(), [string])
            .expect("should create array")
            .into();
        assert!(value.coerce(None, &target_ty).is_ok(), "should coerce");

        // Array[String] (empty) -> Array[String]+ (invalid)
        let value: Value = Array::new::<Value>(None, ty, [])
            .expect("should create array")
            .into();
        assert_eq!(
            format!("{e:?}", e = value.coerce(None, &target_ty).unwrap_err()),
            "cannot coerce empty array value to non-empty array type `Array[String]+`"
        );
    }

    #[test]
    fn array_display() {
        let ty: Type = ArrayType::new(PrimitiveType::Integer).into();
        let value: Value = Array::new(None, ty, [1, 2, 3])
            .expect("should create array")
            .into();

        assert_eq!(value.to_string(), "[1, 2, 3]");
    }

    #[test]
    fn map_coerce() {
        let key1 = PrimitiveValue::new_file("foo");
        let value1 = PrimitiveValue::new_string("bar");
        let key2 = PrimitiveValue::new_file("baz");
        let value2 = PrimitiveValue::new_string("qux");

        let ty = MapType::new(PrimitiveType::File, PrimitiveType::String);
        let file_to_string: Value = Map::new(None, ty, [(key1, value1), (key2, value2)])
            .expect("should create map value")
            .into();

        // Map[File, String] -> Map[String, File]
        let ty = MapType::new(PrimitiveType::String, PrimitiveType::File).into();
        let string_to_file = file_to_string
            .coerce(None, &ty)
            .expect("value should coerce");
        assert_eq!(
            string_to_file.to_string(),
            r#"{"foo": "bar", "baz": "qux"}"#
        );

        // Map[String, File] -> Map[Int, File] (invalid)
        let ty = MapType::new(PrimitiveType::Integer, PrimitiveType::File).into();
        assert_eq!(
            format!("{e:?}", e = string_to_file.coerce(None, &ty).unwrap_err()),
            r#"failed to coerce map key for element at index 0

Caused by:
    cannot coerce type `String` to type `Int`"#
        );

        // Map[String, File] -> Map[String, Int] (invalid)
        let ty = MapType::new(PrimitiveType::String, PrimitiveType::Integer).into();
        assert_eq!(
            format!("{e:?}", e = string_to_file.coerce(None, &ty).unwrap_err()),
            r#"failed to coerce map value for element at index 0

Caused by:
    cannot coerce type `File` to type `Int`"#
        );

        // Map[String, File] -> Struct
        let ty = StructType::new(
            "Foo",
            [("foo", PrimitiveType::File), ("baz", PrimitiveType::File)],
        )
        .into();
        let struct_value = string_to_file
            .coerce(None, &ty)
            .expect("value should coerce");
        assert_eq!(struct_value.to_string(), r#"Foo {foo: "bar", baz: "qux"}"#);

        // Map[File, String] -> Struct
        let ty = StructType::new(
            "Foo",
            [
                ("foo", PrimitiveType::String),
                ("baz", PrimitiveType::String),
            ],
        )
        .into();
        let struct_value = file_to_string
            .coerce(None, &ty)
            .expect("value should coerce");
        assert_eq!(struct_value.to_string(), r#"Foo {foo: "bar", baz: "qux"}"#);

        // Map[String, File] -> Struct (invalid)
        let ty = StructType::new(
            "Foo",
            [
                ("foo", PrimitiveType::File),
                ("baz", PrimitiveType::File),
                ("qux", PrimitiveType::File),
            ],
        )
        .into();
        assert_eq!(
            format!("{e:?}", e = string_to_file.coerce(None, &ty).unwrap_err()),
            "cannot coerce a map of 2 elements to struct type `Foo` as the struct has 3 members"
        );

        // Map[String, File] -> Object
        let object_value = string_to_file
            .coerce(None, &Type::Object)
            .expect("value should coerce");
        assert_eq!(
            object_value.to_string(),
            r#"object {foo: "bar", baz: "qux"}"#
        );

        // Map[File, String] -> Object
        let object_value = file_to_string
            .coerce(None, &Type::Object)
            .expect("value should coerce");
        assert_eq!(
            object_value.to_string(),
            r#"object {foo: "bar", baz: "qux"}"#
        );
    }

    #[test]
    fn map_display() {
        let ty = MapType::new(PrimitiveType::Integer, PrimitiveType::Boolean);
        let value: Value = Map::new(None, ty, [(1, true), (2, false)])
            .expect("should create map value")
            .into();
        assert_eq!(value.to_string(), "{1: true, 2: false}");
    }

    #[test]
    fn pair_coercion() {
        let left = PrimitiveValue::new_file("foo");
        let right = PrimitiveValue::new_string("bar");

        let ty = PairType::new(PrimitiveType::File, PrimitiveType::String);
        let value: Value = Pair::new(None, ty, left, right)
            .expect("should create pair value")
            .into();

        // Pair[File, String] -> Pair[String, File]
        let ty = PairType::new(PrimitiveType::String, PrimitiveType::File).into();
        let value = value.coerce(None, &ty).expect("value should coerce");
        assert_eq!(value.to_string(), r#"("foo", "bar")"#);

        // Pair[String, File] -> Pair[Int, Int]
        let ty = PairType::new(PrimitiveType::Integer, PrimitiveType::Integer).into();
        assert_eq!(
            format!("{e:?}", e = value.coerce(None, &ty).unwrap_err()),
            r#"failed to coerce pair's left value

Caused by:
    cannot coerce type `String` to type `Int`"#
        );
    }

    #[test]
    fn pair_display() {
        let ty = PairType::new(PrimitiveType::Integer, PrimitiveType::Boolean);
        let value: Value = Pair::new(None, ty, 12345, false)
            .expect("should create pair value")
            .into();
        assert_eq!(value.to_string(), "(12345, false)");
    }

    #[test]
    fn struct_coercion() {
        let ty = StructType::new(
            "Foo",
            [
                ("foo", PrimitiveType::Float),
                ("bar", PrimitiveType::Float),
                ("baz", PrimitiveType::Float),
            ],
        );
        let value: Value = Struct::new(None, ty, [("foo", 1.0), ("bar", 2.0), ("baz", 3.0)])
            .expect("should create map value")
            .into();

        // Struct -> Map[String, Float]
        let ty = MapType::new(PrimitiveType::String, PrimitiveType::Float).into();
        let map_value = value.coerce(None, &ty).expect("value should coerce");
        assert_eq!(
            map_value.to_string(),
            r#"{"foo": 1.000000, "bar": 2.000000, "baz": 3.000000}"#
        );

        // Struct -> Map[File, Float]
        let ty = MapType::new(PrimitiveType::File, PrimitiveType::Float).into();
        let map_value = value.coerce(None, &ty).expect("value should coerce");
        assert_eq!(
            map_value.to_string(),
            r#"{"foo": 1.000000, "bar": 2.000000, "baz": 3.000000}"#
        );

        // Struct -> Struct
        let ty = StructType::new(
            "Bar",
            [
                ("foo", PrimitiveType::Float),
                ("bar", PrimitiveType::Float),
                ("baz", PrimitiveType::Float),
            ],
        )
        .into();
        let struct_value = value.coerce(None, &ty).expect("value should coerce");
        assert_eq!(
            struct_value.to_string(),
            r#"Bar {foo: 1.000000, bar: 2.000000, baz: 3.000000}"#
        );

        // Struct -> Object
        let object_value = value
            .coerce(None, &Type::Object)
            .expect("value should coerce");
        assert_eq!(
            object_value.to_string(),
            r#"object {foo: 1.000000, bar: 2.000000, baz: 3.000000}"#
        );
    }

    #[test]
    fn struct_display() {
        let ty = StructType::new(
            "Foo",
            [
                ("foo", PrimitiveType::Float),
                ("bar", PrimitiveType::String),
                ("baz", PrimitiveType::Integer),
            ],
        );
        let value: Value = Struct::new(
            None,
            ty,
            [
                ("foo", Value::from(1.101)),
                ("bar", PrimitiveValue::new_string("foo").into()),
                ("baz", 1234.into()),
            ],
        )
        .expect("should create map value")
        .into();
        assert_eq!(
            value.to_string(),
            r#"Foo {foo: 1.101000, bar: "foo", baz: 1234}"#
        );
    }

    #[test]
    fn pair_serialization() {
        let pair_ty = PairType::new(PrimitiveType::File, PrimitiveType::String);
        let pair: Value = Pair::new(
            None,
            pair_ty,
            PrimitiveValue::new_file("foo"),
            PrimitiveValue::new_string("bar"),
        )
        .expect("should create pair value")
        .into();
        // Serialize pair with `left` and `right` keys
        let value_serializer = ValueSerializer::new(&pair, true);
        let serialized = serde_json::to_string(&value_serializer).expect("should serialize");
        assert_eq!(serialized, r#"{"left":"foo","right":"bar"}"#);

        // Serialize pair without `left` and `right` keys (should fail)
        let value_serializer = ValueSerializer::new(&pair, false);
        assert!(serde_json::to_string(&value_serializer).is_err());

        let array_ty = ArrayType::new(PairType::new(PrimitiveType::File, PrimitiveType::String));
        let array: Value = Array::new(None, array_ty, [pair])
            .expect("should create array value")
            .into();

        // Serialize array of pairs with `left` and `right` keys
        let value_serializer = ValueSerializer::new(&array, true);
        let serialized = serde_json::to_string(&value_serializer).expect("should serialize");
        assert_eq!(serialized, r#"[{"left":"foo","right":"bar"}]"#);
    }

    #[test]
    fn type_name_ref_equality() {
        use std::sync::Arc;

        use wdl_analysis::types::EnumType;

        let enum_type = Type::Compound(
            CompoundType::Custom(CustomType::Enum(Arc::new(
                EnumType::new(
                    "MyEnum",
                    Span::new(0, 0),
                    Type::Primitive(PrimitiveType::Integer, false),
                    Vec::<(String, Type)>::new(),
                    &[],
                )
                .expect("should create enum type"),
            ))),
            false,
        );

        let value1 = Value::TypeNameRef(enum_type.clone());
        let value2 = Value::TypeNameRef(enum_type.clone());

        assert_eq!(value1.ty(), value2.ty());
    }

    #[test]
    fn type_name_ref_ty() {
        use std::sync::Arc;

        let struct_type = Type::Compound(
            CompoundType::Custom(CustomType::Struct(Arc::new(StructType::new(
                "MyStruct",
                Vec::<(&str, Type)>::new(),
            )))),
            false,
        );

        let value = Value::TypeNameRef(struct_type.clone());
        assert_eq!(value.ty(), struct_type);
    }

    #[test]
    fn type_name_ref_display() {
        use std::sync::Arc;

        use wdl_analysis::types::EnumType;

        let enum_type = Type::Compound(
            CompoundType::Custom(CustomType::Enum(Arc::new(
                EnumType::new(
                    "Color",
                    Span::new(0, 0),
                    Type::Primitive(PrimitiveType::Integer, false),
                    Vec::<(String, Type)>::new(),
                    &[],
                )
                .expect("should create enum type"),
            ))),
            false,
        );

        let value = Value::TypeNameRef(enum_type);
        assert_eq!(value.to_string(), "Color");
    }
}
