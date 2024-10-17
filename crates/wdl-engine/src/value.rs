//! Implementation of the WDL runtime and values.

use std::fmt;
use std::sync::Arc;

use id_arena::Id;
use indexmap::IndexMap;
use ordered_float::OrderedFloat;
use string_interner::symbol::SymbolU32;
use wdl_analysis::types::Coercible as _;
use wdl_analysis::types::CompoundTypeDef;
use wdl_analysis::types::CompoundTypeDefId;
use wdl_analysis::types::Optional;
use wdl_analysis::types::PrimitiveTypeKind;
use wdl_analysis::types::Type;
use wdl_analysis::types::Types;

use crate::Engine;

/// Implemented on coercible values.
pub trait Coercible: Sized {
    /// Coerces the value into the given type.
    ///
    /// Returns `None` if the coercion is not supported.
    fn coerce(&self, engine: &mut Engine, types: &Types, target: Type) -> Option<Self>;
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
    Compound(Type, CompoundValueId),
}

impl Value {
    /// Gets the type of the value.
    pub fn ty(&self) -> Type {
        match self {
            Self::Boolean(_) => PrimitiveTypeKind::Boolean.into(),
            Self::Integer(_) => PrimitiveTypeKind::Integer.into(),
            Self::Float(_) => PrimitiveTypeKind::Float.into(),
            Self::String(_) => PrimitiveTypeKind::String.into(),
            Self::File(_) => PrimitiveTypeKind::File.into(),
            Self::Directory(_) => PrimitiveTypeKind::Directory.into(),
            Self::None => Type::None,
            Self::Compound(ty, _) => *ty,
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
            Self::String(_) => self.as_str(engine),
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
            Self::String(_) => self.as_str(engine).expect("string should be interned"),
            _ => panic!("value is not a string"),
        }
    }

    /// Gets the value as a `File`.
    ///
    /// Returns `None` if the value is not a `File`.
    pub fn as_file(self, engine: &Engine) -> Option<&str> {
        match self {
            Self::File(_) => self.as_str(engine),
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
            Self::File(_) => self.as_str(engine).expect("string should be interned"),
            _ => panic!("value is not a file"),
        }
    }

    /// Gets the value as a `Directory`.
    ///
    /// Returns `None` if the value is not a `Directory`.
    pub fn as_directory(self, engine: &Engine) -> Option<&str> {
        match self {
            Self::Directory(_) => self.as_str(engine),
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
            Self::Directory(_) => self.as_str(engine).expect("string should be interned"),
            _ => panic!("value is not a directory"),
        }
    }

    /// Gets the value as an `Array`.
    ///
    /// Returns `None` if the value is not a `Array`.
    pub fn as_array(self, engine: &Engine) -> Option<&[Value]> {
        match self {
            Self::Compound(_, id) => match &engine.values[id] {
                CompoundValue::Array(elements) => Some(elements),
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
    pub fn unwrap_array(self, engine: &Engine) -> &[Value] {
        match self {
            Self::Compound(_, id) => match &engine.values[id] {
                CompoundValue::Array(elements) => elements,
                _ => panic!("value is not an array"),
            },
            _ => panic!("value is not an array"),
        }
    }

    /// Gets the string representation of a `String`, `File`, or `Directory`
    /// value.
    ///
    /// Returns `None` if the value is not a `String`, `File`, or `Directory`.
    pub fn as_str<'a>(&self, engine: &'a Engine) -> Option<&'a str> {
        match self {
            Self::String(sym) | Self::File(sym) | Self::Directory(sym) => {
                engine.interner.resolve(*sym)
            }
            _ => None,
        }
    }

    /// Used to display the value.
    pub fn display<'a>(&'a self, engine: &'a Engine, types: &'a Types) -> impl fmt::Display + 'a {
        /// Helper type for implementing display.
        struct Display<'a> {
            /// A reference to the engine.
            engine: &'a Engine,
            /// A reference to the types collection.
            types: &'a Types,
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
                                .as_str(self.engine)
                                .expect("string should be interned")
                        )
                    }
                    Value::None => write!(f, "None"),
                    Value::Compound(_, id) => {
                        write!(
                            f,
                            "{v}",
                            v = self.engine.values[id].display(self.engine, self.types)
                        )
                    }
                }
            }
        }

        Display {
            engine,
            types,
            value: *self,
        }
    }
}

impl Coercible for Value {
    fn coerce(&self, engine: &mut Engine, types: &Types, target: Type) -> Option<Self> {
        match self {
            Self::Boolean(v) => {
                match target.as_primitive()?.kind() {
                    // Boolean -> Boolean
                    PrimitiveTypeKind::Boolean => Some(Self::Boolean(*v)),
                    _ => None,
                }
            }
            Self::Integer(v) => {
                match target.as_primitive()?.kind() {
                    // Int -> Int
                    PrimitiveTypeKind::Integer => Some(Self::Integer(*v)),
                    // Int -> Float
                    PrimitiveTypeKind::Float => Some(Self::Float((*v as f64).into())),
                    _ => None,
                }
            }
            Self::Float(v) => {
                match target.as_primitive()?.kind() {
                    // Float -> Float
                    PrimitiveTypeKind::Float => Some(Self::Float(*v)),
                    _ => None,
                }
            }
            Self::String(sym) => {
                match target.as_primitive()?.kind() {
                    // String -> String
                    PrimitiveTypeKind::String => Some(Self::String(*sym)),
                    // String -> File
                    PrimitiveTypeKind::File => Some(Self::File(*sym)),
                    // String -> Directory
                    PrimitiveTypeKind::Directory => Some(Self::Directory(*sym)),
                    _ => None,
                }
            }
            Self::File(sym) => {
                match target.as_primitive()?.kind() {
                    // File -> File
                    PrimitiveTypeKind::File => Some(Self::File(*sym)),
                    // File -> String
                    PrimitiveTypeKind::String => Some(Self::String(*sym)),
                    _ => None,
                }
            }
            Self::Directory(sym) => {
                match target.as_primitive()?.kind() {
                    // Directory -> Directory
                    PrimitiveTypeKind::Directory => Some(Self::Directory(*sym)),
                    // Directory -> String
                    PrimitiveTypeKind::String => Some(Self::String(*sym)),
                    _ => None,
                }
            }
            Self::None => {
                if target.is_optional() {
                    Some(Self::None)
                } else {
                    None
                }
            }
            Self::Compound(ty, id) => {
                if ty.is_coercible_to(types, &target) {
                    let v = engine.values[*id].clone().coerce(engine, types, target)?;
                    let id = engine.values.alloc(v);
                    Some(Self::Compound(target, id))
                } else {
                    None
                }
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

/// Represents a compound value.
#[derive(Debug, Clone)]
pub enum CompoundValue {
    /// The value is a `Pair` of values.
    Pair(Value, Value),
    /// The value is an `Array` of values.
    Array(Arc<[Value]>),
    /// The value is a `Map` of values.
    Map(Arc<IndexMap<Value, Value>>),
    /// The value is an `Object.`
    Object(Arc<IndexMap<String, Value>>),
    /// The value is a struct.
    Struct(CompoundTypeDefId, Arc<IndexMap<String, Value>>),
}

impl CompoundValue {
    /// Used to display the value.
    pub fn display<'a>(&'a self, engine: &'a Engine, types: &'a Types) -> impl fmt::Display + 'a {
        /// Helper type for implementing display.
        struct Display<'a> {
            /// A reference to the engine.
            engine: &'a Engine,
            /// A reference to the types collection.
            types: &'a Types,
            /// The value to display.
            value: &'a CompoundValue,
        }

        impl fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self.value {
                    CompoundValue::Pair(left, right) => {
                        write!(
                            f,
                            "({left}, {right})",
                            left = left.display(self.engine, self.types),
                            right = right.display(self.engine, self.types)
                        )
                    }
                    CompoundValue::Array(elements) => {
                        write!(f, "[")?;

                        for (i, element) in elements.iter().enumerate() {
                            if i > 0 {
                                write!(f, ", ")?;
                            }

                            write!(
                                f,
                                "{element}",
                                element = element.display(self.engine, self.types)
                            )?;
                        }

                        write!(f, "]")
                    }
                    CompoundValue::Map(elements) => {
                        write!(f, "{{")?;

                        for (i, (k, v)) in elements.iter().enumerate() {
                            if i > 0 {
                                write!(f, ", ")?;
                            }

                            write!(
                                f,
                                "{k}: {v}",
                                k = k.display(self.engine, self.types),
                                v = v.display(self.engine, self.types)
                            )?;
                        }

                        write!(f, "}}")
                    }
                    CompoundValue::Object(elements) => {
                        write!(f, "object {{")?;

                        for (i, (k, v)) in elements.iter().enumerate() {
                            if i > 0 {
                                write!(f, ", ")?;
                            }

                            write!(f, "{k}: {v}", v = v.display(self.engine, self.types))?;
                        }

                        write!(f, "}}")
                    }
                    CompoundValue::Struct(id, members) => {
                        write!(
                            f,
                            "{name} {{",
                            name = self
                                .types
                                .type_definition(*id)
                                .as_struct()
                                .expect("should be a struct")
                                .name()
                        )?;

                        for (i, (k, v)) in members.iter().enumerate() {
                            if i > 0 {
                                write!(f, ", ")?;
                            }

                            write!(f, "{k}: {v}", v = v.display(self.engine, self.types))?;
                        }

                        write!(f, "}}")
                    }
                }
            }
        }

        Display {
            engine,
            types,
            value: self,
        }
    }
}

impl Coercible for CompoundValue {
    fn coerce(&self, engine: &mut Engine, types: &Types, target: Type) -> Option<Self> {
        if let Type::Compound(target) = target {
            let id = target.definition();
            return match (self, types.type_definition(id)) {
                // Array[X] -> Array[Y](+) where X -> Y
                (Self::Array(elements), CompoundTypeDef::Array(target)) => {
                    // Don't allow coercion when the source is empty but the target has the
                    // non-empty qualifier
                    if target.is_non_empty() && elements.is_empty() {
                        return None;
                    }

                    let element_type = target.element_type();
                    Some(Self::Array(
                        elements
                            .iter()
                            .map(|e| e.coerce(engine, types, element_type))
                            .collect::<Option<_>>()?,
                    ))
                }
                // Map[W, Y] -> Map[X, Z] where W -> X and Y -> Z
                (Self::Map(elements), CompoundTypeDef::Map(target)) => {
                    let key_type = target.key_type();
                    let value_type = target.value_type();
                    Some(Self::Map(Arc::new(
                        elements
                            .iter()
                            .map(|(k, v)| {
                                let k = k.coerce(engine, types, key_type);
                                let v = v.coerce(engine, types, value_type);
                                Some((k?, v?))
                            })
                            .collect::<Option<_>>()?,
                    )))
                }
                // Pair[W, Y] -> Pair[X, Z] where W -> X and Y -> Z
                (Self::Pair(left, right), CompoundTypeDef::Pair(target)) => {
                    let left_type = target.left_type();
                    let right_type = target.right_type();
                    let left = left.coerce(engine, types, left_type)?;
                    let right = right.coerce(engine, types, right_type)?;
                    Some(Self::Pair(left, right))
                }
                // Map[String, Y] -> Struct
                (Self::Map(elements), CompoundTypeDef::Struct(target)) => {
                    if elements.len() != target.members().len() {
                        return None;
                    }

                    let mut members = IndexMap::new();
                    for (name, ty) in target.members() {
                        let v = elements.get(&Value::String(engine.interner.get(name)?))?;
                        members.insert(name.clone(), v.coerce(engine, types, *ty)?);
                    }

                    Some(Self::Struct(id, Arc::new(members)))
                }
                // Struct -> Map[String, Y]
                // Object -> Map[String, Y]
                (Self::Struct(_, elements), CompoundTypeDef::Map(target))
                | (Self::Object(elements), CompoundTypeDef::Map(target)) => {
                    if target.key_type().as_primitive() != Some(PrimitiveTypeKind::String.into()) {
                        return None;
                    }

                    let value_ty = target.value_type();
                    Some(Self::Map(Arc::new(
                        elements
                            .iter()
                            .map(|(n, v)| {
                                let v = v.coerce(engine, types, value_ty)?;
                                Some((engine.new_string(n), v))
                            })
                            .collect::<Option<_>>()?,
                    )))
                }
                // Object -> Struct
                (Self::Object(elements), CompoundTypeDef::Struct(target)) => {
                    if elements.len() != target.members().len() {
                        return None;
                    }

                    Some(Self::Struct(
                        id,
                        Arc::new(
                            elements
                                .iter()
                                .map(|(k, v)| {
                                    let ty = target.members().get(k)?;
                                    let v = v.coerce(engine, types, *ty)?;
                                    Some((k.clone(), v))
                                })
                                .collect::<Option<_>>()?,
                        ),
                    ))
                }
                // Struct -> Struct
                (Self::Struct(_, members), CompoundTypeDef::Struct(target)) => {
                    if members.len() != target.members().len() {
                        return None;
                    }

                    Some(Self::Struct(
                        id,
                        Arc::new(
                            members
                                .iter()
                                .map(|(k, v)| {
                                    let ty = target.members().get(k)?;
                                    let v = v.coerce(engine, types, *ty)?;
                                    Some((k.clone(), v))
                                })
                                .collect::<Option<_>>()?,
                        ),
                    ))
                }
                _ => None,
            };
        }

        if let Type::Object = target {
            return match self {
                // Map[String, Y] -> Object
                Self::Map(elements) => Some(Self::Object(Arc::new(
                    elements
                        .iter()
                        .map(|(k, v)| {
                            let k = k.as_string(engine)?.to_string();
                            Some((k, *v))
                        })
                        .collect::<Option<_>>()?,
                ))),
                // Struct -> Object
                Self::Struct(_, elements) => Some(Self::Object(elements.clone())),
                _ => None,
            };
        }

        None
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
        let types = Types::default();

        // Boolean -> Boolean
        assert_eq!(
            Value::from(false).coerce(&mut engine, &types, PrimitiveTypeKind::Boolean.into()),
            Some(Value::from(false))
        );
        // Boolean -> String (invalid)
        assert_eq!(
            Value::from(true).coerce(&mut engine, &types, PrimitiveTypeKind::String.into()),
            None
        );
    }

    #[test]
    fn boolean_display() {
        let engine = Engine::default();
        let types = Types::default();

        assert_eq!(
            Value::from(false).display(&engine, &types).to_string(),
            "false"
        );
        assert_eq!(
            Value::from(true).display(&engine, &types).to_string(),
            "true"
        );
    }

    #[test]
    fn integer_coercion() {
        let mut engine = Engine::default();
        let types = Types::default();

        // Int -> Int
        assert_eq!(
            Value::from(12345).coerce(&mut engine, &types, PrimitiveTypeKind::Integer.into()),
            Some(Value::from(12345))
        );
        // Int -> Float
        assert_eq!(
            Value::from(12345).coerce(&mut engine, &types, PrimitiveTypeKind::Float.into()),
            Some(Value::from(12345.0))
        );
        // Int -> Boolean (invalid)
        assert_eq!(
            Value::from(12345).coerce(&mut engine, &types, PrimitiveTypeKind::Boolean.into()),
            None
        );
    }

    #[test]
    fn integer_display() {
        let engine = Engine::default();
        let types = Types::default();

        assert_eq!(
            Value::from(12345).display(&engine, &types).to_string(),
            "12345"
        );
        assert_eq!(
            Value::from(-12345).display(&engine, &types).to_string(),
            "-12345"
        );
    }

    #[test]
    fn float_coercion() {
        let mut engine = Engine::default();
        let types = Types::default();

        // Float -> Float
        assert_eq!(
            Value::from(12345.0).coerce(&mut engine, &types, PrimitiveTypeKind::Float.into()),
            Some(Value::from(12345.0))
        );
        // Float -> Int (invalid)
        assert_eq!(
            Value::from(12345.0).coerce(&mut engine, &types, PrimitiveTypeKind::Integer.into()),
            None
        );
    }

    #[test]
    fn float_display() {
        let engine = Engine::default();
        let types = Types::default();

        assert_eq!(
            Value::from(12345.12345)
                .display(&engine, &types)
                .to_string(),
            "12345.12345"
        );
        assert_eq!(
            Value::from(-12345.12345)
                .display(&engine, &types)
                .to_string(),
            "-12345.12345"
        );
    }

    #[test]
    fn string_coercion() {
        let mut engine = Engine::default();
        let types = Types::default();

        let value = engine.new_string("foo");
        let sym = match value {
            Value::String(sym) => sym,
            _ => panic!("expected a string value"),
        };

        // String -> String
        assert_eq!(
            value.coerce(&mut engine, &types, PrimitiveTypeKind::String.into()),
            Some(value)
        );
        // String -> File
        assert_eq!(
            value.coerce(&mut engine, &types, PrimitiveTypeKind::File.into()),
            Some(Value::File(sym))
        );
        // String -> Directory
        assert_eq!(
            value.coerce(&mut engine, &types, PrimitiveTypeKind::Directory.into()),
            Some(Value::Directory(sym))
        );
        // String -> Boolean (invalid)
        assert_eq!(
            value.coerce(&mut engine, &types, PrimitiveTypeKind::Boolean.into()),
            None
        );
    }

    #[test]
    fn string_display() {
        let mut engine = Engine::default();
        let types = Types::default();

        let value = engine.new_string("hello world!");
        assert_eq!(
            value.display(&engine, &types).to_string(),
            "\"hello world!\""
        );
    }

    #[test]
    fn file_coercion() {
        let mut engine = Engine::default();
        let types = Types::default();

        let value = engine.new_file("foo");
        let sym = match value {
            Value::File(sym) => sym,
            _ => panic!("expected a file value"),
        };

        // File -> File
        assert_eq!(
            value.coerce(&mut engine, &types, PrimitiveTypeKind::File.into()),
            Some(value)
        );
        // File -> String
        assert_eq!(
            value.coerce(&mut engine, &types, PrimitiveTypeKind::String.into()),
            Some(Value::String(sym))
        );
        // File -> Directory (invalid)
        assert_eq!(
            value.coerce(&mut engine, &types, PrimitiveTypeKind::Directory.into()),
            None
        );
    }

    #[test]
    fn file_display() {
        let mut engine = Engine::default();
        let types = Types::default();

        let value = engine.new_file("/foo/bar/baz.txt");
        assert_eq!(
            value.display(&engine, &types).to_string(),
            "\"/foo/bar/baz.txt\""
        );
    }

    #[test]
    fn directory_coercion() {
        let mut engine = Engine::default();
        let types = Types::default();

        let value = engine.new_directory("foo");
        let sym = match value {
            Value::Directory(sym) => sym,
            _ => panic!("expected a directory value"),
        };

        // Directory -> Directory
        assert_eq!(
            value.coerce(&mut engine, &types, PrimitiveTypeKind::Directory.into()),
            Some(value)
        );
        // Directory -> String
        assert_eq!(
            value.coerce(&mut engine, &types, PrimitiveTypeKind::String.into()),
            Some(Value::String(sym))
        );
        // Directory -> File (invalid)
        assert_eq!(
            value.coerce(&mut engine, &types, PrimitiveTypeKind::File.into()),
            None
        );
    }

    #[test]
    fn directory_display() {
        let mut engine = Engine::default();
        let types = Types::default();

        let value = engine.new_file("/foo/bar/baz");
        assert_eq!(
            value.display(&engine, &types).to_string(),
            "\"/foo/bar/baz\""
        );
    }

    #[test]
    fn none_coercion() {
        let mut engine = Engine::default();
        let types = Types::default();

        // None -> String?
        assert_eq!(
            Value::None.coerce(
                &mut engine,
                &types,
                Type::from(PrimitiveTypeKind::String).optional()
            ),
            Some(Value::None)
        );

        // None -> String (invalid)
        assert_eq!(
            Value::None.coerce(&mut engine, &types, PrimitiveTypeKind::String.into()),
            None
        );
    }

    #[test]
    fn none_display() {
        let engine = Engine::default();
        let types = Types::default();

        assert_eq!(Value::None.display(&engine, &types).to_string(), "None");
    }

    #[test]
    fn array_coercion() {
        let mut engine = Engine::default();
        let mut types = Types::default();

        let src_ty = types.add_array(ArrayType::new(PrimitiveTypeKind::Integer));
        let target_ty = types.add_array(ArrayType::new(PrimitiveTypeKind::Float));

        // Array[Int] -> Array[Float]
        let src = engine
            .new_array(&types, src_ty, [1, 2, 3])
            .expect("should create array value");
        let target = src
            .coerce(&mut engine, &types, target_ty)
            .expect("should coerce");
        assert_eq!(target.unwrap_array(&engine), &[
            1.0.into(),
            2.0.into(),
            3.0.into()
        ]);

        // Array[Int] -> Array[String] (invalid)
        let target_ty = types.add_array(ArrayType::new(PrimitiveTypeKind::String));
        assert!(
            src.coerce(&mut engine, &types, target_ty).is_none(),
            "should not coerce"
        );
    }

    #[test]
    fn non_empty_array_coercion() {
        let mut engine = Engine::default();
        let mut types = Types::default();

        let ty = types.add_array(ArrayType::new(PrimitiveTypeKind::String));
        let target_ty = types.add_array(ArrayType::non_empty(PrimitiveTypeKind::String));

        // Array[String] (non-empty) -> Array[String]+
        let string = engine.new_string("foo");
        let value = engine
            .new_array(&types, ty, [string])
            .expect("should create array");
        assert!(
            value.coerce(&mut engine, &types, target_ty).is_some(),
            "should coerce"
        );

        // Array[String] (empty) -> Array[String]+ (invalid)
        let value = engine.new_empty_array(&types, ty);
        assert!(
            value.coerce(&mut engine, &types, target_ty).is_none(),
            "should not coerce"
        );
    }

    #[test]
    fn array_display() {
        let mut engine = Engine::default();
        let mut types = Types::default();

        let ty = types.add_array(ArrayType::new(PrimitiveTypeKind::Integer));
        let value = engine
            .new_array(&types, ty, [1, 2, 3])
            .expect("should create array value");

        assert_eq!(value.display(&engine, &types).to_string(), "[1, 2, 3]");
    }

    #[test]
    fn map_coerce() {
        let mut engine = Engine::default();
        let mut types = Types::default();

        let key1 = engine.new_file("foo");
        let value1 = engine.new_string("bar");
        let key2 = engine.new_file("baz");
        let value2 = engine.new_string("qux");

        let ty = types.add_map(MapType::new(
            PrimitiveTypeKind::File,
            PrimitiveTypeKind::String,
        ));
        let value = engine
            .new_map(&types, ty, [(key1, value1), (key2, value2)])
            .expect("should create map value");

        // Map[File, String] -> Map[String, File]
        let ty = types.add_map(MapType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::File,
        ));
        let value = value
            .coerce(&mut engine, &types, ty)
            .expect("value should coerce");
        assert_eq!(
            value.display(&engine, &types).to_string(),
            r#"{"foo": "bar", "baz": "qux"}"#
        );

        // Map[String, File] -> Map[Int, File] (invalid)
        let ty = types.add_map(MapType::new(
            PrimitiveTypeKind::Integer,
            PrimitiveTypeKind::File,
        ));
        assert!(
            value.coerce(&mut engine, &types, ty).is_none(),
            "value should not coerce"
        );

        // Map[String, File] -> Struct
        let ty = types.add_struct(StructType::new("Foo", [
            ("foo", PrimitiveTypeKind::File),
            ("baz", PrimitiveTypeKind::File),
        ]));
        let struct_value = value
            .coerce(&mut engine, &types, ty)
            .expect("value should coerce");
        assert_eq!(
            struct_value.display(&engine, &types).to_string(),
            r#"Foo {foo: "bar", baz: "qux"}"#
        );

        // Map[String, File] -> Struct (invalid)
        let ty = types.add_struct(StructType::new("Foo", [
            ("foo", PrimitiveTypeKind::File),
            ("baz", PrimitiveTypeKind::File),
            ("qux", PrimitiveTypeKind::File),
        ]));
        assert!(value.coerce(&mut engine, &types, ty).is_none());

        // Map[String, File] -> Object
        let object_value = value
            .coerce(&mut engine, &types, Type::Object)
            .expect("value should coerce");
        assert_eq!(
            object_value.display(&engine, &types).to_string(),
            r#"object {foo: "bar", baz: "qux"}"#
        );
    }

    #[test]
    fn map_display() {
        let mut engine = Engine::default();
        let mut types = Types::default();

        let ty = types.add_map(MapType::new(
            PrimitiveTypeKind::Integer,
            PrimitiveTypeKind::Boolean,
        ));

        let value = engine
            .new_map(&types, ty, [(1, true), (2, false)])
            .expect("should create map value");
        assert_eq!(
            value.display(&engine, &types).to_string(),
            "{1: true, 2: false}"
        );
    }

    #[test]
    fn pair_coercion() {
        let mut engine = Engine::default();
        let mut types = Types::default();

        let left = engine.new_file("foo");
        let right = engine.new_string("bar");

        let ty = types.add_pair(PairType::new(
            PrimitiveTypeKind::File,
            PrimitiveTypeKind::String,
        ));
        let value = engine
            .new_pair(&types, ty, left, right)
            .expect("should create map value");

        // Pair[File, String] -> Pair[String, File]
        let ty = types.add_pair(PairType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::File,
        ));
        let value = value
            .coerce(&mut engine, &types, ty)
            .expect("value should coerce");
        assert_eq!(
            value.display(&engine, &types).to_string(),
            r#"("foo", "bar")"#
        );

        // Pair[String, File] -> Pair[Int, Int]
        let ty = types.add_pair(PairType::new(
            PrimitiveTypeKind::Integer,
            PrimitiveTypeKind::Integer,
        ));
        assert!(
            value.coerce(&mut engine, &types, ty).is_none(),
            "value should not coerce"
        );
    }

    #[test]
    fn pair_display() {
        let mut engine = Engine::default();
        let mut types = Types::default();

        let ty = types.add_pair(PairType::new(
            PrimitiveTypeKind::Integer,
            PrimitiveTypeKind::Boolean,
        ));

        let value = engine
            .new_pair(&types, ty, 12345, false)
            .expect("should create pair value");
        assert_eq!(value.display(&engine, &types).to_string(), "(12345, false)");
    }

    #[test]
    fn struct_coercion() {
        let mut engine = Engine::default();
        let mut types = Types::default();

        let ty = types.add_struct(StructType::new("Foo", [
            ("foo", PrimitiveTypeKind::Float),
            ("bar", PrimitiveTypeKind::Float),
            ("baz", PrimitiveTypeKind::Float),
        ]));
        let value = engine
            .new_struct(&types, ty, [("foo", 1.0), ("bar", 2.0), ("baz", 3.0)])
            .expect("should create map value");

        // Struct -> Map[String, Float]
        let ty = types.add_map(MapType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::Float,
        ));
        let map_value = value
            .coerce(&mut engine, &types, ty)
            .expect("value should coerce");
        assert_eq!(
            map_value.display(&engine, &types).to_string(),
            r#"{"foo": 1.0, "bar": 2.0, "baz": 3.0}"#
        );

        // Struct -> Struct
        let ty = types.add_struct(StructType::new("Bar", [
            ("foo", PrimitiveTypeKind::Float),
            ("bar", PrimitiveTypeKind::Float),
            ("baz", PrimitiveTypeKind::Float),
        ]));
        let struct_value = value
            .coerce(&mut engine, &types, ty)
            .expect("value should coerce");
        assert_eq!(
            struct_value.display(&engine, &types).to_string(),
            r#"Bar {foo: 1.0, bar: 2.0, baz: 3.0}"#
        );

        // Struct -> Object
        let object_value = value
            .coerce(&mut engine, &types, Type::Object)
            .expect("value should coerce");
        assert_eq!(
            object_value.display(&engine, &types).to_string(),
            r#"object {foo: 1.0, bar: 2.0, baz: 3.0}"#
        );
    }

    #[test]
    fn struct_display() {}
}
