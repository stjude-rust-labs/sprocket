//! Implementation of the WDL runtime and values.

use std::fmt;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use id_arena::Id;
use indexmap::IndexMap;
use ordered_float::OrderedFloat;
use serde_json::Value as JsonValue;
use string_interner::symbol::SymbolU32;
use wdl_analysis::types::ArrayType;
use wdl_analysis::types::CompoundTypeDef;
use wdl_analysis::types::Optional;
use wdl_analysis::types::PrimitiveTypeKind;
use wdl_analysis::types::Type;

use crate::Engine;

/// Implemented on coercible values.
pub trait Coercible: Sized {
    /// Coerces the value into the given type.
    ///
    /// Returns `None` if the coercion is not supported.
    ///
    /// # Panics
    ///
    /// Panics if the provided target type is not from the given engine's type
    /// collection.
    fn coerce(&self, engine: &mut Engine, target: Type) -> Result<Self>;
}

/// Represents a WDL runtime value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Value {
    /// The value is a `Boolean`.
    Boolean(bool),
    /// The value is an `Int`.
    Integer(i64),
    /// The value is a `Float`.
    Float(OrderedFloat<f64>),
    /// The value is a `String`.
    String(SymbolU32),
    /// The value is a `File`.
    File(SymbolU32),
    /// The value is a `Directory`.
    Directory(SymbolU32),
    /// The value is a literal `None` value.
    None,
    /// The value is a compound value.
    Compound(CompoundValueId),
}

impl Value {
    /// Converts a JSON value into a WDL value.
    ///
    /// Returns an error if the JSON value cannot be represented as a WDL value.
    pub fn from_json(engine: &mut Engine, value: JsonValue) -> Result<Self> {
        match value {
            JsonValue::Null => Ok(Value::None),
            JsonValue::Bool(value) => Ok(value.into()),
            JsonValue::Number(number) => {
                if let Some(value) = number.as_i64() {
                    Ok(value.into())
                } else if let Some(value) = number.as_f64() {
                    Ok(value.into())
                } else {
                    bail!("number `{number}` is out of range for a WDL value")
                }
            }
            JsonValue::String(s) => Ok(engine.new_string(s)),
            JsonValue::Array(elements) => {
                let elements = elements
                    .into_iter()
                    .map(|v| Self::from_json(engine, v))
                    .collect::<Result<Vec<_>>>()?;

                let element_ty = elements
                    .iter()
                    .try_fold(None, |mut ty, element| {
                        let element_ty = element.ty(engine);
                        let ty = ty.get_or_insert(element_ty);
                        ty.common_type(engine.types(), element_ty)
                            .map(Some)
                            .ok_or_else(|| {
                                anyhow!(
                                    "a common element type does not exist between `{ty}` and \
                                     `{element_ty}`",
                                    ty = ty.display(engine.types()),
                                    element_ty = element_ty.display(engine.types())
                                )
                            })
                    })
                    .context("invalid WDL array value")?
                    .unwrap_or(Type::Union);

                let ty = engine.types_mut().add_array(ArrayType::new(element_ty));
                engine.new_array(ty, elements).with_context(|| {
                    format!(
                        "cannot coerce value to `{ty}`",
                        ty = ty.display(engine.types())
                    )
                })
            }
            JsonValue::Object(elements) => {
                let elements = elements
                    .into_iter()
                    .map(|(k, v)| Ok((k, Self::from_json(engine, v)?)))
                    .collect::<Result<Vec<_>>>()?;
                Ok(engine.new_object(elements))
            }
        }
    }

    /// Gets the type of the value.
    pub fn ty(&self, engine: &Engine) -> Type {
        match self {
            Self::Boolean(_) => PrimitiveTypeKind::Boolean.into(),
            Self::Integer(_) => PrimitiveTypeKind::Integer.into(),
            Self::Float(_) => PrimitiveTypeKind::Float.into(),
            Self::String(_) => PrimitiveTypeKind::String.into(),
            Self::File(_) => PrimitiveTypeKind::File.into(),
            Self::Directory(_) => PrimitiveTypeKind::Directory.into(),
            Self::None => Type::None,
            Self::Compound(id) => engine.value(*id).ty(),
        }
    }

    /// Gets the value as a `Boolean`.
    ///
    /// Returns `None` if the value is not a `Boolean`.
    pub fn as_boolean(self) -> Option<bool> {
        match self {
            Self::Boolean(v) => Some(v),
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
    pub fn as_integer(self) -> Option<i64> {
        match self {
            Self::Integer(v) => Some(v),
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
    pub fn as_float(self) -> Option<f64> {
        match self {
            Self::Float(v) => Some(v.into()),
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
    pub fn as_string(self, engine: &Engine) -> Option<&str> {
        match self {
            Self::String(_) => Some(self.to_str(engine).expect("string should be interned")),
            _ => panic!("value is not a string"),
        }
    }

    /// Unwraps the value into a `String`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `String`.
    pub fn unwrap_string(self, engine: &Engine) -> &str {
        match self {
            Self::String(_) => self.to_str(engine).expect("string should be interned"),
            _ => panic!("value is not a string"),
        }
    }

    /// Gets the value as a `File`.
    ///
    /// Returns `None` if the value is not a `File`.
    pub fn as_file(self, engine: &Engine) -> Option<&str> {
        match self {
            Self::File(_) => Some(self.to_str(engine).expect("string should be interned")),
            _ => None,
        }
    }

    /// Unwraps the value into a `File`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `File`.
    pub fn unwrap_file(self, engine: &Engine) -> &str {
        match self {
            Self::File(_) => self.to_str(engine).expect("string should be interned"),
            _ => panic!("value is not a file"),
        }
    }

    /// Gets the value as a `Directory`.
    ///
    /// Returns `None` if the value is not a `Directory`.
    pub fn as_directory(self, engine: &Engine) -> Option<&str> {
        match self {
            Self::Directory(_) => Some(self.to_str(engine).expect("string should be interned")),
            _ => None,
        }
    }

    /// Unwraps the value into a `Directory`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `Directory`.
    pub fn unwrap_directory(self, engine: &Engine) -> &str {
        match self {
            Self::Directory(_) => self.to_str(engine).expect("string should be interned"),
            _ => panic!("value is not a directory"),
        }
    }

    /// Gets the value as a `Pair`.
    ///
    /// Returns `None` if the value is not a `Pair`.
    pub fn as_pair(self, engine: &Engine) -> Option<&Pair> {
        match self {
            Self::Compound(id) => match engine.value(id) {
                CompoundValue::Pair(v) => Some(v),
                _ => None,
            },
            _ => None,
        }
    }

    /// Unwraps the value into a `Pair`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `Pair`.
    pub fn unwrap_pair(self, engine: &Engine) -> &Pair {
        match self {
            Self::Compound(id) => match engine.value(id) {
                CompoundValue::Pair(v) => v,
                _ => panic!("value is not a pair"),
            },
            _ => panic!("value is not a pair"),
        }
    }

    /// Gets the value as an `Array`.
    ///
    /// Returns `None` if the value is not an `Array`.
    pub fn as_array(self, engine: &Engine) -> Option<&Array> {
        match self {
            Self::Compound(id) => match engine.value(id) {
                CompoundValue::Array(v) => Some(v),
                _ => None,
            },
            _ => None,
        }
    }

    /// Unwraps the value into an `Array`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not an `Array`.
    pub fn unwrap_array(self, engine: &Engine) -> &Array {
        match self {
            Self::Compound(id) => match engine.value(id) {
                CompoundValue::Array(v) => v,
                _ => panic!("value is not an array"),
            },
            _ => panic!("value is not an array"),
        }
    }

    /// Gets the value as a `Map`.
    ///
    /// Returns `None` if the value is not a `Map`.
    pub fn as_map(self, engine: &Engine) -> Option<&Map> {
        match self {
            Self::Compound(id) => match engine.value(id) {
                CompoundValue::Map(v) => Some(v),
                _ => None,
            },
            _ => None,
        }
    }

    /// Unwraps the value into a `Map`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `Map`.
    pub fn unwrap_map(self, engine: &Engine) -> &Map {
        match self {
            Self::Compound(id) => match engine.value(id) {
                CompoundValue::Map(v) => v,
                _ => panic!("value is not a map"),
            },
            _ => panic!("value is not a map"),
        }
    }

    /// Gets the value as an `Object`.
    ///
    /// Returns `None` if the value is not an `Object`.
    pub fn as_object(self, engine: &Engine) -> Option<&Object> {
        match self {
            Self::Compound(id) => match engine.value(id) {
                CompoundValue::Object(v) => Some(v),
                _ => None,
            },
            _ => None,
        }
    }

    /// Unwraps the value into an `Object`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not an `Object`.
    pub fn unwrap_object(self, engine: &Engine) -> &Object {
        match self {
            Self::Compound(id) => match engine.value(id) {
                CompoundValue::Object(v) => v,
                _ => panic!("value is not an object"),
            },
            _ => panic!("value is not an object"),
        }
    }

    /// Gets the value as a `Struct`.
    ///
    /// Returns `None` if the value is not a `Struct`.
    pub fn as_struct(self, engine: &Engine) -> Option<&Struct> {
        match self {
            Self::Compound(id) => match engine.value(id) {
                CompoundValue::Struct(v) => Some(v),
                _ => None,
            },
            _ => None,
        }
    }

    /// Unwraps the value into a `Struct`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `Map`.
    pub fn unwrap_struct(self, engine: &Engine) -> &Struct {
        match self {
            Self::Compound(id) => match engine.value(id) {
                CompoundValue::Struct(v) => v,
                _ => panic!("value is not a struct"),
            },
            _ => panic!("value is not a struct"),
        }
    }

    /// Gets the string representation of a `String`, `File`, or `Directory`
    /// value.
    ///
    /// Returns `None` if the value is not a `String`, `File`, or `Directory`.
    pub fn to_str<'a>(&self, engine: &'a Engine) -> Option<&'a str> {
        match self {
            Self::String(sym) | Self::File(sym) | Self::Directory(sym) => {
                engine.interner().resolve(*sym)
            }
            _ => None,
        }
    }

    /// Used to display the value.
    pub fn display<'a>(&'a self, engine: &'a Engine) -> impl fmt::Display + 'a {
        /// Helper type for implementing display.
        struct Display<'a> {
            /// A reference to the engine.
            engine: &'a Engine,
            /// The value to display.
            value: Value,
        }

        impl fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self.value {
                    Value::Boolean(v) => write!(f, "{v}"),
                    Value::Integer(v) => write!(f, "{v}"),
                    Value::Float(v) => write!(f, "{v:?}"),
                    Value::String(_) | Value::File(_) | Value::Directory(_) => {
                        // TODO: handle necessary escape sequences
                        write!(
                            f,
                            "\"{v}\"",
                            v = self
                                .value
                                .to_str(self.engine)
                                .expect("string should be interned")
                        )
                    }
                    Value::None => write!(f, "None"),
                    Value::Compound(id) => {
                        write!(f, "{v}", v = self.engine.value(id).display(self.engine))
                    }
                }
            }
        }

        Display {
            engine,
            value: *self,
        }
    }

    /// Asserts that the value is valid for the given engine.
    pub(crate) fn assert_valid(&self, engine: &Engine) {
        match self {
            Self::Boolean(_) | Self::Integer(_) | Self::Float(_) | Self::None => {}
            Self::String(sym) | Self::File(sym) | Self::Directory(sym) => assert!(
                engine.interner().resolve(*sym).is_some(),
                "value comes from a different engine"
            ),
            Self::Compound(id) => engine.assert_same_arena(*id),
        }
    }
}

impl Coercible for Value {
    fn coerce(&self, engine: &mut Engine, target: Type) -> Result<Self> {
        match self {
            Self::Boolean(v) => {
                target
                    .as_primitive()
                    .and_then(|ty| match ty.kind() {
                        // Boolean -> Boolean
                        PrimitiveTypeKind::Boolean => Some(Self::Boolean(*v)),
                        _ => None,
                    })
                    .ok_or_else(|| {
                        anyhow!(
                            "cannot coerce type `Boolean` to type `{target}`",
                            target = target.display(engine.types())
                        )
                    })
            }
            Self::Integer(v) => {
                target
                    .as_primitive()
                    .and_then(|ty| match ty.kind() {
                        // Int -> Int
                        PrimitiveTypeKind::Integer => Some(Self::Integer(*v)),
                        // Int -> Float
                        PrimitiveTypeKind::Float => Some(Self::Float((*v as f64).into())),
                        _ => None,
                    })
                    .ok_or_else(|| {
                        anyhow!(
                            "cannot coerce type `Int` to type `{target}`",
                            target = target.display(engine.types())
                        )
                    })
            }
            Self::Float(v) => {
                target
                    .as_primitive()
                    .and_then(|ty| match ty.kind() {
                        // Float -> Float
                        PrimitiveTypeKind::Float => Some(Self::Float(*v)),
                        _ => None,
                    })
                    .ok_or_else(|| {
                        anyhow!(
                            "cannot coerce type `Float` to type `{target}`",
                            target = target.display(engine.types())
                        )
                    })
            }
            Self::String(sym) => {
                target
                    .as_primitive()
                    .and_then(|ty| match ty.kind() {
                        // String -> String
                        PrimitiveTypeKind::String => Some(Self::String(*sym)),
                        // String -> File
                        PrimitiveTypeKind::File => Some(Self::File(*sym)),
                        // String -> Directory
                        PrimitiveTypeKind::Directory => Some(Self::Directory(*sym)),
                        _ => None,
                    })
                    .ok_or_else(|| {
                        anyhow!(
                            "cannot coerce type `String` to type `{target}`",
                            target = target.display(engine.types())
                        )
                    })
            }
            Self::File(sym) => {
                target
                    .as_primitive()
                    .and_then(|ty| match ty.kind() {
                        // File -> File
                        PrimitiveTypeKind::File => Some(Self::File(*sym)),
                        // File -> String
                        PrimitiveTypeKind::String => Some(Self::String(*sym)),
                        _ => None,
                    })
                    .ok_or_else(|| {
                        anyhow!(
                            "cannot coerce type `File` to type `{target}`",
                            target = target.display(engine.types())
                        )
                    })
            }
            Self::Directory(sym) => {
                target
                    .as_primitive()
                    .and_then(|ty| match ty.kind() {
                        // Directory -> Directory
                        PrimitiveTypeKind::Directory => Some(Self::Directory(*sym)),
                        // Directory -> String
                        PrimitiveTypeKind::String => Some(Self::String(*sym)),
                        _ => None,
                    })
                    .ok_or_else(|| {
                        anyhow!(
                            "cannot coerce type `Directory` to type `{target}`",
                            target = target.display(engine.types())
                        )
                    })
            }
            Self::None => {
                if target.is_optional() {
                    Ok(Self::None)
                } else {
                    bail!(
                        "cannot coerce `None` to non-optional type `{target}`",
                        target = target.display(engine.types())
                    );
                }
            }
            Self::Compound(id) => {
                let value = engine.value(*id).clone().coerce(engine, target)?;
                Ok(Self::Compound(engine.alloc(value)))
            }
        }
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Self::Float(value.into())
    }
}

/// Represents a `Pair` value.
#[derive(Debug, Clone, Copy)]
pub struct Pair {
    /// The type of the pair.
    ty: Type,
    /// The left value of the pair.
    left: Value,
    /// The right value of the pair.
    right: Value,
}

impl Pair {
    /// Constructs a new `Pair` value.
    pub(crate) fn new(ty: Type, left: Value, right: Value) -> Self {
        Self { ty, left, right }
    }

    /// Gets the type of the `Pair`.
    pub fn ty(&self) -> Type {
        self.ty
    }

    /// Gets the left value of the `Pair`.
    pub fn left(&self) -> Value {
        self.left
    }

    /// Gets the right value of the `Pair`.
    pub fn right(&self) -> Value {
        self.right
    }

    /// Used to display the value.
    pub fn display<'a>(&'a self, engine: &'a Engine) -> impl fmt::Display + 'a {
        /// Helper type for implementing display.
        struct Display<'a> {
            /// A reference to the engine.
            engine: &'a Engine,
            /// The value to display.
            value: &'a Pair,
        }

        impl fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(
                    f,
                    "({left}, {right})",
                    left = self.value.left.display(self.engine),
                    right = self.value.right.display(self.engine)
                )
            }
        }

        Display {
            engine,
            value: self,
        }
    }
}

/// Represents an `Array` value.
#[derive(Debug, Clone)]
pub struct Array {
    /// The type of the array.
    ty: Type,
    /// The array's elements.
    elements: Arc<[Value]>,
}

impl Array {
    /// Constructs a new `Array` value.
    pub(crate) fn new(ty: Type, elements: Arc<[Value]>) -> Self {
        Self { ty, elements }
    }

    /// Gets the type of the `Array` value.
    pub fn ty(&self) -> Type {
        self.ty
    }

    /// Gets the elements of the `Array` value.
    pub fn elements(&self) -> &[Value] {
        &self.elements
    }

    /// Used to display the value.
    pub fn display<'a>(&'a self, engine: &'a Engine) -> impl fmt::Display + 'a {
        /// Helper type for implementing display.
        struct Display<'a> {
            /// A reference to the engine.
            engine: &'a Engine,
            /// The value to display.
            value: &'a Array,
        }

        impl fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "[")?;

                for (i, element) in self.value.elements.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }

                    write!(f, "{element}", element = element.display(self.engine))?;
                }

                write!(f, "]")
            }
        }

        Display {
            engine,
            value: self,
        }
    }
}

/// Represents a `Map` value.
#[derive(Debug, Clone)]
pub struct Map {
    /// The type of the map value.
    ty: Type,
    /// The elements of the map value.
    elements: Arc<IndexMap<Value, Value>>,
}

impl Map {
    /// Constructs a new `Map` value.
    pub(crate) fn new(ty: Type, elements: Arc<IndexMap<Value, Value>>) -> Self {
        Self { ty, elements }
    }

    /// Gets the type of the `Map` value.
    pub fn ty(&self) -> Type {
        self.ty
    }

    /// Gets the elements of the `Map` value.
    pub fn elements(&self) -> &IndexMap<Value, Value> {
        &self.elements
    }

    /// Used to display the value.
    pub fn display<'a>(&'a self, engine: &'a Engine) -> impl fmt::Display + 'a {
        /// Helper type for implementing display.
        struct Display<'a> {
            /// A reference to the engine.
            engine: &'a Engine,
            /// The value to display.
            value: &'a Map,
        }

        impl fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{{")?;

                for (i, (k, v)) in self.value.elements.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }

                    write!(
                        f,
                        "{k}: {v}",
                        k = k.display(self.engine),
                        v = v.display(self.engine)
                    )?;
                }

                write!(f, "}}")
            }
        }

        Display {
            engine,
            value: self,
        }
    }
}

/// Represents an `Object` value.
#[derive(Debug, Clone)]
pub struct Object {
    /// The members of the object.
    members: Arc<IndexMap<String, Value>>,
}

impl Object {
    /// Constructs a new `Object` value.
    pub(crate) fn new(members: Arc<IndexMap<String, Value>>) -> Self {
        Self { members }
    }

    /// Gets the type of the `Object` value.
    pub fn ty(&self) -> Type {
        Type::Object
    }

    /// Gets the members of the `Object` value.
    pub fn members(&self) -> &IndexMap<String, Value> {
        &self.members
    }

    /// Used to display the value.
    pub fn display<'a>(&'a self, engine: &'a Engine) -> impl fmt::Display + 'a {
        /// Helper type for implementing display.
        struct Display<'a> {
            /// A reference to the engine.
            engine: &'a Engine,
            /// The value to display.
            value: &'a Object,
        }

        impl fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "object {{")?;

                for (i, (k, v)) in self.value.members.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }

                    write!(f, "{k}: {v}", v = v.display(self.engine))?;
                }

                write!(f, "}}")
            }
        }

        Display {
            engine,
            value: self,
        }
    }
}

/// Represents a `Struct` value.
#[derive(Debug, Clone)]
pub struct Struct {
    /// The type of the struct value.
    ty: Type,
    /// The members of the struct value.
    members: Arc<IndexMap<String, Value>>,
}

impl Struct {
    /// Constructs a new `Struct` value.
    pub(crate) fn new(ty: Type, members: Arc<IndexMap<String, Value>>) -> Self {
        Self { ty, members }
    }

    /// Gets the type of the `Struct` value.
    pub fn ty(&self) -> Type {
        self.ty
    }

    /// Gets the members of the `Struct` value.
    pub fn members(&self) -> &IndexMap<String, Value> {
        &self.members
    }

    /// Used to display the value.
    pub fn display<'a>(&'a self, engine: &'a Engine) -> impl fmt::Display + 'a {
        /// Helper type for implementing display.
        struct Display<'a> {
            /// A reference to the engine.
            engine: &'a Engine,
            /// The value to display.
            value: &'a Struct,
        }

        impl fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(
                    f,
                    "{name} {{",
                    name = self
                        .engine
                        .types()
                        .type_definition(match self.value.ty {
                            Type::Compound(ty) => ty.definition(),
                            _ => unreachable!("expected a struct type"),
                        })
                        .as_struct()
                        .expect("should be a struct")
                        .name()
                )?;

                for (i, (k, v)) in self.value.members.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }

                    write!(f, "{k}: {v}", v = v.display(self.engine))?;
                }

                write!(f, "}}")
            }
        }

        Display {
            engine,
            value: self,
        }
    }
}

/// Represents a compound value.
///
/// Compound values may be trivially cloned.
#[derive(Debug, Clone)]
pub enum CompoundValue {
    /// The value is a `Pair` of values.
    Pair(Pair),
    /// The value is an `Array` of values.
    Array(Array),
    /// The value is a `Map` of values.
    Map(Map),
    /// The value is an `Object.`
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

    /// Used to display the value.
    pub fn display<'a>(&'a self, engine: &'a Engine) -> impl fmt::Display + 'a {
        /// Helper type for implementing display.
        struct Display<'a> {
            /// A reference to the engine.
            engine: &'a Engine,
            /// The value to display.
            value: &'a CompoundValue,
        }

        impl fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self.value {
                    CompoundValue::Pair(v) => {
                        write!(f, "{v}", v = v.display(self.engine))
                    }
                    CompoundValue::Array(v) => {
                        write!(f, "{v}", v = v.display(self.engine))
                    }
                    CompoundValue::Map(v) => {
                        write!(f, "{v}", v = v.display(self.engine))
                    }
                    CompoundValue::Object(v) => {
                        write!(f, "{v}", v = v.display(self.engine))
                    }
                    CompoundValue::Struct(v) => {
                        write!(f, "{v}", v = v.display(self.engine))
                    }
                }
            }
        }

        Display {
            engine,
            value: self,
        }
    }
}

impl Coercible for CompoundValue {
    fn coerce(&self, engine: &mut Engine, target: Type) -> Result<Self> {
        if let Type::Compound(compound_ty) = target {
            match (
                self,
                engine.types().type_definition(compound_ty.definition()),
            ) {
                // Array[X] -> Array[Y](+) where X -> Y
                (Self::Array(v), CompoundTypeDef::Array(array_ty)) => {
                    // Don't allow coercion when the source is empty but the target has the
                    // non-empty qualifier
                    if v.elements.is_empty() && array_ty.is_non_empty() {
                        bail!(
                            "cannot coerce empty array value to non-empty array type `{ty}`",
                            ty = array_ty.display(engine.types())
                        );
                    }

                    let element_type = array_ty.element_type();
                    return Ok(Self::Array(Array::new(
                        target,
                        v.elements
                            .iter()
                            .enumerate()
                            .map(|(i, e)| {
                                e.coerce(engine, element_type).with_context(|| {
                                    format!("failed to coerce array element at index {i}")
                                })
                            })
                            .collect::<Result<_>>()?,
                    )));
                }
                // Map[W, Y] -> Map[X, Z] where W -> X and Y -> Z
                (Self::Map(v), CompoundTypeDef::Map(map_ty)) => {
                    let key_type = map_ty.key_type();
                    let value_type = map_ty.value_type();
                    return Ok(Self::Map(Map::new(
                        target,
                        Arc::new(
                            v.elements
                                .iter()
                                .enumerate()
                                .map(|(i, (k, v))| {
                                    let k = k.coerce(engine, key_type).with_context(|| {
                                        format!("failed to coerce map key at index {i}")
                                    })?;
                                    let v = v.coerce(engine, value_type).with_context(|| {
                                        format!("failed to coerce map value at index {i}")
                                    })?;
                                    Ok((k, v))
                                })
                                .collect::<Result<_>>()?,
                        ),
                    )));
                }
                // Pair[W, Y] -> Pair[X, Z] where W -> X and Y -> Z
                (Self::Pair(v), CompoundTypeDef::Pair(pair_ty)) => {
                    let left_type = pair_ty.left_type();
                    let right_type: Type = pair_ty.right_type();
                    let left = v
                        .left
                        .coerce(engine, left_type)
                        .context("failed to coerce pair's left value")?;
                    let right = v
                        .right
                        .coerce(engine, right_type)
                        .context("failed to coerce pair's right value")?;
                    return Ok(Self::Pair(Pair::new(target, left, right)));
                }
                // Map[String, Y] -> Struct
                (Self::Map(v), CompoundTypeDef::Struct(_)) => {
                    let len = v.elements.len();
                    let expected_len = engine
                        .types()
                        .type_definition(compound_ty.definition())
                        .as_struct()
                        .expect("should be struct")
                        .members()
                        .len();

                    if len != expected_len {
                        bail!(
                            "cannot coerce a map of {len} element{s1} to struct type `{ty}` as \
                             the struct has {expected_len} member{s2}",
                            s1 = if len == 1 { "" } else { "s" },
                            ty = compound_ty.display(engine.types()),
                            s2 = if expected_len == 1 { "" } else { "s" }
                        );
                    }

                    let members = v
                        .elements
                        .iter()
                        .map(|(k, v)| {
                            let k = k
                                .as_string(engine)
                                .ok_or_else(|| {
                                    anyhow!(
                                        "cannot coerce a map with a non-string key type to struct \
                                         type `{ty}`",
                                        ty = compound_ty.display(engine.types())
                                    )
                                })?
                                .to_string();
                            let ty = *engine
                                .types()
                                .type_definition(compound_ty.definition())
                                .as_struct()
                                .expect("should be struct")
                                .members()
                                .get(&k)
                                .ok_or_else(|| {
                                    anyhow!(
                                        "cannot coerce a map with key `{k}` to struct type `{ty}` \
                                         as the struct does not contain a member with that name",
                                        ty = compound_ty.display(engine.types())
                                    )
                                })?;
                            let v = v.coerce(engine, ty).with_context(|| {
                                format!("failed to coerce value of map key `{k}")
                            })?;
                            Ok((k, v))
                        })
                        .collect::<Result<_>>()?;

                    return Ok(Self::Struct(Struct::new(target, Arc::new(members))));
                }
                // Struct -> Map[String, Y]
                // Object -> Map[String, Y]
                (Self::Struct(Struct { members, .. }), CompoundTypeDef::Map(map_ty))
                | (Self::Object(Object { members }), CompoundTypeDef::Map(map_ty)) => {
                    if map_ty.key_type().as_primitive() != Some(PrimitiveTypeKind::String.into()) {
                        bail!(
                            "cannot coerce a struct or object to type `{ty}` as it requires a \
                             `String` key type",
                            ty = compound_ty.display(engine.types())
                        );
                    }

                    let value_ty = map_ty.value_type();
                    return Ok(Self::Map(Map::new(
                        target,
                        Arc::new(
                            members
                                .iter()
                                .map(|(n, v)| {
                                    let v = v.coerce(engine, value_ty).with_context(|| {
                                        format!("failed to coerce member `{n}`")
                                    })?;
                                    Ok((engine.new_string(n), v))
                                })
                                .collect::<Result<_>>()?,
                        ),
                    )));
                }
                // Object -> Struct
                (Self::Object(v), CompoundTypeDef::Struct(_)) => {
                    let len = v.members.len();
                    let expected_len = engine
                        .types()
                        .type_definition(compound_ty.definition())
                        .as_struct()
                        .expect("should be struct")
                        .members()
                        .len();

                    if len != expected_len {
                        bail!(
                            "cannot coerce an object of {len} members{s1} to struct type `{ty}` \
                             as the target struct has {expected_len} member{s2}",
                            s1 = if len == 1 { "" } else { "s" },
                            ty = compound_ty.display(engine.types()),
                            s2 = if expected_len == 1 { "" } else { "s" }
                        );
                    }

                    let members = Arc::new(
                        v.members
                            .iter()
                            .map(|(k, v)| {
                                let ty = engine
                                    .types()
                                    .type_definition(compound_ty.definition())
                                    .as_struct()
                                    .expect("should be struct")
                                    .members()
                                    .get(k)
                                    .ok_or_else(|| {
                                        anyhow!(
                                            "cannot coerce an object with member `{k}` to struct \
                                             type `{ty}` as the struct does not contain a member \
                                             with that name",
                                            ty = compound_ty.display(engine.types())
                                        )
                                    })?;
                                let v = v.coerce(engine, *ty)?;
                                Ok((k.clone(), v))
                            })
                            .collect::<Result<_>>()?,
                    );

                    return Ok(Self::Struct(Struct::new(target, members)));
                }
                // Struct -> Struct
                (Self::Struct(v), CompoundTypeDef::Struct(_)) => {
                    let len = v.members.len();
                    let expected_len = engine
                        .types()
                        .type_definition(compound_ty.definition())
                        .as_struct()
                        .expect("should be struct")
                        .members()
                        .len();

                    if len != expected_len {
                        bail!(
                            "cannot coerce a struct of {len} members{s1} to struct type `{ty}` as \
                             the target struct has {expected_len} member{s2}",
                            s1 = if len == 1 { "" } else { "s" },
                            ty = compound_ty.display(engine.types()),
                            s2 = if expected_len == 1 { "" } else { "s" }
                        );
                    }

                    let members = Arc::new(
                        v.members
                            .iter()
                            .map(|(k, v)| {
                                let ty = engine
                                    .types()
                                    .type_definition(compound_ty.definition())
                                    .as_struct()
                                    .expect("should be struct")
                                    .members()
                                    .get(k)
                                    .ok_or_else(|| {
                                        anyhow!(
                                            "cannot coerce a struct with member `{k}` to struct \
                                             type `{ty}` as the target struct does not contain a \
                                             member with that name",
                                            ty = compound_ty.display(engine.types())
                                        )
                                    })?;
                                let v = v
                                    .coerce(engine, *ty)
                                    .with_context(|| format!("failed to coerce member `{k}`"))?;
                                Ok((k.clone(), v))
                            })
                            .collect::<Result<_>>()?,
                    );

                    return Ok(Self::Struct(Struct::new(target, members)));
                }
                _ => {}
            };
        }

        if let Type::Object = target {
            match self {
                // Map[String, Y] -> Object
                Self::Map(v) => {
                    return Ok(Self::Object(Object::new(Arc::new(
                        v.elements
                            .iter()
                            .map(|(k, v)| {
                                let k = k
                                    .as_string(engine)
                                    .ok_or_else(|| {
                                        anyhow!(
                                            "cannot coerce a map with a non-string key type to \
                                             type `Object`"
                                        )
                                    })?
                                    .to_string();
                                Ok((k, *v))
                            })
                            .collect::<Result<_>>()?,
                    ))));
                }
                // Struct -> Object
                Self::Struct(v) => return Ok(Self::Object(Object::new(v.members.clone()))),
                _ => {}
            };
        }

        bail!(
            "cannot coerce a value of type `{ty}` to type `{expected}`",
            ty = self.ty().display(engine.types()),
            expected = target.display(engine.types())
        );
    }
}

/// Represents an identifier of a compound value.
pub type CompoundValueId = Id<CompoundValue>;

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use wdl_analysis::types::ArrayType;
    use wdl_analysis::types::MapType;
    use wdl_analysis::types::PairType;
    use wdl_analysis::types::StructType;

    use super::*;

    #[test]
    fn boolean_coercion() {
        let mut engine = Engine::default();

        // Boolean -> Boolean
        assert_eq!(
            Value::from(false)
                .coerce(&mut engine, PrimitiveTypeKind::Boolean.into())
                .expect("should coerce"),
            Value::from(false)
        );
        // Boolean -> String (invalid)
        assert_eq!(
            format!(
                "{e:?}",
                e = Value::from(true)
                    .coerce(&mut engine, PrimitiveTypeKind::String.into())
                    .unwrap_err()
            ),
            "cannot coerce type `Boolean` to type `String`"
        );
    }

    #[test]
    fn boolean_display() {
        let engine = Engine::default();

        assert_eq!(Value::from(false).display(&engine).to_string(), "false");
        assert_eq!(Value::from(true).display(&engine).to_string(), "true");
    }

    #[test]
    fn integer_coercion() {
        let mut engine = Engine::default();

        // Int -> Int
        assert_eq!(
            Value::from(12345)
                .coerce(&mut engine, PrimitiveTypeKind::Integer.into())
                .expect("should coerce"),
            Value::from(12345)
        );
        // Int -> Float
        assert_eq!(
            Value::from(12345)
                .coerce(&mut engine, PrimitiveTypeKind::Float.into())
                .expect("should coerce"),
            Value::from(12345.0)
        );
        // Int -> Boolean (invalid)
        assert_eq!(
            format!(
                "{e:?}",
                e = Value::from(12345)
                    .coerce(&mut engine, PrimitiveTypeKind::Boolean.into())
                    .unwrap_err()
            ),
            "cannot coerce type `Int` to type `Boolean`"
        );
    }

    #[test]
    fn integer_display() {
        let engine = Engine::default();

        assert_eq!(Value::from(12345).display(&engine).to_string(), "12345");
        assert_eq!(Value::from(-12345).display(&engine).to_string(), "-12345");
    }

    #[test]
    fn float_coercion() {
        let mut engine = Engine::default();

        // Float -> Float
        assert_eq!(
            Value::from(12345.0)
                .coerce(&mut engine, PrimitiveTypeKind::Float.into())
                .expect("should coerce"),
            Value::from(12345.0)
        );
        // Float -> Int (invalid)
        assert_eq!(
            format!(
                "{e:?}",
                e = Value::from(12345.0)
                    .coerce(&mut engine, PrimitiveTypeKind::Integer.into())
                    .unwrap_err()
            ),
            "cannot coerce type `Float` to type `Int`"
        );
    }

    #[test]
    fn float_display() {
        let engine = Engine::default();

        assert_eq!(
            Value::from(12345.12345).display(&engine).to_string(),
            "12345.12345"
        );
        assert_eq!(
            Value::from(-12345.12345).display(&engine).to_string(),
            "-12345.12345"
        );
    }

    #[test]
    fn string_coercion() {
        let mut engine = Engine::default();

        let value = engine.new_string("foo");
        let sym = match value {
            Value::String(sym) => sym,
            _ => panic!("expected a string value"),
        };

        // String -> String
        assert_eq!(
            value
                .coerce(&mut engine, PrimitiveTypeKind::String.into())
                .expect("should coerce"),
            value
        );
        // String -> File
        assert_eq!(
            value
                .coerce(&mut engine, PrimitiveTypeKind::File.into())
                .expect("should coerce"),
            Value::File(sym)
        );
        // String -> Directory
        assert_eq!(
            value
                .coerce(&mut engine, PrimitiveTypeKind::Directory.into())
                .expect("should coerce"),
            Value::Directory(sym)
        );
        // String -> Boolean (invalid)
        assert_eq!(
            format!(
                "{e:?}",
                e = value
                    .coerce(&mut engine, PrimitiveTypeKind::Boolean.into())
                    .unwrap_err()
            ),
            "cannot coerce type `String` to type `Boolean`"
        );
    }

    #[test]
    fn string_display() {
        let mut engine = Engine::default();

        let value = engine.new_string("hello world!");
        assert_eq!(value.display(&engine).to_string(), "\"hello world!\"");
    }

    #[test]
    fn file_coercion() {
        let mut engine = Engine::default();

        let value = engine.new_file("foo");
        let sym = match value {
            Value::File(sym) => sym,
            _ => panic!("expected a file value"),
        };

        // File -> File
        assert_eq!(
            value
                .coerce(&mut engine, PrimitiveTypeKind::File.into())
                .expect("should coerce"),
            value
        );
        // File -> String
        assert_eq!(
            value
                .coerce(&mut engine, PrimitiveTypeKind::String.into())
                .expect("should coerce"),
            Value::String(sym)
        );
        // File -> Directory (invalid)
        assert_eq!(
            format!(
                "{e:?}",
                e = value
                    .coerce(&mut engine, PrimitiveTypeKind::Directory.into())
                    .unwrap_err()
            ),
            "cannot coerce type `File` to type `Directory`"
        );
    }

    #[test]
    fn file_display() {
        let mut engine = Engine::default();

        let value = engine.new_file("/foo/bar/baz.txt");
        assert_eq!(value.display(&engine).to_string(), "\"/foo/bar/baz.txt\"");
    }

    #[test]
    fn directory_coercion() {
        let mut engine = Engine::default();

        let value = engine.new_directory("foo");
        let sym = match value {
            Value::Directory(sym) => sym,
            _ => panic!("expected a directory value"),
        };

        // Directory -> Directory
        assert_eq!(
            value
                .coerce(&mut engine, PrimitiveTypeKind::Directory.into())
                .expect("should coerce"),
            value
        );
        // Directory -> String
        assert_eq!(
            value
                .coerce(&mut engine, PrimitiveTypeKind::String.into())
                .expect("should coerce"),
            Value::String(sym)
        );
        // Directory -> File (invalid)
        assert_eq!(
            format!(
                "{e:?}",
                e = value
                    .coerce(&mut engine, PrimitiveTypeKind::File.into())
                    .unwrap_err()
            ),
            "cannot coerce type `Directory` to type `File`"
        );
    }

    #[test]
    fn directory_display() {
        let mut engine = Engine::default();

        let value = engine.new_file("/foo/bar/baz");
        assert_eq!(value.display(&engine).to_string(), "\"/foo/bar/baz\"");
    }

    #[test]
    fn none_coercion() {
        let mut engine = Engine::default();

        // None -> String?
        assert_eq!(
            Value::None
                .coerce(
                    &mut engine,
                    Type::from(PrimitiveTypeKind::String).optional()
                )
                .expect("should coerce"),
            Value::None
        );

        // None -> String (invalid)
        assert_eq!(
            format!(
                "{e:?}",
                e = Value::None
                    .coerce(&mut engine, PrimitiveTypeKind::String.into())
                    .unwrap_err()
            ),
            "cannot coerce `None` to non-optional type `String`"
        );
    }

    #[test]
    fn none_display() {
        let engine = Engine::default();

        assert_eq!(Value::None.display(&engine).to_string(), "None");
    }

    #[test]
    fn array_coercion() {
        let mut engine = Engine::default();

        let src_ty = engine
            .types_mut()
            .add_array(ArrayType::new(PrimitiveTypeKind::Integer));
        let target_ty = engine
            .types_mut()
            .add_array(ArrayType::new(PrimitiveTypeKind::Float));

        // Array[Int] -> Array[Float]
        let src = engine
            .new_array(src_ty, [1, 2, 3])
            .expect("should create array value");
        let target = src.coerce(&mut engine, target_ty).expect("should coerce");
        assert_eq!(target.unwrap_array(&engine).elements(), &[
            1.0.into(),
            2.0.into(),
            3.0.into()
        ]);

        // Array[Int] -> Array[String] (invalid)
        let target_ty = engine
            .types_mut()
            .add_array(ArrayType::new(PrimitiveTypeKind::String));
        assert_eq!(
            format!("{e:?}", e = src.coerce(&mut engine, target_ty).unwrap_err()),
            r#"failed to coerce array element at index 0

Caused by:
    cannot coerce type `Int` to type `String`"#
        );
    }

    #[test]
    fn non_empty_array_coercion() {
        let mut engine = Engine::default();

        let ty = engine
            .types_mut()
            .add_array(ArrayType::new(PrimitiveTypeKind::String));
        let target_ty = engine
            .types_mut()
            .add_array(ArrayType::non_empty(PrimitiveTypeKind::String));

        // Array[String] (non-empty) -> Array[String]+
        let string = engine.new_string("foo");
        let value = engine.new_array(ty, [string]).expect("should create array");
        assert!(
            value.coerce(&mut engine, target_ty).is_ok(),
            "should coerce"
        );

        // Array[String] (empty) -> Array[String]+ (invalid)
        let value = engine
            .new_array::<Value>(ty, [])
            .expect("should create array");
        assert_eq!(
            format!(
                "{e:?}",
                e = value.coerce(&mut engine, target_ty).unwrap_err()
            ),
            "cannot coerce empty array value to non-empty array type `Array[String]+`"
        );
    }

    #[test]
    fn array_display() {
        let mut engine = Engine::default();

        let ty = engine
            .types_mut()
            .add_array(ArrayType::new(PrimitiveTypeKind::Integer));
        let value = engine
            .new_array(ty, [1, 2, 3])
            .expect("should create array value");

        assert_eq!(value.display(&engine).to_string(), "[1, 2, 3]");
    }

    #[test]
    fn map_coerce() {
        let mut engine = Engine::default();

        let key1 = engine.new_file("foo");
        let value1 = engine.new_string("bar");
        let key2 = engine.new_file("baz");
        let value2 = engine.new_string("qux");

        let ty = engine.types_mut().add_map(MapType::new(
            PrimitiveTypeKind::File,
            PrimitiveTypeKind::String,
        ));
        let value = engine
            .new_map(ty, [(key1, value1), (key2, value2)])
            .expect("should create map value");

        // Map[File, String] -> Map[String, File]
        let ty = engine.types_mut().add_map(MapType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::File,
        ));
        let value = value.coerce(&mut engine, ty).expect("value should coerce");
        assert_eq!(
            value.display(&engine).to_string(),
            r#"{"foo": "bar", "baz": "qux"}"#
        );

        // Map[String, File] -> Map[Int, File] (invalid)
        let ty = engine.types_mut().add_map(MapType::new(
            PrimitiveTypeKind::Integer,
            PrimitiveTypeKind::File,
        ));
        assert_eq!(
            format!("{e:?}", e = value.coerce(&mut engine, ty).unwrap_err()),
            r#"failed to coerce map key at index 0

Caused by:
    cannot coerce type `String` to type `Int`"#
        );

        // Map[String, File] -> Struct
        let ty = engine.types_mut().add_struct(StructType::new("Foo", [
            ("foo", PrimitiveTypeKind::File),
            ("baz", PrimitiveTypeKind::File),
        ]));
        let struct_value = value.coerce(&mut engine, ty).expect("value should coerce");
        assert_eq!(
            struct_value.display(&engine).to_string(),
            r#"Foo {foo: "bar", baz: "qux"}"#
        );

        // Map[String, File] -> Struct (invalid)
        let ty = engine.types_mut().add_struct(StructType::new("Foo", [
            ("foo", PrimitiveTypeKind::File),
            ("baz", PrimitiveTypeKind::File),
            ("qux", PrimitiveTypeKind::File),
        ]));
        assert_eq!(
            format!("{e:?}", e = value.coerce(&mut engine, ty).unwrap_err()),
            "cannot coerce a map of 2 elements to struct type `Foo` as the struct has 3 members"
        );

        // Map[String, File] -> Object
        let object_value = value
            .coerce(&mut engine, Type::Object)
            .expect("value should coerce");
        assert_eq!(
            object_value.display(&engine).to_string(),
            r#"object {foo: "bar", baz: "qux"}"#
        );
    }

    #[test]
    fn map_display() {
        let mut engine = Engine::default();

        let ty = engine.types_mut().add_map(MapType::new(
            PrimitiveTypeKind::Integer,
            PrimitiveTypeKind::Boolean,
        ));

        let value = engine
            .new_map(ty, [(1, true), (2, false)])
            .expect("should create map value");
        assert_eq!(value.display(&engine).to_string(), "{1: true, 2: false}");
    }

    #[test]
    fn pair_coercion() {
        let mut engine = Engine::default();

        let left = engine.new_file("foo");
        let right = engine.new_string("bar");

        let ty = engine.types_mut().add_pair(PairType::new(
            PrimitiveTypeKind::File,
            PrimitiveTypeKind::String,
        ));
        let value = engine
            .new_pair(ty, left, right)
            .expect("should create map value");

        // Pair[File, String] -> Pair[String, File]
        let ty = engine.types_mut().add_pair(PairType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::File,
        ));
        let value = value.coerce(&mut engine, ty).expect("value should coerce");
        assert_eq!(value.display(&engine).to_string(), r#"("foo", "bar")"#);

        // Pair[String, File] -> Pair[Int, Int]
        let ty = engine.types_mut().add_pair(PairType::new(
            PrimitiveTypeKind::Integer,
            PrimitiveTypeKind::Integer,
        ));
        assert_eq!(
            format!("{e:?}", e = value.coerce(&mut engine, ty).unwrap_err()),
            r#"failed to coerce pair's left value

Caused by:
    cannot coerce type `String` to type `Int`"#
        );
    }

    #[test]
    fn pair_display() {
        let mut engine = Engine::default();

        let ty = engine.types_mut().add_pair(PairType::new(
            PrimitiveTypeKind::Integer,
            PrimitiveTypeKind::Boolean,
        ));

        let value = engine
            .new_pair(ty, 12345, false)
            .expect("should create pair value");
        assert_eq!(value.display(&engine).to_string(), "(12345, false)");
    }

    #[test]
    fn struct_coercion() {
        let mut engine = Engine::default();

        let ty = engine.types_mut().add_struct(StructType::new("Foo", [
            ("foo", PrimitiveTypeKind::Float),
            ("bar", PrimitiveTypeKind::Float),
            ("baz", PrimitiveTypeKind::Float),
        ]));
        let value = engine
            .new_struct(ty, [("foo", 1.0), ("bar", 2.0), ("baz", 3.0)])
            .expect("should create map value");

        // Struct -> Map[String, Float]
        let ty = engine.types_mut().add_map(MapType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::Float,
        ));
        let map_value = value.coerce(&mut engine, ty).expect("value should coerce");
        assert_eq!(
            map_value.display(&engine).to_string(),
            r#"{"foo": 1.0, "bar": 2.0, "baz": 3.0}"#
        );

        // Struct -> Struct
        let ty = engine.types_mut().add_struct(StructType::new("Bar", [
            ("foo", PrimitiveTypeKind::Float),
            ("bar", PrimitiveTypeKind::Float),
            ("baz", PrimitiveTypeKind::Float),
        ]));
        let struct_value = value.coerce(&mut engine, ty).expect("value should coerce");
        assert_eq!(
            struct_value.display(&engine).to_string(),
            r#"Bar {foo: 1.0, bar: 2.0, baz: 3.0}"#
        );

        // Struct -> Object
        let object_value = value
            .coerce(&mut engine, Type::Object)
            .expect("value should coerce");
        assert_eq!(
            object_value.display(&engine).to_string(),
            r#"object {foo: 1.0, bar: 2.0, baz: 3.0}"#
        );
    }

    #[test]
    fn struct_display() {}
}
