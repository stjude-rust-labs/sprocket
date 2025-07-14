//! Implementation of the WDL runtime and values.

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
use wdl_analysis::types::Optional;
use wdl_analysis::types::PrimitiveType;
use wdl_analysis::types::Type;
use wdl_ast::AstToken;
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
use wdl_ast::v1::TASK_FIELD_MEMORY;
use wdl_ast::v1::TASK_FIELD_META;
use wdl_ast::v1::TASK_FIELD_NAME;
use wdl_ast::v1::TASK_FIELD_PARAMETER_META;
use wdl_ast::v1::TASK_FIELD_RETURN_CODE;
use wdl_grammar::lexer::v1::is_ident;

use crate::EvaluationContext;
use crate::Outputs;
use crate::TaskExecutionConstraints;
use crate::path;

/// Implemented on coercible values.
pub trait Coercible: Sized {
    /// Coerces the value into the given type.
    ///
    /// Returns an error if the coercion is not supported.
    fn coerce(&self, target: &Type) -> Result<Self>;
}

/// Represents a WDL runtime value.
///
/// Values are cheap to clone.
#[derive(Debug, Clone)]
pub enum Value {
    /// The value is a literal `None` value.
    None,
    /// The value is a primitive value.
    Primitive(PrimitiveValue),
    /// The value is a compound value.
    Compound(CompoundValue),
    /// The value is a task variable.
    ///
    /// This value occurs only during command and output section evaluation in
    /// WDL 1.2 tasks.
    Task(TaskValue),
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
    /// The value is the outputs of a call.
    Call(CallValue),
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
            v1::MetadataValue::Null(_) => Self::None,
            v1::MetadataValue::Object(o) => Object::from_v1_metadata(o.items()).into(),
            v1::MetadataValue::Array(a) => Array::new_unchecked(
                ANALYSIS_STDLIB.array_object_type().clone(),
                a.elements().map(|v| Value::from_v1_metadata(&v)).collect(),
            )
            .into(),
        }
    }

    /// Gets the type of the value.
    pub fn ty(&self) -> Type {
        match self {
            Self::None => Type::None,
            Self::Primitive(v) => v.ty(),
            Self::Compound(v) => v.ty(),
            Self::Task(_) => Type::Task,
            Self::Hints(_) => Type::Hints,
            Self::Input(_) => Type::Input,
            Self::Output(_) => Type::Output,
            Self::Call(v) => Type::Call(v.ty.clone()),
        }
    }

    /// Determines if the value is `None`.
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
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
    pub fn as_file(&self) -> Option<&Arc<String>> {
        match self {
            Self::Primitive(PrimitiveValue::File(s)) => Some(s),
            _ => None,
        }
    }

    /// Unwraps the value into a `File`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `File`.
    pub fn unwrap_file(self) -> Arc<String> {
        match self {
            Self::Primitive(PrimitiveValue::File(s)) => s,
            _ => panic!("value is not a file"),
        }
    }

    /// Gets the value as a `Directory`.
    ///
    /// Returns `None` if the value is not a `Directory`.
    pub fn as_directory(&self) -> Option<&Arc<String>> {
        match self {
            Self::Primitive(PrimitiveValue::Directory(s)) => Some(s),
            _ => None,
        }
    }

    /// Unwraps the value into a `Directory`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `Directory`.
    pub fn unwrap_directory(self) -> Arc<String> {
        match self {
            Self::Primitive(PrimitiveValue::Directory(s)) => s,
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

    /// Gets the value as a task.
    ///
    /// Returns `None` if the value is not a task.
    pub fn as_task(&self) -> Option<&TaskValue> {
        match self {
            Self::Task(v) => Some(v),
            _ => None,
        }
    }

    /// Gets a mutable reference to the value as a task.
    ///
    /// Returns `None` if the value is not a task.
    pub(crate) fn as_task_mut(&mut self) -> Option<&mut TaskValue> {
        match self {
            Self::Task(v) => Some(v),
            _ => None,
        }
    }

    /// Unwraps the value into a task.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a task.
    pub fn unwrap_task(self) -> TaskValue {
        match self {
            Self::Task(v) => v,
            _ => panic!("value is not a task"),
        }
    }

    /// Gets the value as a hints value.
    ///
    /// Returns `None` if the value is not a hints value.
    pub fn as_hints(&self) -> Option<&HintsValue> {
        match self {
            Self::Hints(v) => Some(v),
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
            Self::Hints(v) => v,
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

    /// Visits each `File` or `Directory` value contained in this value.
    ///
    /// Note that paths may be specified as URLs.
    pub(crate) fn visit_paths(
        &self,
        optional: bool,
        cb: &mut impl FnMut(bool, &PrimitiveValue) -> Result<()>,
    ) -> Result<()> {
        match self {
            Self::Primitive(v) => v.visit_paths(optional, cb),
            Self::Compound(v) => v.visit_paths(cb),
            _ => Ok(()),
        }
    }

    /// Mutably visits each `File` or `Directory` value contained in this value.
    ///
    /// If the provided callback returns `Ok(false)`, the `File` or `Directory`
    /// value will be replaced with `None`.
    ///
    /// Note that paths may be specified as URLs.
    pub(crate) fn visit_paths_mut(
        &mut self,
        optional: bool,
        cb: &mut impl FnMut(bool, &mut PrimitiveValue) -> Result<bool>,
    ) -> Result<()> {
        match self {
            Self::Primitive(v) => {
                if !v.visit_paths_mut(optional, cb)? {
                    *self = Value::None;
                }

                Ok(())
            }
            Self::Compound(v) => v.visit_paths_mut(cb),
            _ => Ok(()),
        }
    }

    /// Determines if two values have equality according to the WDL
    /// specification.
    ///
    /// Returns `None` if the two values cannot be compared for equality.
    pub fn equals(left: &Self, right: &Self) -> Option<bool> {
        match (left, right) {
            (Value::None, Value::None) => Some(true),
            (Value::None, _) | (_, Value::None) => Some(false),
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
            Self::None => write!(f, "None"),
            Self::Primitive(v) => v.fmt(f),
            Self::Compound(v) => v.fmt(f),
            Self::Task(_) => write!(f, "task"),
            Self::Hints(v) => v.fmt(f),
            Self::Input(v) => v.fmt(f),
            Self::Output(v) => v.fmt(f),
            Self::Call(c) => c.fmt(f),
        }
    }
}

impl Coercible for Value {
    fn coerce(&self, target: &Type) -> Result<Self> {
        if target.is_union() || target.is_none() || self.ty().eq(target) {
            return Ok(self.clone());
        }

        match self {
            Self::None => {
                if target.is_optional() {
                    Ok(Self::None)
                } else {
                    bail!("cannot coerce `None` to non-optional type `{target}`");
                }
            }
            Self::Primitive(v) => v.coerce(target).map(Self::Primitive),
            Self::Compound(v) => v.coerce(target).map(Self::Compound),
            Self::Task(_) => {
                if matches!(target, Type::Task) {
                    return Ok(self.clone());
                }

                bail!("task variables cannot be coerced to any other type");
            }
            Self::Hints(_) => {
                if matches!(target, Type::Hints) {
                    return Ok(self.clone());
                }

                bail!("hints values cannot be coerced to any other type");
            }
            Self::Input(_) => {
                if matches!(target, Type::Input) {
                    return Ok(self.clone());
                }

                bail!("input values cannot be coerced to any other type");
            }
            Self::Output(_) => {
                if matches!(target, Type::Output) {
                    return Ok(self.clone());
                }

                bail!("output values cannot be coerced to any other type");
            }
            Self::Call(_) => {
                bail!("call values cannot be coerced to any other type");
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
            None => Self::None,
        }
    }
}

impl From<CompoundValue> for Value {
    fn from(value: CompoundValue) -> Self {
        Self::Compound(value)
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

impl From<TaskValue> for Value {
    fn from(value: TaskValue) -> Self {
        Self::Task(value)
    }
}

impl From<HintsValue> for Value {
    fn from(value: HintsValue) -> Self {
        Self::Hints(value)
    }
}

impl From<CallValue> for Value {
    fn from(value: CallValue) -> Self {
        Self::Call(value)
    }
}

impl serde::Serialize for Value {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::Error;

        match self {
            Self::None => serializer.serialize_none(),
            Self::Primitive(v) => v.serialize(serializer),
            Self::Compound(v) => v.serialize(serializer),
            Self::Task(_) | Self::Hints(_) | Self::Input(_) | Self::Output(_) | Self::Call(_) => {
                Err(S::Error::custom("value cannot be serialized"))
            }
        }
    }
}

impl<'de> serde::Deserialize<'de> for Value {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::Deserialize as _;

        /// Helper for deserializing the elements of sequences and maps
        struct Deserialize;

        impl<'de> serde::de::DeserializeSeed<'de> for Deserialize {
            type Value = Value;

            fn deserialize<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                deserializer.deserialize_any(Visitor)
            }
        }

        /// Visitor for deserialization.
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Value;

            fn visit_unit<E>(self) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Value::None)
            }

            fn visit_none<E>(self) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Value::None)
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
                use serde::de::Error;

                let mut elements = Vec::new();
                while let Some(v) = seq.next_element_seed(Deserialize)? {
                    elements.push(v);
                }

                let element_ty = elements
                    .iter()
                    .try_fold(None, |mut ty, element| {
                        let element_ty = element.ty();
                        let ty = ty.get_or_insert(element_ty.clone());
                        ty.common_type(&element_ty).map(Some).ok_or_else(|| {
                            A::Error::custom(format!(
                                "a common element type does not exist between `{ty}` and \
                                 `{element_ty}`"
                            ))
                        })
                    })?
                    .unwrap_or(Type::Union);

                let ty: Type = ArrayType::new(element_ty).into();
                Ok(Array::new(ty.clone(), elements)
                    .map_err(|e| A::Error::custom(format!("cannot coerce value to `{ty}`: {e:#}")))?
                    .into())
            }

            fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                use serde::de::Error;

                let mut members = IndexMap::new();
                while let Some(key) = map.next_key::<String>()? {
                    if !is_ident(&key) {
                        return Err(A::Error::custom(format!(
                            "object key `{key}` is not a valid WDL identifier"
                        )));
                    }

                    members.insert(key, map.next_value_seed(Deserialize)?);
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
    File(Arc<String>),
    /// The value is a `Directory`.
    Directory(Arc<String>),
}

impl PrimitiveValue {
    /// Creates a new `String` value.
    pub fn new_string(s: impl Into<String>) -> Self {
        Self::String(Arc::new(s.into()))
    }

    /// Creates a new `File` value.
    pub fn new_file(s: impl Into<String>) -> Self {
        Self::File(Arc::new(s.into()))
    }

    /// Creates a new `Directory` value.
    pub fn new_directory(s: impl Into<String>) -> Self {
        Self::Directory(Arc::new(s.into()))
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
    pub fn as_file(&self) -> Option<&Arc<String>> {
        match self {
            Self::File(s) => Some(s),
            _ => None,
        }
    }

    /// Unwraps the value into a `File`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `File`.
    pub fn unwrap_file(self) -> Arc<String> {
        match self {
            Self::File(s) => s,
            _ => panic!("value is not a file"),
        }
    }

    /// Gets the value as a `Directory`.
    ///
    /// Returns `None` if the value is not a `Directory`.
    pub fn as_directory(&self) -> Option<&Arc<String>> {
        match self {
            Self::Directory(s) => Some(s),
            _ => None,
        }
    }

    /// Unwraps the value into a `Directory`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `Directory`.
    pub fn unwrap_directory(self) -> Arc<String> {
        match self {
            Self::Directory(s) => s,
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
            | (Self::String(left), Self::File(right))
            | (Self::String(left), Self::Directory(right))
            | (Self::File(left), Self::File(right))
            | (Self::File(left), Self::String(right))
            | (Self::Directory(left), Self::Directory(right))
            | (Self::Directory(left), Self::String(right)) => Some(left.cmp(right)),
            _ => None,
        }
    }

    /// Gets a raw display of the value.
    ///
    /// This differs from the [Display][fmt::Display] implementation in that
    /// strings, files, and directories are not quoted and not escaped.
    ///
    /// If an evaluation context is provided, path translation is attempted.
    pub fn raw<'a>(
        &'a self,
        context: Option<&'a dyn EvaluationContext>,
    ) -> impl fmt::Display + use<'a> {
        /// Helper for displaying a raw value.
        struct Display<'a> {
            /// The associated evaluation context.
            context: Option<&'a dyn EvaluationContext>,
            /// The value to display.
            value: &'a PrimitiveValue,
        }

        impl fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self.value {
                    PrimitiveValue::Boolean(v) => write!(f, "{v}"),
                    PrimitiveValue::Integer(v) => write!(f, "{v}"),
                    PrimitiveValue::Float(v) => write!(f, "{v:.6?}"),
                    PrimitiveValue::String(v)
                    | PrimitiveValue::File(v)
                    | PrimitiveValue::Directory(v) => {
                        match self.context.and_then(|c| c.translate_path(v)) {
                            Some(path) => write!(f, "{path}", path = path.display()),
                            None => {
                                write!(f, "{v}")
                            }
                        }
                    }
                }
            }
        }

        Display {
            context,
            value: self,
        }
    }

    /// Visits each `File` or `Directory` value contained in this value.
    ///
    /// Note that paths may be specified as URLs.
    fn visit_paths(
        &self,
        optional: bool,
        cb: &mut impl FnMut(bool, &PrimitiveValue) -> Result<()>,
    ) -> Result<()> {
        match self {
            Self::File(_) | Self::Directory(_) => cb(optional, self),
            _ => Ok(()),
        }
    }

    /// Mutably visits each `File` or `Directory` value contained in this value.
    ///
    /// If the provided callback returns `Ok(false)`, this `File` or `Directory`
    /// value will be replaced with `None`.
    ///
    /// Note that paths may be specified as URLs.
    fn visit_paths_mut(
        &mut self,
        optional: bool,
        cb: &mut impl FnMut(bool, &mut PrimitiveValue) -> Result<bool>,
    ) -> Result<bool> {
        match self {
            Self::File(_) | Self::Directory(_) => cb(optional, self),
            _ => Ok(true),
        }
    }

    /// Performs expansions for file and directory paths.
    pub(crate) fn expand_path(&mut self) -> Result<()> {
        let path = match self {
            PrimitiveValue::File(path) => path,
            PrimitiveValue::Directory(path) => path,
            _ => unreachable!("only file and directory values can be expanded"),
        };

        let result = shellexpand::full(path.as_str())
            .context("expanding file/directory path using shell rules")?;
        *Arc::make_mut(path) = result.to_string();

        Ok(())
    }

    /// Joins this path to the given path.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `File` or `Directory`.
    pub(crate) fn join_path_to(&mut self, to: &Path) {
        let path = match self {
            PrimitiveValue::File(path) => path,
            PrimitiveValue::Directory(path) => path,
            _ => unreachable!("only file and directory values can be joined to a path"),
        };

        // Don't join URLs
        if path::is_url(path) {
            return;
        }

        // Perform the join
        if let Ok(s) = to.join(path.as_str()).into_os_string().into_string() {
            *Arc::make_mut(path) = s;
        }
    }

    /// Ensures a path value exists on disk.
    ///
    /// Returns `Ok(true)` if the path exists.
    ///
    /// Returns `Ok(false)` if the the path does not exist and the type was
    /// optional.
    ///
    /// Otherwise, returns an error if the path does not exist.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `File` or `Directory`.
    pub(crate) fn ensure_path_exists(&self, optional: bool) -> Result<bool> {
        let (path, is_file) = match self {
            PrimitiveValue::File(path) => (path, true),
            PrimitiveValue::Directory(path) => (path, false),
            _ => unreachable!("only file and directory values should be passed to the callback"),
        };

        // If it's a file URL, check that the file exists
        if path::is_file_url(path) {
            let exists = path::parse_url(path)
                .and_then(|url| url.to_file_path().ok())
                .map(|p| p.exists())
                .unwrap_or(false);
            if exists {
                return Ok(true);
            }

            if optional && !exists {
                return Ok(false);
            }

            bail!("path `{path}` does not exist");
        } else if path::is_url(path) {
            // Treat other URLs as they exist
            return Ok(true);
        }

        // Check for existence
        let path = Path::new(path.as_str());
        if is_file && !path.is_file() {
            if optional {
                return Ok(false);
            }

            bail!("file `{path}` does not exist", path = path.display());
        } else if !is_file && !path.is_dir() {
            if optional {
                return Ok(false);
            }

            bail!("directory `{path}` does not exist", path = path.display())
        }

        Ok(true)
    }
}

impl fmt::Display for PrimitiveValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Boolean(v) => write!(f, "{v}"),
            Self::Integer(v) => write!(f, "{v}"),
            Self::Float(v) => write!(f, "{v:.6?}"),
            Self::String(s) | Self::File(s) | Self::Directory(s) => {
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
            Self::String(v) | Self::File(v) | Self::Directory(v) => {
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
    fn coerce(&self, target: &Type) -> Result<Self> {
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
                        PrimitiveType::File => Some(Self::File(s.clone())),
                        // String -> Directory
                        PrimitiveType::Directory => Some(Self::Directory(s.clone())),
                        _ => None,
                    })
                    .with_context(|| format!("cannot coerce type `String` to type `{target}`"))
            }
            Self::File(s) => {
                target
                    .as_primitive()
                    .and_then(|ty| match ty {
                        // File -> File
                        PrimitiveType::File => Some(Self::File(s.clone())),
                        // File -> String
                        PrimitiveType::String => Some(Self::String(s.clone())),
                        _ => None,
                    })
                    .with_context(|| format!("cannot coerce type `File` to type `{target}`"))
            }
            Self::Directory(s) => {
                target
                    .as_primitive()
                    .and_then(|ty| match ty {
                        // Directory -> Directory
                        PrimitiveType::Directory => Some(Self::Directory(s.clone())),
                        // Directory -> String
                        PrimitiveType::String => Some(Self::String(s.clone())),
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
            Self::String(s) | Self::File(s) | Self::Directory(s) => s.serialize(serializer),
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
        ty: impl Into<Type>,
        left: impl Into<Value>,
        right: impl Into<Value>,
    ) -> Result<Self> {
        let ty = ty.into();
        if let Type::Compound(CompoundType::Pair(ty), _) = ty {
            let left = left
                .into()
                .coerce(ty.left_type())
                .context("failed to coerce pair's left value")?;
            let right = right
                .into()
                .coerce(ty.right_type())
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
    pub fn new<V>(ty: impl Into<Type>, elements: impl IntoIterator<Item = V>) -> Result<Self>
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
                    v.coerce(element_type)
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
                            match k.coerce(key_type).with_context(|| {
                                format!("failed to coerce map key for element at index {i}")
                            })? {
                                Value::None => None,
                                Value::Primitive(v) => Some(v),
                                _ => {
                                    bail!("not all key values are primitive")
                                }
                            }
                        },
                        v.coerce(value_type).with_context(|| {
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
    pub fn iter(&self) -> impl Iterator<Item = (&Option<PrimitiveValue>, &Value)> {
        self.elements
            .as_ref()
            .map(|m| Either::Left(m.iter()))
            .unwrap_or(Either::Right(std::iter::empty()))
    }

    /// Iterates the keys of the map.
    pub fn keys(&self) -> impl Iterator<Item = &Option<PrimitiveValue>> {
        self.elements
            .as_ref()
            .map(|m| Either::Left(m.keys()))
            .unwrap_or(Either::Right(std::iter::empty()))
    }

    /// Iterates the values of the map.
    pub fn values(&self) -> impl Iterator<Item = &Value> {
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
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.members
            .as_ref()
            .map(|m| Either::Left(m.iter().map(|(k, v)| (k.as_str(), v))))
            .unwrap_or(Either::Right(std::iter::empty()))
    }

    /// Iterates the keys of the object.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.members
            .as_ref()
            .map(|m| Either::Left(m.keys().map(|k| k.as_str())))
            .unwrap_or(Either::Right(std::iter::empty()))
    }

    /// Iterates the values of the object.
    pub fn values(&self) -> impl Iterator<Item = &Value> {
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
    pub fn new<S, V>(ty: impl Into<Type>, members: impl IntoIterator<Item = (S, V)>) -> Result<Self>
    where
        S: Into<String>,
        V: Into<Value>,
    {
        let ty = ty.into();
        if let Type::Compound(CompoundType::Struct(ty), optional) = ty {
            let mut members = members
                .into_iter()
                .map(|(n, v)| {
                    let n = n.into();
                    let v = v.into();
                    let v = v
                        .coerce(ty.members().get(&n).ok_or_else(|| {
                            anyhow!("struct does not contain a member named `{n}`")
                        })?)
                        .with_context(|| format!("failed to coerce struct member `{n}`"))?;
                    Ok((n, v))
                })
                .collect::<Result<IndexMap<_, _>>>()?;

            for (name, ty) in ty.members().iter() {
                // Check for optional members that should be set to `None`
                if ty.is_optional() {
                    if !members.contains_key(name) {
                        members.insert(name.clone(), Value::None);
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
                ty: Type::Compound(CompoundType::Struct(ty), optional),
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
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.members.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Iterates the keys of the struct.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.members.keys().map(|k| k.as_str())
    }

    /// Iterates the values of the struct.
    pub fn values(&self) -> impl Iterator<Item = &Value> {
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
            _ => None,
        }
    }

    /// Visits each `File` or `Directory` value contained in this value.
    ///
    /// Note that paths may be specified as URLs.
    fn visit_paths(&self, cb: &mut impl FnMut(bool, &PrimitiveValue) -> Result<()>) -> Result<()> {
        match self {
            Self::Pair(pair) => {
                let ty = pair.ty.as_pair().expect("should be a pair type");
                pair.left().visit_paths(ty.left_type().is_optional(), cb)?;
                pair.right()
                    .visit_paths(ty.right_type().is_optional(), cb)?;
            }
            Self::Array(array) => {
                let ty = array.ty.as_array().expect("should be an array type");
                let optional = ty.element_type().is_optional();
                if let Some(elements) = &array.elements {
                    for v in elements.iter() {
                        v.visit_paths(optional, cb)?;
                    }
                }
            }
            Self::Map(map) => {
                let ty = map.ty.as_map().expect("should be a map type");
                let (key_optional, value_optional) =
                    (ty.key_type().is_optional(), ty.value_type().is_optional());
                if let Some(elements) = &map.elements {
                    for (k, v) in elements.iter() {
                        if let Some(k) = k {
                            k.visit_paths(key_optional, cb)?;
                        }

                        v.visit_paths(value_optional, cb)?;
                    }
                }
            }
            Self::Object(object) => {
                if let Some(members) = &object.members {
                    for v in members.values() {
                        v.visit_paths(false, cb)?;
                    }
                }
            }
            Self::Struct(s) => {
                let ty = s.ty.as_struct().expect("should be a struct type");
                for (n, v) in s.members.iter() {
                    v.visit_paths(ty.members()[n].is_optional(), cb)?;
                }
            }
        }

        Ok(())
    }

    /// Mutably visits each `File` or `Directory` value contained in this value.
    ///
    /// If the provided callback returns `Ok(false)`, the `File` or `Directory`
    /// value will be replaced with `None`.
    ///
    /// Note that paths may be specified as URLs.
    fn visit_paths_mut(
        &mut self,
        cb: &mut impl FnMut(bool, &mut PrimitiveValue) -> Result<bool>,
    ) -> Result<()> {
        match self {
            Self::Pair(pair) => {
                let ty = pair.ty.as_pair().expect("should be a pair type");
                let (left_optional, right_optional) =
                    (ty.left_type().is_optional(), ty.right_type().is_optional());
                let values = Arc::make_mut(&mut pair.values);
                values.0.visit_paths_mut(left_optional, cb)?;
                values.1.visit_paths_mut(right_optional, cb)?;
            }
            Self::Array(array) => {
                let ty = array.ty.as_array().expect("should be an array type");
                let optional = ty.element_type().is_optional();
                if let Some(elements) = &mut array.elements {
                    for v in Arc::make_mut(elements) {
                        v.visit_paths_mut(optional, cb)?;
                    }
                }
            }
            Self::Map(map) => {
                let ty = map.ty.as_map().expect("should be a map type");
                let (key_optional, value_optional) =
                    (ty.key_type().is_optional(), ty.value_type().is_optional());
                if let Some(elements) = &mut map.elements {
                    if elements
                        .iter()
                        .find_map(|(k, _)| {
                            k.as_ref().map(|v| {
                                matches!(v, PrimitiveValue::File(_) | PrimitiveValue::Directory(_))
                            })
                        })
                        .unwrap_or(false)
                    {
                        // The key type contains a path, we need to rebuild the map to alter the
                        // keys
                        let elements = Arc::make_mut(elements);
                        let new = elements
                            .drain(..)
                            .map(|(mut k, mut v)| {
                                if let Some(v) = &mut k {
                                    if !v.visit_paths_mut(key_optional, cb)? {
                                        k = None;
                                    }
                                }

                                v.visit_paths_mut(value_optional, cb)?;
                                Ok((k, v))
                            })
                            .collect::<Result<Vec<_>>>()?;
                        elements.extend(new);
                    } else {
                        // Otherwise, we can just mutable the values in place
                        for v in Arc::make_mut(elements).values_mut() {
                            v.visit_paths_mut(value_optional, cb)?;
                        }
                    }
                }
            }
            Self::Object(object) => {
                if let Some(members) = &mut object.members {
                    for v in Arc::make_mut(members).values_mut() {
                        v.visit_paths_mut(false, cb)?;
                    }
                }
            }
            Self::Struct(s) => {
                let ty = s.ty.as_struct().expect("should be a struct type");
                for (n, v) in Arc::make_mut(&mut s.members).iter_mut() {
                    v.visit_paths_mut(ty.members()[n].is_optional(), cb)?;
                }
            }
        }

        Ok(())
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
        }
    }
}

impl Coercible for CompoundValue {
    fn coerce(&self, target: &Type) -> Result<Self> {
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
                        target.clone(),
                        v.as_slice().iter().cloned(),
                    )?));
                }
                // Map[W, Y] -> Map[X, Z] where W -> X and Y -> Z
                (Self::Map(v), CompoundType::Map(_)) => {
                    return Ok(Self::Map(Map::new(
                        target.clone(),
                        v.iter().map(|(k, v)| {
                            (k.clone().map(Into::into).unwrap_or(Value::None), v.clone())
                        }),
                    )?));
                }
                // Pair[W, Y] -> Pair[X, Z] where W -> X and Y -> Z
                (Self::Pair(v), CompoundType::Pair(_)) => {
                    return Ok(Self::Pair(Pair::new(
                        target.clone(),
                        v.values.0.clone(),
                        v.values.1.clone(),
                    )?));
                }
                // Map[String, Y] -> Struct
                (Self::Map(v), CompoundType::Struct(target_ty)) => {
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
                                    let k: String = k
                                        .as_ref()
                                        .and_then(|k| k.as_string())
                                        .with_context(|| {
                                            format!(
                                                "cannot coerce a map with a non-string key type \
                                                 to struct type `{target}`"
                                            )
                                        })?
                                        .to_string();
                                    let ty = target_ty.members().get(&k).with_context(|| {
                                        format!(
                                            "cannot coerce a map with key `{k}` to struct type \
                                             `{target}` as the struct does not contain a member \
                                             with that name"
                                        )
                                    })?;
                                    let v = v.coerce(ty).with_context(|| {
                                        format!("failed to coerce value of map key `{k}")
                                    })?;
                                    Ok((k, v))
                                })
                                .collect::<Result<_>>()?,
                        ),
                    }));
                }
                // Struct -> Map[String, Y]
                // Object -> Map[String, Y]
                (Self::Struct(Struct { members, .. }), CompoundType::Map(map_ty)) => {
                    if map_ty.key_type().as_primitive() != Some(PrimitiveType::String) {
                        bail!(
                            "cannot coerce a struct or object to type `{target}` as it requires a \
                             `String` key type"
                        );
                    }

                    let value_ty = map_ty.value_type();
                    return Ok(Self::Map(Map::new_unchecked(
                        target.clone(),
                        members
                            .iter()
                            .map(|(n, v)| {
                                let v = v
                                    .coerce(value_ty)
                                    .with_context(|| format!("failed to coerce member `{n}`"))?;
                                Ok((PrimitiveValue::new_string(n).into(), v))
                            })
                            .collect::<Result<_>>()?,
                    )));
                }
                (Self::Object(object), CompoundType::Map(map_ty)) => {
                    if map_ty.key_type().as_primitive() != Some(PrimitiveType::String) {
                        bail!(
                            "cannot coerce a struct or object to type `{target}` as it requires a \
                             `String` key type",
                        );
                    }

                    let value_ty = map_ty.value_type();
                    return Ok(Self::Map(Map::new_unchecked(
                        target.clone(),
                        object
                            .iter()
                            .map(|(n, v)| {
                                let v = v
                                    .coerce(value_ty)
                                    .with_context(|| format!("failed to coerce member `{n}`"))?;
                                Ok((PrimitiveValue::new_string(n).into(), v))
                            })
                            .collect::<Result<_>>()?,
                    )));
                }
                // Object -> Struct
                (Self::Object(v), CompoundType::Struct(_)) => {
                    return Ok(Self::Struct(Struct::new(
                        target.clone(),
                        v.iter().map(|(k, v)| (k, v.clone())),
                    )?));
                }
                // Struct -> Struct
                (Self::Struct(v), CompoundType::Struct(struct_ty)) => {
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
                                    let v = v.coerce(ty).with_context(|| {
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
                // Map[String, Y] -> Object
                Self::Map(v) => {
                    return Ok(Self::Object(Object::new(
                        v.iter()
                            .map(|(k, v)| {
                                let k = k
                                    .as_ref()
                                    .and_then(|k| k.as_string())
                                    .context(
                                        "cannot coerce a map with a non-string key type to type \
                                         `Object`",
                                    )?
                                    .to_string();
                                Ok((k, v.clone()))
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

impl serde::Serialize for CompoundValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::Error;

        match self {
            Self::Pair(_) => Err(S::Error::custom("a pair cannot be serialized")),
            Self::Array(v) => {
                let mut s = serializer.serialize_seq(Some(v.len()))?;
                for v in v.as_slice() {
                    s.serialize_element(v)?;
                }

                s.end()
            }
            Self::Map(v) => {
                if !v
                    .ty()
                    .as_map()
                    .expect("type should be a map")
                    .key_type()
                    .is_coercible_to(&PrimitiveType::String.into())
                {
                    return Err(S::Error::custom(
                        "only maps with `String` key types may be serialized",
                    ));
                }

                let mut s = serializer.serialize_map(Some(v.len()))?;
                for (k, v) in v.iter() {
                    s.serialize_entry(k, v)?;
                }

                s.end()
            }
            Self::Object(object) => {
                let mut s = serializer.serialize_map(Some(object.len()))?;
                for (k, v) in object.iter() {
                    s.serialize_entry(k, v)?;
                }

                s.end()
            }
            Self::Struct(Struct { members, .. }) => {
                let mut s = serializer.serialize_map(Some(members.len()))?;
                for (k, v) in members.iter() {
                    s.serialize_entry(k, v)?;
                }

                s.end()
            }
        }
    }
}

/// Immutable data for task values.
#[derive(Debug)]
struct TaskData {
    /// The name of the task.
    name: Arc<String>,
    /// The id of the task.
    id: Arc<String>,
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
    /// The time by which the task must be completed, as a Unix time stamp.
    ///
    /// A value of `None` indicates there is no deadline.
    end_time: Option<i64>,
    /// The task's `meta` section as an object.
    meta: Object,
    /// The tasks's `parameter_meta` section as an object.
    parameter_meta: Object,
    /// The task's extension metadata.
    ext: Object,
}

/// Represents a value for `task` variables in WDL 1.2.
///
/// Task values are cheap to clone.
#[derive(Debug, Clone)]
pub struct TaskValue {
    /// The immutable data for task values.
    data: Arc<TaskData>,
    /// The current task attempt count.
    ///
    /// The value must be 0 the first time the task is executed and incremented
    /// by 1 each time the task is retried (if any).
    attempt: i64,
    /// The task's return code.
    ///
    /// Initially set to `None`, but set after task execution completes.
    return_code: Option<i64>,
}

impl TaskValue {
    /// Constructs a new task value with the given name and identifier.
    pub(crate) fn new_v1<N: TreeNode>(
        name: impl Into<String>,
        id: impl Into<String>,
        definition: &v1::TaskDefinition<N>,
        constraints: TaskExecutionConstraints,
        attempt: i64,
    ) -> Self {
        Self {
            data: Arc::new(TaskData {
                name: Arc::new(name.into()),
                id: Arc::new(id.into()),
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
                end_time: None,
                meta: definition
                    .metadata()
                    .map(|s| Object::from_v1_metadata(s.items()))
                    .unwrap_or_else(Object::empty),
                parameter_meta: definition
                    .parameter_metadata()
                    .map(|s| Object::from_v1_metadata(s.items()))
                    .unwrap_or_else(Object::empty),
                ext: Object::empty(),
            }),
            attempt,
            return_code: None,
        }
    }

    /// Gets the task name.
    pub fn name(&self) -> &Arc<String> {
        &self.data.name
    }

    /// Gets the unique ID of the task.
    pub fn id(&self) -> &Arc<String> {
        &self.data.id
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
        self.data.end_time
    }

    /// Gets the task's return code.
    ///
    /// Initially set to `None`, but set after task execution completes.
    pub fn return_code(&self) -> Option<i64> {
        self.return_code
    }

    /// Gets the task's `meta` section as an object.
    pub fn meta(&self) -> &Object {
        &self.data.meta
    }

    /// Gets the tasks's `parameter_meta` section as an object.
    pub fn parameter_meta(&self) -> &Object {
        &self.data.parameter_meta
    }

    /// Gets the task's extension metadata.
    pub fn ext(&self) -> &Object {
        &self.data.ext
    }

    /// Sets the return code after the task execution has completed.
    pub(crate) fn set_return_code(&mut self, code: i32) {
        self.return_code = Some(code as i64);
    }

    /// Sets the attempt number for the task.
    pub(crate) fn set_attempt(&mut self, attempt: i64) {
        self.attempt = attempt;
    }

    /// Accesses a field of the task value by name.
    ///
    /// Returns `None` if the name is not a known field name.
    pub fn field(&self, name: &str) -> Option<Value> {
        match name {
            n if n == TASK_FIELD_NAME => {
                Some(PrimitiveValue::String(self.data.name.clone()).into())
            }
            n if n == TASK_FIELD_ID => Some(PrimitiveValue::String(self.data.id.clone()).into()),
            n if n == TASK_FIELD_CONTAINER => Some(
                self.data
                    .container
                    .clone()
                    .map(|c| PrimitiveValue::String(c).into())
                    .unwrap_or(Value::None),
            ),
            n if n == TASK_FIELD_CPU => Some(self.data.cpu.into()),
            n if n == TASK_FIELD_MEMORY => Some(self.data.memory.into()),
            n if n == TASK_FIELD_GPU => Some(self.data.gpu.clone().into()),
            n if n == TASK_FIELD_FPGA => Some(self.data.fpga.clone().into()),
            n if n == TASK_FIELD_DISKS => Some(self.data.disks.clone().into()),
            n if n == TASK_FIELD_ATTEMPT => Some(self.attempt.into()),
            n if n == TASK_FIELD_END_TIME => {
                Some(self.data.end_time.map(Into::into).unwrap_or(Value::None))
            }
            n if n == TASK_FIELD_RETURN_CODE => {
                Some(self.return_code.map(Into::into).unwrap_or(Value::None))
            }
            n if n == TASK_FIELD_META => Some(self.data.meta.clone().into()),
            n if n == TASK_FIELD_PARAMETER_META => Some(self.data.parameter_meta.clone().into()),
            n if n == TASK_FIELD_EXT => Some(self.data.ext.clone().into()),
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

#[cfg(test)]
mod test {
    use approx::assert_relative_eq;
    use pretty_assertions::assert_eq;
    use wdl_analysis::types::ArrayType;
    use wdl_analysis::types::MapType;
    use wdl_analysis::types::PairType;
    use wdl_analysis::types::StructType;

    use super::*;

    #[test]
    fn boolean_coercion() {
        // Boolean -> Boolean
        assert_eq!(
            Value::from(false)
                .coerce(&PrimitiveType::Boolean.into())
                .expect("should coerce")
                .unwrap_boolean(),
            Value::from(false).unwrap_boolean()
        );
        // Boolean -> String (invalid)
        assert_eq!(
            format!(
                "{e:?}",
                e = Value::from(true)
                    .coerce(&PrimitiveType::String.into())
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
                .coerce(&PrimitiveType::Integer.into())
                .expect("should coerce")
                .unwrap_integer(),
            Value::from(12345).unwrap_integer()
        );
        // Int -> Float
        assert_relative_eq!(
            Value::from(12345)
                .coerce(&PrimitiveType::Float.into())
                .expect("should coerce")
                .unwrap_float(),
            Value::from(12345.0).unwrap_float()
        );
        // Int -> Boolean (invalid)
        assert_eq!(
            format!(
                "{e:?}",
                e = Value::from(12345)
                    .coerce(&PrimitiveType::Boolean.into())
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
                .coerce(&PrimitiveType::Float.into())
                .expect("should coerce")
                .unwrap_float(),
            Value::from(12345.0).unwrap_float()
        );
        // Float -> Int (invalid)
        assert_eq!(
            format!(
                "{e:?}",
                e = Value::from(12345.0)
                    .coerce(&PrimitiveType::Integer.into())
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
                .coerce(&PrimitiveType::String.into())
                .expect("should coerce"),
            value
        );
        // String -> File
        assert_eq!(
            value
                .coerce(&PrimitiveType::File.into())
                .expect("should coerce"),
            PrimitiveValue::File(value.as_string().expect("should be string").clone())
        );
        // String -> Directory
        assert_eq!(
            value
                .coerce(&PrimitiveType::Directory.into())
                .expect("should coerce"),
            PrimitiveValue::Directory(value.as_string().expect("should be string").clone())
        );
        // String -> Boolean (invalid)
        assert_eq!(
            format!(
                "{e:?}",
                e = value.coerce(&PrimitiveType::Boolean.into()).unwrap_err()
            ),
            "cannot coerce type `String` to type `Boolean`"
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
                .coerce(&PrimitiveType::File.into())
                .expect("should coerce"),
            value
        );
        // File -> String
        assert_eq!(
            value
                .coerce(&PrimitiveType::String.into())
                .expect("should coerce"),
            PrimitiveValue::String(value.as_file().expect("should be file").clone())
        );
        // File -> Directory (invalid)
        assert_eq!(
            format!(
                "{e:?}",
                e = value.coerce(&PrimitiveType::Directory.into()).unwrap_err()
            ),
            "cannot coerce type `File` to type `Directory`"
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
                .coerce(&PrimitiveType::Directory.into())
                .expect("should coerce"),
            value
        );
        // Directory -> String
        assert_eq!(
            value
                .coerce(&PrimitiveType::String.into())
                .expect("should coerce"),
            PrimitiveValue::String(value.as_directory().expect("should be directory").clone())
        );
        // Directory -> File (invalid)
        assert_eq!(
            format!(
                "{e:?}",
                e = value.coerce(&PrimitiveType::File.into()).unwrap_err()
            ),
            "cannot coerce type `Directory` to type `File`"
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
            Value::None
                .coerce(&Type::from(PrimitiveType::String).optional())
                .expect("should coerce")
                .is_none(),
        );

        // None -> String (invalid)
        assert_eq!(
            format!(
                "{e:?}",
                e = Value::None
                    .coerce(&PrimitiveType::String.into())
                    .unwrap_err()
            ),
            "cannot coerce `None` to non-optional type `String`"
        );
    }

    #[test]
    fn none_display() {
        assert_eq!(Value::None.to_string(), "None");
    }

    #[test]
    fn array_coercion() {
        let src_ty: Type = ArrayType::new(PrimitiveType::Integer).into();
        let target_ty: Type = ArrayType::new(PrimitiveType::Float).into();

        // Array[Int] -> Array[Float]
        let src: CompoundValue = Array::new(src_ty, [1, 2, 3])
            .expect("should create array value")
            .into();
        let target = src.coerce(&target_ty).expect("should coerce");
        assert_eq!(
            target.unwrap_array().to_string(),
            "[1.000000, 2.000000, 3.000000]"
        );

        // Array[Int] -> Array[String] (invalid)
        let target_ty: Type = ArrayType::new(PrimitiveType::String).into();
        assert_eq!(
            format!("{e:?}", e = src.coerce(&target_ty).unwrap_err()),
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
        let value: Value = Array::new(ty.clone(), [string])
            .expect("should create array")
            .into();
        assert!(value.coerce(&target_ty).is_ok(), "should coerce");

        // Array[String] (empty) -> Array[String]+ (invalid)
        let value: Value = Array::new::<Value>(ty, [])
            .expect("should create array")
            .into();
        assert_eq!(
            format!("{e:?}", e = value.coerce(&target_ty).unwrap_err()),
            "cannot coerce empty array value to non-empty array type `Array[String]+`"
        );
    }

    #[test]
    fn array_display() {
        let ty: Type = ArrayType::new(PrimitiveType::Integer).into();
        let value: Value = Array::new(ty, [1, 2, 3])
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
        let value: Value = Map::new(ty, [(key1, value1), (key2, value2)])
            .expect("should create map value")
            .into();

        // Map[File, String] -> Map[String, File]
        let ty = MapType::new(PrimitiveType::String, PrimitiveType::File).into();
        let value = value.coerce(&ty).expect("value should coerce");
        assert_eq!(value.to_string(), r#"{"foo": "bar", "baz": "qux"}"#);

        // Map[String, File] -> Map[Int, File] (invalid)
        let ty = MapType::new(PrimitiveType::Integer, PrimitiveType::File).into();
        assert_eq!(
            format!("{e:?}", e = value.coerce(&ty).unwrap_err()),
            r#"failed to coerce map key for element at index 0

Caused by:
    cannot coerce type `String` to type `Int`"#
        );

        // Map[String, File] -> Map[String, Int] (invalid)
        let ty = MapType::new(PrimitiveType::String, PrimitiveType::Integer).into();
        assert_eq!(
            format!("{e:?}", e = value.coerce(&ty).unwrap_err()),
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
        let struct_value = value.coerce(&ty).expect("value should coerce");
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
            format!("{e:?}", e = value.coerce(&ty).unwrap_err()),
            "cannot coerce a map of 2 elements to struct type `Foo` as the struct has 3 members"
        );

        // Map[String, File] -> Object
        let object_value = value.coerce(&Type::Object).expect("value should coerce");
        assert_eq!(
            object_value.to_string(),
            r#"object {foo: "bar", baz: "qux"}"#
        );
    }

    #[test]
    fn map_display() {
        let ty = MapType::new(PrimitiveType::Integer, PrimitiveType::Boolean);
        let value: Value = Map::new(ty, [(1, true), (2, false)])
            .expect("should create map value")
            .into();
        assert_eq!(value.to_string(), "{1: true, 2: false}");
    }

    #[test]
    fn pair_coercion() {
        let left = PrimitiveValue::new_file("foo");
        let right = PrimitiveValue::new_string("bar");

        let ty = PairType::new(PrimitiveType::File, PrimitiveType::String);
        let value: Value = Pair::new(ty, left, right)
            .expect("should create map value")
            .into();

        // Pair[File, String] -> Pair[String, File]
        let ty = PairType::new(PrimitiveType::String, PrimitiveType::File).into();
        let value = value.coerce(&ty).expect("value should coerce");
        assert_eq!(value.to_string(), r#"("foo", "bar")"#);

        // Pair[String, File] -> Pair[Int, Int]
        let ty = PairType::new(PrimitiveType::Integer, PrimitiveType::Integer).into();
        assert_eq!(
            format!("{e:?}", e = value.coerce(&ty).unwrap_err()),
            r#"failed to coerce pair's left value

Caused by:
    cannot coerce type `String` to type `Int`"#
        );
    }

    #[test]
    fn pair_display() {
        let ty = PairType::new(PrimitiveType::Integer, PrimitiveType::Boolean);
        let value: Value = Pair::new(ty, 12345, false)
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
        let value: Value = Struct::new(ty, [("foo", 1.0), ("bar", 2.0), ("baz", 3.0)])
            .expect("should create map value")
            .into();

        // Struct -> Map[String, Float]
        let ty = MapType::new(PrimitiveType::String, PrimitiveType::Float).into();
        let map_value = value.coerce(&ty).expect("value should coerce");
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
        let struct_value = value.coerce(&ty).expect("value should coerce");
        assert_eq!(
            struct_value.to_string(),
            r#"Bar {foo: 1.000000, bar: 2.000000, baz: 3.000000}"#
        );

        // Struct -> Object
        let object_value = value.coerce(&Type::Object).expect("value should coerce");
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
}
