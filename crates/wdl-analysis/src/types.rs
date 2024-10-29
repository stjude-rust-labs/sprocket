//! Representation of the WDL type system.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use id_arena::Arena;
use id_arena::ArenaBehavior;
use id_arena::DefaultArenaBehavior;
use id_arena::Id;
use indexmap::IndexMap;

use crate::document::Input;
use crate::document::Output;
use crate::stdlib::STDLIB;

pub mod v1;

/// A trait implemented on types that may be optional.
pub trait Optional: Copy {
    /// Determines if the type is optional.
    fn is_optional(&self) -> bool;

    /// Makes the type optional if it isn't already optional.
    fn optional(&self) -> Self;

    /// Makes the type required if it isn't already required.
    fn require(&self) -> Self;
}

/// A trait implemented on types that are coercible to other types.
pub trait Coercible {
    /// Determines if the type is coercible to the target type.
    fn is_coercible_to(&self, types: &Types, target: &Self) -> bool;
}

/// A trait implement on types for type equality.
///
/// This is similar to `Eq` except it supports recursive types.
pub trait TypeEq {
    /// Determines if the two types are equal.
    fn type_eq(&self, types: &Types, other: &Self) -> bool;
}

/// Represents a kind of primitive WDL type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimitiveTypeKind {
    /// The type is a `Boolean`.
    Boolean,
    /// The type is an `Int`.
    Integer,
    /// The type is a `Float`.
    Float,
    /// The type is a `String`.
    String,
    /// The type is a `File`.
    File,
    /// The type is a `Directory`.
    Directory,
}

impl Coercible for PrimitiveTypeKind {
    fn is_coercible_to(&self, _: &Types, target: &Self) -> bool {
        if self == target {
            return true;
        }

        match (self, target) {
            // String -> File
            (Self::String, Self::File) |
            // String -> Directory
            (Self::String, Self::Directory) |
            // Int -> Float
            (Self::Integer, Self::Float) |
            // File -> String
            (Self::File, Self::String) |
            // Directory -> String
            (Self::Directory, Self::String)
            => true,

            // Not coercible
            _ => false
        }
    }
}

/// Represents a primitive WDL type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PrimitiveType {
    /// The kind of primitive type.
    kind: PrimitiveTypeKind,
    /// Whether or not the primitive type is optional.
    optional: bool,
}

impl PrimitiveType {
    /// Constructs a new primitive type.
    pub fn new(kind: PrimitiveTypeKind) -> Self {
        Self {
            kind,
            optional: false,
        }
    }

    /// Constructs a new optional primitive type.
    pub fn optional(kind: PrimitiveTypeKind) -> Self {
        Self {
            kind,
            optional: true,
        }
    }

    /// Gets the kind of primitive type.
    pub fn kind(&self) -> PrimitiveTypeKind {
        self.kind
    }
}

impl Optional for PrimitiveType {
    fn is_optional(&self) -> bool {
        self.optional
    }

    fn optional(&self) -> Self {
        Self {
            kind: self.kind,
            optional: true,
        }
    }

    fn require(&self) -> Self {
        Self {
            kind: self.kind,
            optional: false,
        }
    }
}

impl Coercible for PrimitiveType {
    fn is_coercible_to(&self, types: &Types, target: &Self) -> bool {
        // An optional type cannot coerce into a required type
        if self.optional && !target.optional {
            return false;
        }

        self.kind.is_coercible_to(types, &target.kind)
    }
}

impl TypeEq for PrimitiveType {
    fn type_eq(&self, _: &Types, other: &Self) -> bool {
        self == other
    }
}

impl fmt::Display for PrimitiveType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            PrimitiveTypeKind::Boolean => write!(f, "Boolean")?,
            PrimitiveTypeKind::Integer => write!(f, "Int")?,
            PrimitiveTypeKind::Float => write!(f, "Float")?,
            PrimitiveTypeKind::String => write!(f, "String")?,
            PrimitiveTypeKind::File => write!(f, "File")?,
            PrimitiveTypeKind::Directory => write!(f, "Directory")?,
        }

        if self.optional {
            write!(f, "?")?;
        }

        Ok(())
    }
}

impl From<PrimitiveTypeKind> for PrimitiveType {
    fn from(value: PrimitiveTypeKind) -> Self {
        Self {
            kind: value,
            optional: false,
        }
    }
}

/// Represents an identifier of a defined compound type.
pub type CompoundTypeDefId = Id<CompoundTypeDef>;

/// Represents the kind of a promotion of a type from one scope to another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PromotionKind {
    /// The type is being promoted as an output of a scatter statement.
    Scatter,
    /// The type is being promoted as an output of a conditional statement.
    Conditional,
}

/// Represents a WDL type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Type {
    /// The type is a primitive type.
    Primitive(PrimitiveType),
    /// The type is a compound type.
    Compound(CompoundType),
    /// The type is `Object`.
    Object,
    /// The type is `Object?`.
    OptionalObject,
    /// A special hidden type for a value that may have any one of several
    /// concrete types.
    ///
    /// This variant is also used to convey an "indeterminate" type; an
    /// indeterminate type may result from a previous type error.
    Union,
    /// A special type that behaves like an optional `Union`.
    None,
    /// A special hidden type for `task` that is available in command and task
    /// output sections in WDL 1.2.
    Task,
    /// A special hidden type for `hints` that is available in task hints
    /// sections.
    Hints,
    /// A special hidden type for `input` that is available in task hints
    /// sections.
    Input,
    /// A special hidden type for `output` that is available in task hints
    /// sections.
    Output,
}

impl Type {
    /// Casts the type to a primitive type.
    ///
    /// Returns `None` if the type is not primitive.
    pub fn as_primitive(&self) -> Option<PrimitiveType> {
        match self {
            Self::Primitive(ty) => Some(*ty),
            _ => None,
        }
    }

    /// Determines if the type is `Union`.
    pub fn is_union(&self) -> bool {
        matches!(self, Type::Union)
    }

    /// Determines if the type is `None`.
    pub fn is_none(&self) -> bool {
        matches!(self, Type::None)
    }

    /// Promotes the type from one scope to another.
    pub fn promote(&self, types: &mut Types, kind: PromotionKind) -> Self {
        // For calls, the outputs of the call are promoted instead of the call itself
        if let Type::Compound(ty) = self {
            if let CompoundTypeDef::Call(ty) = types.type_definition(ty.definition()) {
                let mut ty = ty.clone();
                for output in Arc::make_mut(&mut ty.outputs).values_mut() {
                    *output = Output::new(output.ty().promote(types, kind));
                }

                return types.add_call(ty);
            }
        }

        match kind {
            PromotionKind::Scatter => types.add_array(ArrayType::new(*self)),
            PromotionKind::Conditional => self.optional(),
        }
    }

    /// Returns an object that implements `Display` for formatting the type.
    pub fn display<'a>(&self, types: &'a Types) -> impl fmt::Display + use<'a> {
        #[allow(clippy::missing_docs_in_private_items)]
        struct Display<'a> {
            types: &'a Types,
            ty: Type,
        }

        impl fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self.ty {
                    Type::Primitive(ty) => ty.fmt(f),
                    Type::Compound(ty) => ty.display(self.types).fmt(f),
                    Type::Object => write!(f, "Object"),
                    Type::OptionalObject => write!(f, "Object?"),
                    Type::Union => write!(f, "Union"),
                    Type::None => write!(f, "None"),
                    Type::Task => write!(f, "task"),
                    Type::Hints => write!(f, "hints"),
                    Type::Input => write!(f, "input"),
                    Type::Output => write!(f, "output"),
                }
            }
        }

        Display { types, ty: *self }
    }

    /// Asserts that the type is valid.
    fn assert_valid(&self, types: &Types) {
        match self {
            Self::Compound(ty) => {
                let arena_id = DefaultArenaBehavior::arena_id(ty.definition());
                assert!(
                    arena_id == DefaultArenaBehavior::arena_id(types.0.next_id())
                        || arena_id == DefaultArenaBehavior::arena_id(STDLIB.types().0.next_id()),
                    "type comes from a different arena"
                );
                ty.assert_valid(types);
            }
            Self::Primitive(_)
            | Self::Object
            | Self::OptionalObject
            | Self::Union
            | Self::None
            | Self::Task
            | Self::Hints
            | Self::Input
            | Self::Output => {}
        }
    }
}

impl Optional for Type {
    fn is_optional(&self) -> bool {
        match self {
            Self::Primitive(ty) => ty.is_optional(),
            Self::Compound(ty) => ty.is_optional(),
            Self::OptionalObject | Self::None => true,
            Self::Object | Self::Union | Self::Task | Self::Hints | Self::Input | Self::Output => {
                false
            }
        }
    }

    fn optional(&self) -> Self {
        match self {
            Self::Primitive(ty) => Self::Primitive(ty.optional()),
            Self::Compound(ty) => Self::Compound(ty.optional()),
            Self::Object | Self::OptionalObject => Self::OptionalObject,
            Self::Union | Self::None | Self::Task | Self::Hints | Self::Input | Self::Output => {
                Self::None
            }
        }
    }

    fn require(&self) -> Self {
        match self {
            Self::Primitive(ty) => Self::Primitive(ty.require()),
            Self::Compound(ty) => Self::Compound(ty.require()),
            Self::Object | Self::OptionalObject => Self::Object,
            Self::Union | Self::None => Self::Union,
            Self::Task => Self::Task,
            Self::Hints => Self::Hints,
            Self::Input => Self::Input,
            Self::Output => Self::Output,
        }
    }
}

impl Coercible for Type {
    fn is_coercible_to(&self, types: &Types, target: &Self) -> bool {
        if self == target {
            return true;
        }

        match (self, target) {
            (Self::Primitive(src), Self::Primitive(target)) => src.is_coercible_to(types, target),
            (Self::Compound(src), Self::Compound(target)) => src.is_coercible_to(types, target),

            // Object -> Object, Object -> Object?, Object? -> Object?
            (Self::Object, Self::Object)
            | (Self::Object, Self::OptionalObject)
            | (Self::OptionalObject, Self::OptionalObject) => true,

            // Map[String, X] -> Object, Map[String, X] -> Object?, Map[String, X]? -> Object?
            // Struct -> Object, Struct -> Object?, Struct? -> Object?
            (Self::Compound(src), Self::Object) | (Self::Compound(src), Self::OptionalObject) => {
                if src.is_optional() && *target == Self::Object {
                    return false;
                }

                match types.type_definition(src.definition) {
                    CompoundTypeDef::Map(src) => {
                        if !matches!(src.key_type, Type::Primitive(ty) if ty.kind() == PrimitiveTypeKind::String)
                        {
                            return false;
                        }

                        true
                    }
                    CompoundTypeDef::Struct(_) => true,
                    _ => false,
                }
            }

            // Object -> Map[String, X], Object -> Map[String, X]?, Object? -> Map[String, X]? (if
            // all object members are coercible to X)
            // Object -> Struct, Object -> Struct?, Object? -> Struct? (if object keys match struct
            // member names and object values must be coercible to struct member types)
            (Self::Object, Self::Compound(target))
            | (Self::OptionalObject, Self::Compound(target)) => {
                if *self == Self::OptionalObject && !target.is_optional() {
                    return false;
                }

                match types.type_definition(target.definition) {
                    CompoundTypeDef::Map(target) => {
                        if !matches!(target.key_type, Type::Primitive(ty) if ty.kind() == PrimitiveTypeKind::String)
                        {
                            return false;
                        }

                        // Note: checking object members is a runtime value constraint
                        true
                    }
                    CompoundTypeDef::Struct(_) => {
                        // Note: checking object keys and values is a runtime constraint
                        true
                    }
                    _ => false,
                }
            }

            // Union is always coercible to the target
            (Self::Union, _) => true,

            // None is coercible to an optional type
            (Self::None, ty) if ty.is_optional() => true,

            // Not coercible
            _ => false,
        }
    }
}

impl TypeEq for Type {
    fn type_eq(&self, types: &Types, other: &Self) -> bool {
        if self == other {
            return true;
        }

        match (self, other) {
            (Self::Primitive(a), Self::Primitive(b)) => a.type_eq(types, b),
            (Self::Compound(a), Self::Compound(b)) => a.type_eq(types, b),
            _ => false,
        }
    }
}

impl From<PrimitiveTypeKind> for Type {
    fn from(value: PrimitiveTypeKind) -> Self {
        Self::Primitive(PrimitiveType::new(value))
    }
}

impl From<PrimitiveType> for Type {
    fn from(value: PrimitiveType) -> Self {
        Self::Primitive(value)
    }
}

/// Represents a compound type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CompoundType {
    /// The definition identifier for the compound type.
    definition: CompoundTypeDefId,
    /// Whether or not the type is optional.
    optional: bool,
}

impl CompoundType {
    /// Gets the definition identifier of the compound type.
    pub fn definition(&self) -> CompoundTypeDefId {
        self.definition
    }

    /// Returns an object that implements `Display` for formatting the type.
    pub fn display<'a>(&self, types: &'a Types) -> impl fmt::Display + use<'a> {
        #[allow(clippy::missing_docs_in_private_items)]
        struct Display<'a> {
            types: &'a Types,
            ty: &'a CompoundTypeDef,
            optional: bool,
        }

        impl fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self.ty {
                    CompoundTypeDef::Array(ty) => ty.display(self.types).fmt(f)?,
                    CompoundTypeDef::Pair(ty) => ty.display(self.types).fmt(f)?,
                    CompoundTypeDef::Map(ty) => ty.display(self.types).fmt(f)?,
                    CompoundTypeDef::Struct(ty) => ty.fmt(f)?,
                    CompoundTypeDef::Call(ty) => ty.fmt(f)?,
                }

                if self.optional {
                    write!(f, "?")?;
                }

                Ok(())
            }
        }

        Display {
            types,
            ty: types.type_definition(self.definition),
            optional: self.optional,
        }
    }

    /// Asserts that the type is valid.
    fn assert_valid(&self, types: &Types) {
        types.type_definition(self.definition).assert_valid(types);
    }
}

impl Optional for CompoundType {
    fn is_optional(&self) -> bool {
        self.optional
    }

    fn optional(&self) -> Self {
        Self {
            definition: self.definition,
            optional: true,
        }
    }

    fn require(&self) -> Self {
        Self {
            definition: self.definition,
            optional: false,
        }
    }
}

impl Coercible for CompoundType {
    fn is_coercible_to(&self, types: &Types, target: &Self) -> bool {
        if self.is_optional() && !target.is_optional() {
            return false;
        }

        types
            .type_definition(self.definition)
            .is_coercible_to(types, types.type_definition(target.definition))
    }
}

impl TypeEq for CompoundType {
    fn type_eq(&self, types: &Types, other: &Self) -> bool {
        if self.optional != other.optional {
            return false;
        }

        if self.definition == other.definition {
            return true;
        }

        match (
            types.type_definition(self.definition),
            types.type_definition(other.definition),
        ) {
            (CompoundTypeDef::Array(a), CompoundTypeDef::Array(b)) => a.type_eq(types, b),
            (CompoundTypeDef::Pair(a), CompoundTypeDef::Pair(b)) => a.type_eq(types, b),
            (CompoundTypeDef::Map(a), CompoundTypeDef::Map(b)) => a.type_eq(types, b),
            (CompoundTypeDef::Struct(_), CompoundTypeDef::Struct(_)) => {
                // Struct types are only equivalent if they're the same definition
                false
            }
            _ => false,
        }
    }
}

/// Represents a compound type definition.
#[derive(Debug)]
pub enum CompoundTypeDef {
    /// The type is an `Array`.
    Array(ArrayType),
    /// The type is a `Pair`.
    Pair(PairType),
    /// The type is a `Map`.
    Map(MapType),
    /// The type is a struct (e.g. `Foo`).
    Struct(StructType),
    /// The type is a call.
    Call(CallType),
}

impl CompoundTypeDef {
    /// Asserts that this type is valid.
    fn assert_valid(&self, types: &Types) {
        match self {
            Self::Array(ty) => {
                ty.assert_valid(types);
            }
            Self::Pair(ty) => {
                ty.assert_valid(types);
            }
            Self::Map(ty) => {
                ty.assert_valid(types);
            }
            Self::Struct(ty) => {
                ty.assert_valid(types);
            }
            Self::Call(ty) => {
                ty.assert_valid(types);
            }
        }
    }

    /// Converts the compound type to an array type.
    ///
    /// Returns `None` if the compound type is not an array type.
    pub fn as_array(&self) -> Option<&ArrayType> {
        match self {
            Self::Array(ty) => Some(ty),
            _ => None,
        }
    }

    /// Converts the compound type to a pair type.
    ///
    /// Returns `None` if the compound type is not a pair type.
    pub fn as_pair(&self) -> Option<&PairType> {
        match self {
            Self::Pair(ty) => Some(ty),
            _ => None,
        }
    }

    /// Converts the compound type to a map type.
    ///
    /// Returns `None` if the compound type is not a map type.
    pub fn as_map(&self) -> Option<&MapType> {
        match self {
            Self::Map(ty) => Some(ty),
            _ => None,
        }
    }

    /// Converts the compound type to a struct type.
    ///
    /// Returns `None` if the compound type is not a struct type.
    pub fn as_struct(&self) -> Option<&StructType> {
        match self {
            Self::Struct(ty) => Some(ty),
            _ => None,
        }
    }
}

impl Coercible for CompoundTypeDef {
    fn is_coercible_to(&self, types: &Types, target: &Self) -> bool {
        match (self, target) {
            // Array[X] -> Array[Y], Array[X] -> Array[Y]?, Array[X]? -> Array[Y]?, Array[X]+ ->
            // Array[Y] (if X is coercible to Y)
            (Self::Array(src), Self::Array(target)) => src.is_coercible_to(types, target),

            // Pair[W, X] -> Pair[Y, Z], Pair[W, X] -> Pair[Y, Z]?, Pair[W, X]? -> Pair[Y, Z]? (if W
            // is coercible to Y and X is coercible to Z)
            (Self::Pair(src), Self::Pair(target)) => src.is_coercible_to(types, target),

            // Map[W, X] -> Map[Y, Z], Map[W, X] -> Map[Y, Z]?, Map[W, X]? -> Map[Y, Z]? (if W is
            // coercible to Y and X is coercible to Z)
            (Self::Map(src), Self::Map(target)) => src.is_coercible_to(types, target),

            // Struct -> Struct, Struct -> Struct?, Struct? -> Struct? (if the two struct types have
            // members with identical names and compatible types)
            (Self::Struct(src), Self::Struct(target)) => src.is_coercible_to(types, target),

            // Map[String, X] -> Struct, Map[String, X] -> Struct?, Map[String, X]? -> Struct? (if
            // `Map` keys match struct member name and all struct member types are coercible from X)
            (Self::Map(src), Self::Struct(target)) => {
                if !matches!(src.key_type, Type::Primitive(ty) if ty.kind() == PrimitiveTypeKind::String)
                {
                    return false;
                }

                // Ensure the value type is coercible to every struct member type
                if !target
                    .members
                    .values()
                    .all(|ty| src.value_type.is_coercible_to(types, ty))
                {
                    return false;
                }

                // Note: checking map keys is a runtime value constraint
                true
            }

            // Struct -> Map[String, X], Struct -> Map[String, X]?, Struct? -> Map[String, X]? (if
            // all struct members are coercible to X)
            (Self::Struct(src), Self::Map(target)) => {
                if !matches!(target.key_type, Type::Primitive(ty) if ty.kind() == PrimitiveTypeKind::String)
                {
                    return false;
                }

                // Ensure all the struct members are coercible to the value type
                if !src
                    .members
                    .values()
                    .all(|ty| ty.is_coercible_to(types, &target.value_type))
                {
                    return false;
                }

                true
            }

            _ => false,
        }
    }
}

impl From<ArrayType> for CompoundTypeDef {
    fn from(value: ArrayType) -> Self {
        Self::Array(value)
    }
}

impl From<PairType> for CompoundTypeDef {
    fn from(value: PairType) -> Self {
        Self::Pair(value)
    }
}

impl From<MapType> for CompoundTypeDef {
    fn from(value: MapType) -> Self {
        Self::Map(value)
    }
}

impl From<StructType> for CompoundTypeDef {
    fn from(value: StructType) -> Self {
        Self::Struct(value)
    }
}

/// Represents the type of an `Array`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ArrayType {
    /// The element type of the array.
    element_type: Type,
    /// Whether or not the array type is non-empty.
    ///
    /// This is `None` for literal arrays so that the array may coerce to both
    /// empty and non-empty types.
    non_empty: bool,
}

impl ArrayType {
    /// Constructs a new array type.
    pub fn new(element_type: impl Into<Type>) -> Self {
        Self {
            element_type: element_type.into(),
            non_empty: false,
        }
    }

    /// Constructs a new non-empty array type.
    pub fn non_empty(element_type: impl Into<Type>) -> Self {
        Self {
            element_type: element_type.into(),
            non_empty: true,
        }
    }

    /// Gets the array's element type.
    pub fn element_type(&self) -> Type {
        self.element_type
    }

    /// Determines if the array type is non-empty.
    pub fn is_non_empty(&self) -> bool {
        self.non_empty
    }

    /// Returns an object that implements `Display` for formatting the type.
    pub fn display<'a>(&'a self, types: &'a Types) -> impl fmt::Display + 'a {
        #[allow(clippy::missing_docs_in_private_items)]
        struct Display<'a> {
            types: &'a Types,
            ty: &'a ArrayType,
        }

        impl fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "Array[")?;
                self.ty.element_type.display(self.types).fmt(f)?;
                write!(f, "]")?;

                if self.ty.non_empty {
                    write!(f, "+")?;
                }

                Ok(())
            }
        }

        Display { types, ty: self }
    }

    /// Asserts that the type is valid.
    fn assert_valid(&self, types: &Types) {
        self.element_type.assert_valid(types);
    }
}

impl Coercible for ArrayType {
    fn is_coercible_to(&self, types: &Types, target: &Self) -> bool {
        self.element_type
            .is_coercible_to(types, &target.element_type)
    }
}

impl TypeEq for ArrayType {
    fn type_eq(&self, types: &Types, other: &Self) -> bool {
        self.non_empty == other.non_empty && self.element_type.type_eq(types, &other.element_type)
    }
}

/// Represents the type of a `Pair`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PairType {
    /// The type of the left element of the pair.
    left_type: Type,
    /// The type of the right element of the pair.
    right_type: Type,
}

impl PairType {
    /// Constructs a new pair type.
    pub fn new(left_type: impl Into<Type>, right_type: impl Into<Type>) -> Self {
        Self {
            left_type: left_type.into(),
            right_type: right_type.into(),
        }
    }

    /// Gets the pairs's left type.
    pub fn left_type(&self) -> Type {
        self.left_type
    }

    /// Gets the pairs's right type.
    pub fn right_type(&self) -> Type {
        self.right_type
    }

    /// Returns an object that implements `Display` for formatting the type.
    pub fn display<'a>(&'a self, types: &'a Types) -> impl fmt::Display + 'a {
        #[allow(clippy::missing_docs_in_private_items)]
        struct Display<'a> {
            types: &'a Types,
            ty: &'a PairType,
        }

        impl fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "Pair[")?;
                self.ty.left_type.display(self.types).fmt(f)?;
                write!(f, ", ")?;
                self.ty.right_type.display(self.types).fmt(f)?;
                write!(f, "]")
            }
        }

        Display { types, ty: self }
    }

    /// Asserts that the type is valid.
    fn assert_valid(&self, types: &Types) {
        self.left_type.assert_valid(types);
        self.right_type.assert_valid(types);
    }
}

impl Coercible for PairType {
    fn is_coercible_to(&self, types: &Types, target: &Self) -> bool {
        self.left_type.is_coercible_to(types, &target.left_type)
            && self.right_type.is_coercible_to(types, &target.right_type)
    }
}

impl TypeEq for PairType {
    fn type_eq(&self, types: &Types, other: &Self) -> bool {
        self.left_type.type_eq(types, &other.left_type)
            && self.right_type.type_eq(types, &other.right_type)
    }
}

/// Represents the type of a `Map`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapType {
    /// The key type of the map.
    key_type: Type,
    /// The value type of the map.
    value_type: Type,
}

impl MapType {
    /// Constructs a new map type.
    pub fn new(key_type: impl Into<Type>, value_type: impl Into<Type>) -> Self {
        Self {
            key_type: key_type.into(),
            value_type: value_type.into(),
        }
    }

    /// Gets the maps's key type.
    pub fn key_type(&self) -> Type {
        self.key_type
    }

    /// Gets the maps's value type.
    pub fn value_type(&self) -> Type {
        self.value_type
    }

    /// Returns an object that implements `Display` for formatting the type.
    pub fn display<'a>(&'a self, types: &'a Types) -> impl fmt::Display + 'a {
        #[allow(clippy::missing_docs_in_private_items)]
        struct Display<'a> {
            types: &'a Types,
            ty: &'a MapType,
        }

        impl fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "Map[")?;
                self.ty.key_type.display(self.types).fmt(f)?;
                write!(f, ", ")?;
                self.ty.value_type.display(self.types).fmt(f)?;
                write!(f, "]")
            }
        }

        Display { types, ty: self }
    }

    /// Asserts that the type is valid.
    fn assert_valid(&self, types: &Types) {
        self.value_type.assert_valid(types);
    }
}

impl Coercible for MapType {
    fn is_coercible_to(&self, types: &Types, target: &Self) -> bool {
        self.key_type.is_coercible_to(types, &target.key_type)
            && self.value_type.is_coercible_to(types, &target.value_type)
    }
}

impl TypeEq for MapType {
    fn type_eq(&self, types: &Types, other: &Self) -> bool {
        self.key_type.type_eq(types, &other.key_type)
            && self.value_type.type_eq(types, &other.value_type)
    }
}

/// Represents the type of a struct.
#[derive(Debug)]
pub struct StructType {
    /// The name of the struct.
    pub(crate) name: String,
    /// The members of the struct.
    pub(crate) members: IndexMap<String, Type>,
}

impl StructType {
    /// Constructs a new struct type definition.
    pub fn new<N, T>(name: impl Into<String>, members: impl IntoIterator<Item = (N, T)>) -> Self
    where
        N: Into<String>,
        T: Into<Type>,
    {
        Self {
            name: name.into(),
            members: members
                .into_iter()
                .map(|(n, ty)| (n.into(), ty.into()))
                .collect(),
        }
    }

    /// Gets the name of the struct.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Gets the members of the struct.
    pub fn members(&self) -> &IndexMap<String, Type> {
        &self.members
    }

    /// Asserts that this type is valid.
    fn assert_valid(&self, types: &Types) {
        for v in self.members.values() {
            v.assert_valid(types);
        }
    }
}

impl Coercible for StructType {
    fn is_coercible_to(&self, types: &Types, target: &Self) -> bool {
        if self.members.len() != target.members.len() {
            return false;
        }

        self.members.iter().all(|(k, v)| {
            target
                .members
                .get(k)
                .map(|target| v.is_coercible_to(types, target))
                .unwrap_or(false)
        })
    }
}

impl fmt::Display for StructType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{name}", name = self.name)
    }
}

/// The kind of call for a call type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CallKind {
    /// The call is to a task.
    Task,
    /// The call is to a workflow.
    Workflow,
}

impl fmt::Display for CallKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Task => write!(f, "task"),
            Self::Workflow => write!(f, "workflow"),
        }
    }
}

/// Represents the type of a call.
#[derive(Debug, Clone)]
pub struct CallType {
    /// The call kind.
    kind: CallKind,
    /// The namespace of the call.
    namespace: Option<Arc<String>>,
    /// The name of the task or workflow that was called.
    name: Arc<String>,
    /// The input types to the call.
    inputs: Arc<HashMap<String, Input>>,
    /// The output types from the call.
    outputs: Arc<HashMap<String, Output>>,
}

impl CallType {
    /// Constructs a new call type given the task or workflow name being called.
    pub fn new(
        kind: CallKind,
        name: impl Into<String>,
        inputs: Arc<HashMap<String, Input>>,
        outputs: Arc<HashMap<String, Output>>,
    ) -> Self {
        Self {
            kind,
            namespace: None,
            name: Arc::new(name.into()),
            inputs,
            outputs,
        }
    }

    /// Constructs a new call type given namespace and the task or workflow name
    /// being called.
    pub fn namespaced(
        kind: CallKind,
        namespace: impl Into<String>,
        name: impl Into<String>,
        inputs: Arc<HashMap<String, Input>>,
        outputs: Arc<HashMap<String, Output>>,
    ) -> Self {
        Self {
            kind,
            namespace: Some(Arc::new(namespace.into())),
            name: Arc::new(name.into()),
            inputs,
            outputs,
        }
    }

    /// Gets the kind of the call.
    pub fn kind(&self) -> CallKind {
        self.kind
    }

    /// Gets the namespace of the call target.
    ///
    /// Returns `None` if the call is local to the current document.
    pub fn namespace(&self) -> Option<&str> {
        self.namespace.as_ref().map(|ns| ns.as_str())
    }

    /// Gets the name of the call target.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Gets the inputs of the call.
    pub fn inputs(&self) -> &HashMap<String, Input> {
        &self.inputs
    }

    /// Gets the outputs of the call.
    pub fn outputs(&self) -> &HashMap<String, Output> {
        &self.outputs
    }

    /// Asserts that this type is valid.
    fn assert_valid(&self, types: &Types) {
        for v in self.inputs.values() {
            v.ty().assert_valid(types);
        }

        for v in self.outputs.values() {
            v.ty().assert_valid(types);
        }
    }
}

impl Coercible for CallType {
    fn is_coercible_to(&self, _: &Types, _: &Self) -> bool {
        // Calls are not coercible to other types
        false
    }
}

impl fmt::Display for CallType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ns) = &self.namespace {
            write!(
                f,
                "call to {kind} `{ns}.{name}`",
                kind = self.kind,
                name = self.name,
            )
        } else {
            write!(
                f,
                "call to {kind} `{name}`",
                kind = self.kind,
                name = self.name,
            )
        }
    }
}

/// Represents a collection of types.
#[derive(Debug, Default)]
pub struct Types(Arena<CompoundTypeDef>);

impl Types {
    /// Constructs a new type collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an array type to the type collection.
    ///
    /// # Panics
    ///
    /// Panics if the provided type contains a type definition identifier from a
    /// different types collection.
    pub fn add_array(&mut self, ty: ArrayType) -> Type {
        ty.assert_valid(self);
        Type::Compound(CompoundType {
            definition: self.0.alloc(CompoundTypeDef::Array(ty)),
            optional: false,
        })
    }

    /// Adds a pair type to the type collection.
    ///
    /// # Panics
    ///
    /// Panics if the provided type contains a type definition identifier from a
    /// different types collection.
    pub fn add_pair(&mut self, ty: PairType) -> Type {
        ty.assert_valid(self);
        Type::Compound(CompoundType {
            definition: self.0.alloc(CompoundTypeDef::Pair(ty)),
            optional: false,
        })
    }

    /// Adds a map type to the type collection.
    ///
    /// # Panics
    ///
    /// Panics if the provided type contains a type definition identifier from a
    /// different types collection.
    pub fn add_map(&mut self, ty: MapType) -> Type {
        ty.assert_valid(self);
        Type::Compound(CompoundType {
            definition: self.0.alloc(CompoundTypeDef::Map(ty)),
            optional: false,
        })
    }

    /// Adds a struct type to the type collection.
    ///
    /// # Panics
    ///
    /// Panics if the provided type contains a type definition identifier from a
    /// different types collection.
    pub fn add_struct(&mut self, ty: StructType) -> Type {
        ty.assert_valid(self);
        Type::Compound(CompoundType {
            definition: self.0.alloc(CompoundTypeDef::Struct(ty)),
            optional: false,
        })
    }

    /// Adds a call type to the type collection.
    pub fn add_call(&mut self, ty: CallType) -> Type {
        Type::Compound(CompoundType {
            definition: self.0.alloc(CompoundTypeDef::Call(ty)),
            optional: false,
        })
    }

    /// Gets a compound type definition by identifier.
    ///
    /// # Panics
    ///
    /// Panics if the identifier is not for this type collection.
    pub fn type_definition(&self, id: CompoundTypeDefId) -> &CompoundTypeDef {
        self.0
            .get(id)
            // Fall back to types defined by the standard library
            .or_else(|| STDLIB.types().0.get(id))
            .expect("invalid type identifier")
    }

    /// Gets a struct type from the type collection.
    ///
    /// Returns `None` if the type is not a struct.
    pub fn struct_type(&self, ty: Type) -> Option<&StructType> {
        if let Type::Compound(ty) = ty {
            if let CompoundTypeDef::Struct(s) = &self.0[ty.definition()] {
                return Some(s);
            }
        }

        None
    }

    /// Imports a type from a foreign type collection.
    ///
    /// Returns the new type that is local to this type collection.
    ///
    /// # Panics
    ///
    /// Panics if the specified type is a call, as calls cannot be imported.
    pub fn import(&mut self, types: &Self, ty: Type) -> Type {
        match ty {
            Type::Primitive(ty) => Type::Primitive(ty),
            Type::Compound(ty) => match &types.0[ty.definition] {
                CompoundTypeDef::Array(ty) => {
                    let element_type = self.import(types, ty.element_type);
                    self.add_array(ArrayType {
                        element_type,
                        non_empty: ty.non_empty,
                    })
                }
                CompoundTypeDef::Pair(ty) => {
                    let left_type = self.import(types, ty.left_type);
                    let right_type = self.import(types, ty.right_type);
                    self.add_pair(PairType {
                        left_type,
                        right_type,
                    })
                }
                CompoundTypeDef::Map(ty) => {
                    let value_type = self.import(types, ty.value_type);
                    self.add_map(MapType {
                        key_type: ty.key_type,
                        value_type,
                    })
                }
                CompoundTypeDef::Struct(ty) => {
                    let members = ty
                        .members
                        .iter()
                        .map(|(k, v)| (k.clone(), self.import(types, *v)))
                        .collect();

                    self.add_struct(StructType {
                        name: ty.name.clone(),
                        members,
                    })
                }
                CompoundTypeDef::Call(_) => panic!("call types cannot be imported"),
            },
            Type::Object => Type::Object,
            Type::OptionalObject => Type::OptionalObject,
            Type::Union => Type::Union,
            Type::None => Type::None,
            Type::Task => Type::Task,
            Type::Hints => Type::Hints,
            Type::Input => Type::Input,
            Type::Output => Type::Output,
        }
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn primitive_type_display() {
        assert_eq!(
            PrimitiveType::new(PrimitiveTypeKind::Boolean).to_string(),
            "Boolean"
        );
        assert_eq!(
            PrimitiveType::new(PrimitiveTypeKind::Integer).to_string(),
            "Int"
        );
        assert_eq!(
            PrimitiveType::new(PrimitiveTypeKind::Float).to_string(),
            "Float"
        );
        assert_eq!(
            PrimitiveType::new(PrimitiveTypeKind::String).to_string(),
            "String"
        );
        assert_eq!(
            PrimitiveType::new(PrimitiveTypeKind::File).to_string(),
            "File"
        );
        assert_eq!(
            PrimitiveType::new(PrimitiveTypeKind::Directory).to_string(),
            "Directory"
        );
        assert_eq!(
            PrimitiveType::optional(PrimitiveTypeKind::Boolean).to_string(),
            "Boolean?"
        );
        assert_eq!(
            PrimitiveType::optional(PrimitiveTypeKind::Integer).to_string(),
            "Int?"
        );
        assert_eq!(
            PrimitiveType::optional(PrimitiveTypeKind::Float).to_string(),
            "Float?"
        );
        assert_eq!(
            PrimitiveType::optional(PrimitiveTypeKind::String).to_string(),
            "String?"
        );
        assert_eq!(
            PrimitiveType::optional(PrimitiveTypeKind::File).to_string(),
            "File?"
        );
        assert_eq!(
            PrimitiveType::optional(PrimitiveTypeKind::Directory).to_string(),
            "Directory?"
        );
    }

    #[test]
    fn array_type_display() {
        let mut types = Types::new();
        assert_eq!(
            ArrayType::new(PrimitiveTypeKind::String)
                .display(&types)
                .to_string(),
            "Array[String]"
        );
        assert_eq!(
            ArrayType::non_empty(PrimitiveTypeKind::String)
                .display(&types)
                .to_string(),
            "Array[String]+"
        );

        let ty = types.add_array(ArrayType::new(PrimitiveTypeKind::String));
        assert_eq!(
            types
                .add_array(ArrayType::new(ty))
                .display(&types)
                .to_string(),
            "Array[Array[String]]"
        );

        let ty = types
            .add_array(ArrayType::non_empty(PrimitiveType::optional(
                PrimitiveTypeKind::String,
            )))
            .optional();
        assert_eq!(
            types
                .add_array(ArrayType::non_empty(ty))
                .optional()
                .display(&types)
                .to_string(),
            "Array[Array[String?]+?]+?"
        );
    }

    #[test]
    fn pair_type_display() {
        let mut types = Types::new();
        assert_eq!(
            PairType::new(PrimitiveTypeKind::String, PrimitiveTypeKind::Boolean)
                .display(&types)
                .to_string(),
            "Pair[String, Boolean]"
        );

        let ty = types.add_array(ArrayType::new(PrimitiveTypeKind::String));
        assert_eq!(
            types
                .add_pair(PairType::new(ty, ty))
                .display(&types)
                .to_string(),
            "Pair[Array[String], Array[String]]"
        );

        let ty = types
            .add_array(ArrayType::non_empty(PrimitiveType::optional(
                PrimitiveTypeKind::File,
            )))
            .optional();
        assert_eq!(
            types
                .add_pair(PairType::new(ty, ty))
                .optional()
                .display(&types)
                .to_string(),
            "Pair[Array[File?]+?, Array[File?]+?]?"
        );
    }

    #[test]
    fn map_type_display() {
        let mut types = Types::new();
        assert_eq!(
            MapType::new(PrimitiveTypeKind::String, PrimitiveTypeKind::Boolean)
                .display(&types)
                .to_string(),
            "Map[String, Boolean]"
        );

        let ty = types.add_array(ArrayType::new(PrimitiveTypeKind::String));
        assert_eq!(
            types
                .add_map(MapType::new(PrimitiveTypeKind::Boolean, ty))
                .display(&types)
                .to_string(),
            "Map[Boolean, Array[String]]"
        );

        let ty = types
            .add_array(ArrayType::non_empty(PrimitiveType::optional(
                PrimitiveTypeKind::File,
            )))
            .optional();
        assert_eq!(
            types
                .add_map(MapType::new(PrimitiveTypeKind::String, ty))
                .optional()
                .display(&types)
                .to_string(),
            "Map[String, Array[File?]+?]?"
        );
    }

    #[test]
    fn struct_type_display() {
        assert_eq!(
            StructType::new("Foobar", std::iter::empty::<(String, Type)>()).to_string(),
            "Foobar"
        );
    }

    #[test]
    fn object_type_display() {
        let types = Types::new();
        assert_eq!(Type::Object.display(&types).to_string(), "Object");
        assert_eq!(Type::OptionalObject.display(&types).to_string(), "Object?");
    }

    #[test]
    fn union_type_display() {
        let types = Types::new();
        assert_eq!(Type::Union.display(&types).to_string(), "Union");
    }

    #[test]
    fn none_type_display() {
        let types = Types::new();
        assert_eq!(Type::None.display(&types).to_string(), "None");
    }

    #[test]
    fn primitive_type_coercion() {
        let types = Types::new();

        // All types should be coercible to self, and required should coerce to optional
        // (but not vice versa)
        for kind in [
            PrimitiveTypeKind::Boolean,
            PrimitiveTypeKind::Directory,
            PrimitiveTypeKind::File,
            PrimitiveTypeKind::Float,
            PrimitiveTypeKind::Integer,
            PrimitiveTypeKind::String,
        ] {
            assert!(PrimitiveType::new(kind).is_coercible_to(&types, &PrimitiveType::new(kind)));
            assert!(
                PrimitiveType::optional(kind)
                    .is_coercible_to(&types, &PrimitiveType::optional(kind))
            );
            assert!(
                PrimitiveType::new(kind).is_coercible_to(&types, &PrimitiveType::optional(kind))
            );
            assert!(
                !PrimitiveType::optional(kind).is_coercible_to(&types, &PrimitiveType::new(kind))
            );
        }

        // Check the valid coercions
        assert!(
            PrimitiveType::new(PrimitiveTypeKind::String)
                .is_coercible_to(&types, &PrimitiveType::new(PrimitiveTypeKind::File))
        );
        assert!(
            PrimitiveType::new(PrimitiveTypeKind::String)
                .is_coercible_to(&types, &PrimitiveType::new(PrimitiveTypeKind::Directory))
        );
        assert!(
            PrimitiveType::new(PrimitiveTypeKind::Integer)
                .is_coercible_to(&types, &PrimitiveType::new(PrimitiveTypeKind::Float))
        );
        assert!(
            PrimitiveType::new(PrimitiveTypeKind::File)
                .is_coercible_to(&types, &PrimitiveType::new(PrimitiveTypeKind::String))
        );
        assert!(
            PrimitiveType::new(PrimitiveTypeKind::Directory)
                .is_coercible_to(&types, &PrimitiveType::new(PrimitiveTypeKind::String))
        );
        assert!(
            !PrimitiveType::new(PrimitiveTypeKind::Float)
                .is_coercible_to(&types, &PrimitiveType::new(PrimitiveTypeKind::Integer))
        );
    }

    #[test]
    fn object_type_coercion() {
        let mut types = Types::new();
        assert!(Type::Object.is_coercible_to(&types, &Type::Object));
        assert!(Type::Object.is_coercible_to(&types, &Type::OptionalObject));
        assert!(Type::OptionalObject.is_coercible_to(&types, &Type::OptionalObject));
        assert!(!Type::OptionalObject.is_coercible_to(&types, &Type::Object));

        // Object -> Map[String, X]
        let ty = types.add_map(MapType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::String,
        ));
        assert!(!Type::OptionalObject.is_coercible_to(&types, &ty));

        // Object -> Map[Int, X] (not a string key)
        let ty = types.add_map(MapType::new(
            PrimitiveTypeKind::Integer,
            PrimitiveTypeKind::String,
        ));
        assert!(!Type::Object.is_coercible_to(&types, &ty));

        // Object -> Map[String, X]?
        let ty = types
            .add_map(MapType::new(
                PrimitiveTypeKind::String,
                PrimitiveTypeKind::String,
            ))
            .optional();
        assert!(Type::Object.is_coercible_to(&types, &ty));

        // Object? -> Map[String, X]?
        let ty = types
            .add_map(MapType::new(
                PrimitiveTypeKind::String,
                PrimitiveTypeKind::String,
            ))
            .optional();
        assert!(Type::OptionalObject.is_coercible_to(&types, &ty));

        // Object? -> Map[String, X]
        let ty = types.add_map(MapType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::String,
        ));
        assert!(!Type::OptionalObject.is_coercible_to(&types, &ty));

        // Object -> Struct
        let ty = types.add_struct(StructType::new("Foo", [("foo", PrimitiveTypeKind::String)]));
        assert!(Type::Object.is_coercible_to(&types, &ty));

        // Object -> Struct?
        let ty = types
            .add_struct(StructType::new("Foo", [("foo", PrimitiveTypeKind::String)]))
            .optional();
        assert!(Type::Object.is_coercible_to(&types, &ty));

        // Object? -> Struct?
        let ty = types
            .add_struct(StructType::new("Foo", [("foo", PrimitiveTypeKind::String)]))
            .optional();
        assert!(Type::OptionalObject.is_coercible_to(&types, &ty));

        // Object? -> Struct
        let ty = types.add_struct(StructType::new("Foo", [("foo", PrimitiveTypeKind::String)]));
        assert!(!Type::OptionalObject.is_coercible_to(&types, &ty));
    }

    #[test]
    fn array_type_coercion() {
        let mut types = Types::new();

        // Array[X] -> Array[Y]
        assert!(
            ArrayType::new(PrimitiveTypeKind::String)
                .is_coercible_to(&types, &ArrayType::new(PrimitiveTypeKind::String))
        );
        assert!(
            ArrayType::new(PrimitiveTypeKind::File)
                .is_coercible_to(&types, &ArrayType::new(PrimitiveTypeKind::String))
        );
        assert!(
            ArrayType::new(PrimitiveTypeKind::String)
                .is_coercible_to(&types, &ArrayType::new(PrimitiveTypeKind::File))
        );

        // Array[X] -> Array[Y?]
        let type1 = types.add_array(ArrayType::new(PrimitiveTypeKind::String));
        let type2 = types.add_array(ArrayType::new(PrimitiveType::optional(
            PrimitiveTypeKind::File,
        )));
        assert!(type1.is_coercible_to(&types, &type2));
        assert!(!type2.is_coercible_to(&types, &type1));

        // Array[Array[X]] -> Array[Array[Y]]
        let type1 = types.add_array(ArrayType::new(type1));
        let type2 = types.add_array(ArrayType::new(type2));
        assert!(type1.is_coercible_to(&types, &type2));
        assert!(!type2.is_coercible_to(&types, &type1));

        // Array[X]+ -> Array[Y]
        let type1 = types.add_array(ArrayType::non_empty(PrimitiveTypeKind::String));
        let type2 = types.add_array(ArrayType::new(PrimitiveType::optional(
            PrimitiveTypeKind::File,
        )));
        assert!(type1.is_coercible_to(&types, &type2));
        assert!(!type2.is_coercible_to(&types, &type1));

        // Array[X]+ -> Array[X?]
        let type1 = types.add_array(ArrayType::non_empty(PrimitiveTypeKind::String));
        let type2 = types.add_array(ArrayType::new(PrimitiveType::optional(
            PrimitiveTypeKind::String,
        )));
        assert!(type1.is_coercible_to(&types, &type2));
        assert!(!type2.is_coercible_to(&types, &type1));

        // Array[X] -> Array[X]
        let type1 = types.add_array(ArrayType::new(PrimitiveTypeKind::String));
        let type2 = types.add_array(ArrayType::new(PrimitiveTypeKind::String));
        assert!(type1.is_coercible_to(&types, &type2));
        assert!(type2.is_coercible_to(&types, &type1));

        // Array[X]? -> Array[X]?
        let type1 = types
            .add_array(ArrayType::new(PrimitiveTypeKind::String))
            .optional();
        let type2 = types
            .add_array(ArrayType::new(PrimitiveTypeKind::String))
            .optional();
        assert!(type1.is_coercible_to(&types, &type2));
        assert!(type2.is_coercible_to(&types, &type1));

        // Array[X] -> Array[X]?
        let type1 = types.add_array(ArrayType::new(PrimitiveTypeKind::String));
        let type2 = types
            .add_array(ArrayType::new(PrimitiveTypeKind::String))
            .optional();
        assert!(type1.is_coercible_to(&types, &type2));
        assert!(!type2.is_coercible_to(&types, &type1));
    }

    #[test]
    fn pair_type_coercion() {
        let mut types = Types::new();

        // Pair[W, X] -> Pair[Y, Z]
        assert!(
            PairType::new(PrimitiveTypeKind::String, PrimitiveTypeKind::String).is_coercible_to(
                &types,
                &PairType::new(PrimitiveTypeKind::String, PrimitiveTypeKind::String)
            )
        );
        assert!(
            PairType::new(PrimitiveTypeKind::String, PrimitiveTypeKind::String).is_coercible_to(
                &types,
                &PairType::new(PrimitiveTypeKind::File, PrimitiveTypeKind::Directory)
            )
        );
        assert!(
            PairType::new(PrimitiveTypeKind::File, PrimitiveTypeKind::Directory).is_coercible_to(
                &types,
                &PairType::new(PrimitiveTypeKind::String, PrimitiveTypeKind::String)
            )
        );

        // Pair[W, X] -> Pair[Y?, Z?]
        let type1 = types.add_pair(PairType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::String,
        ));
        let type2 = types.add_pair(PairType::new(
            PrimitiveType::optional(PrimitiveTypeKind::File),
            PrimitiveType::optional(PrimitiveTypeKind::Directory),
        ));
        assert!(type1.is_coercible_to(&types, &type2));
        assert!(!type2.is_coercible_to(&types, &type1));

        // Pair[Pair[W, X], Pair[W, X]] -> Pair[Pair[Y, Z], Pair[Y, Z]]
        let type1 = types.add_pair(PairType::new(type1, type1));
        let type2 = types.add_pair(PairType::new(type2, type2));
        assert!(type1.is_coercible_to(&types, &type2));
        assert!(!type2.is_coercible_to(&types, &type1));

        // Pair[W, X] -> Pair[W, X]
        let type1 = types.add_pair(PairType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::String,
        ));
        let type2 = types.add_pair(PairType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::String,
        ));
        assert!(type1.is_coercible_to(&types, &type2));
        assert!(type2.is_coercible_to(&types, &type1));

        // Pair[W, X]? -> Pair[W, X]?
        let type1 = types
            .add_pair(PairType::new(
                PrimitiveTypeKind::String,
                PrimitiveTypeKind::String,
            ))
            .optional();
        let type2 = types
            .add_pair(PairType::new(
                PrimitiveTypeKind::String,
                PrimitiveTypeKind::String,
            ))
            .optional();
        assert!(type1.is_coercible_to(&types, &type2));
        assert!(type2.is_coercible_to(&types, &type1));

        // Pair[W, X] -> Pair[W, X]?
        let type1 = types.add_pair(PairType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::String,
        ));
        let type2 = types
            .add_pair(PairType::new(
                PrimitiveTypeKind::String,
                PrimitiveTypeKind::String,
            ))
            .optional();
        assert!(type1.is_coercible_to(&types, &type2));
        assert!(!type2.is_coercible_to(&types, &type1));
    }

    #[test]
    fn map_type_coercion() {
        let mut types = Types::new();

        // Map[W, X] -> Map[Y, Z]
        assert!(
            MapType::new(PrimitiveTypeKind::String, PrimitiveTypeKind::String).is_coercible_to(
                &types,
                &MapType::new(PrimitiveTypeKind::String, PrimitiveTypeKind::String)
            )
        );
        assert!(
            MapType::new(PrimitiveTypeKind::String, PrimitiveTypeKind::String).is_coercible_to(
                &types,
                &MapType::new(PrimitiveTypeKind::File, PrimitiveTypeKind::Directory)
            )
        );
        assert!(
            MapType::new(PrimitiveTypeKind::File, PrimitiveTypeKind::Directory).is_coercible_to(
                &types,
                &MapType::new(PrimitiveTypeKind::String, PrimitiveTypeKind::String)
            )
        );

        // Map[W, X] -> Map[Y?, Z?]
        let type1 = types.add_map(MapType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::String,
        ));
        let type2 = types.add_map(MapType::new(
            PrimitiveType::optional(PrimitiveTypeKind::File),
            PrimitiveType::optional(PrimitiveTypeKind::Directory),
        ));
        assert!(type1.is_coercible_to(&types, &type2));
        assert!(!type2.is_coercible_to(&types, &type1));

        // Map[P, Map[W, X]] -> Map[Q, Map[Y, Z]]
        let type1 = types.add_map(MapType::new(PrimitiveTypeKind::String, type1));
        let type2 = types.add_map(MapType::new(PrimitiveTypeKind::Directory, type2));
        assert!(type1.is_coercible_to(&types, &type2));
        assert!(!type2.is_coercible_to(&types, &type1));

        // Map[W, X] -> Map[W, X]
        let type1 = types.add_map(MapType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::String,
        ));
        let type2 = types.add_map(MapType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::String,
        ));
        assert!(type1.is_coercible_to(&types, &type2));
        assert!(type2.is_coercible_to(&types, &type1));

        // Map[W, X]? -> Map[W, X]?
        let type1 = types
            .add_map(MapType::new(
                PrimitiveTypeKind::String,
                PrimitiveTypeKind::String,
            ))
            .optional();
        let type2: Type = types
            .add_map(MapType::new(
                PrimitiveTypeKind::String,
                PrimitiveTypeKind::String,
            ))
            .optional();
        assert!(type1.is_coercible_to(&types, &type2));
        assert!(type2.is_coercible_to(&types, &type1));

        // Map[W, X] -> Map[W, X]?
        let type1 = types.add_map(MapType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::String,
        ));
        let type2 = types
            .add_map(MapType::new(
                PrimitiveTypeKind::String,
                PrimitiveTypeKind::String,
            ))
            .optional();
        assert!(type1.is_coercible_to(&types, &type2));
        assert!(!type2.is_coercible_to(&types, &type1));

        // Map[String, X] -> Struct
        let type1 = types.add_map(MapType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::Integer,
        ));
        let type2 = types.add_struct(StructType::new("Foo", [
            ("foo", PrimitiveTypeKind::Integer),
            ("bar", PrimitiveTypeKind::Integer),
            ("baz", PrimitiveTypeKind::Integer),
        ]));
        assert!(type1.is_coercible_to(&types, &type2));

        // Map[String, X] -> Struct (mismatched fields)
        let type1 = types.add_map(MapType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::Integer,
        ));
        let type2 = types.add_struct(StructType::new("Foo", [
            ("foo", PrimitiveTypeKind::Integer),
            ("bar", PrimitiveTypeKind::String),
            ("baz", PrimitiveTypeKind::Integer),
        ]));
        assert!(!type1.is_coercible_to(&types, &type2));

        // Map[Int, X] -> Struct
        let type1 = types.add_map(MapType::new(
            PrimitiveTypeKind::Integer,
            PrimitiveTypeKind::Integer,
        ));
        let type2 = types.add_struct(StructType::new("Foo", [
            ("foo", PrimitiveTypeKind::Integer),
            ("bar", PrimitiveTypeKind::Integer),
            ("baz", PrimitiveTypeKind::Integer),
        ]));
        assert!(!type1.is_coercible_to(&types, &type2));

        // Map[String, X] -> Object
        let type1 = types.add_map(MapType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::Integer,
        ));
        assert!(type1.is_coercible_to(&types, &Type::Object));

        // Map[String, X] -> Object?
        let type1 = types.add_map(MapType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::Integer,
        ));
        assert!(type1.is_coercible_to(&types, &Type::OptionalObject));

        // Map[String, X]? -> Object?
        let type1 = types
            .add_map(MapType::new(
                PrimitiveTypeKind::String,
                PrimitiveTypeKind::Integer,
            ))
            .optional();
        assert!(type1.is_coercible_to(&types, &Type::OptionalObject));

        // Map[String, X]? -> Object
        let type1 = types
            .add_map(MapType::new(
                PrimitiveTypeKind::String,
                PrimitiveTypeKind::Integer,
            ))
            .optional();
        assert!(!type1.is_coercible_to(&types, &Type::Object));

        // Map[Integer, X] -> Object
        let type1 = types.add_map(MapType::new(
            PrimitiveTypeKind::Integer,
            PrimitiveTypeKind::Integer,
        ));
        assert!(!type1.is_coercible_to(&types, &Type::Object));
    }

    #[test]
    fn struct_type_coercion() {
        let mut types = Types::new();

        // S -> S (identical)
        let type1 = types.add_struct(StructType::new("Foo", [
            ("foo", PrimitiveTypeKind::String),
            ("bar", PrimitiveTypeKind::String),
            ("baz", PrimitiveTypeKind::Integer),
        ]));
        let type2 = types.add_struct(StructType::new("Foo", [
            ("foo", PrimitiveTypeKind::String),
            ("bar", PrimitiveTypeKind::String),
            ("baz", PrimitiveTypeKind::Integer),
        ]));
        assert!(type1.is_coercible_to(&types, &type2));
        assert!(type2.is_coercible_to(&types, &type1));

        // S -> S?
        let type1 = types.add_struct(StructType::new("Foo", [
            ("foo", PrimitiveTypeKind::String),
            ("bar", PrimitiveTypeKind::String),
            ("baz", PrimitiveTypeKind::Integer),
        ]));
        let type2 = types
            .add_struct(StructType::new("Foo", [
                ("foo", PrimitiveTypeKind::String),
                ("bar", PrimitiveTypeKind::String),
                ("baz", PrimitiveTypeKind::Integer),
            ]))
            .optional();
        assert!(type1.is_coercible_to(&types, &type2));
        assert!(!type2.is_coercible_to(&types, &type1));

        // S? -> S?
        let type1 = types
            .add_struct(StructType::new("Foo", [
                ("foo", PrimitiveTypeKind::String),
                ("bar", PrimitiveTypeKind::String),
                ("baz", PrimitiveTypeKind::Integer),
            ]))
            .optional();
        let type2 = types
            .add_struct(StructType::new("Foo", [
                ("foo", PrimitiveTypeKind::String),
                ("bar", PrimitiveTypeKind::String),
                ("baz", PrimitiveTypeKind::Integer),
            ]))
            .optional();
        assert!(type1.is_coercible_to(&types, &type2));
        assert!(type2.is_coercible_to(&types, &type1));

        // S -> S (coercible fields)
        let type1 = types.add_struct(StructType::new("Foo", [
            ("foo", PrimitiveTypeKind::String),
            ("bar", PrimitiveTypeKind::String),
            ("baz", PrimitiveTypeKind::Integer),
        ]));
        let type2 = types.add_struct(StructType::new("Bar", [
            ("foo", PrimitiveTypeKind::File),
            ("bar", PrimitiveTypeKind::Directory),
            ("baz", PrimitiveTypeKind::Float),
        ]));
        assert!(type1.is_coercible_to(&types, &type2));
        assert!(!type2.is_coercible_to(&types, &type1));

        // S -> S (mismatched fields)
        let type1 = types.add_struct(StructType::new("Foo", [
            ("foo", PrimitiveTypeKind::String),
            ("bar", PrimitiveTypeKind::String),
            ("baz", PrimitiveTypeKind::Integer),
        ]));
        let type2 = types.add_struct(StructType::new("Bar", [("baz", PrimitiveTypeKind::Float)]));
        assert!(!type1.is_coercible_to(&types, &type2));
        assert!(!type2.is_coercible_to(&types, &type1));

        // Struct -> Map[String, X]
        let type1 = types.add_struct(StructType::new("Foo", [
            ("foo", PrimitiveTypeKind::String),
            ("bar", PrimitiveTypeKind::String),
            ("baz", PrimitiveTypeKind::String),
        ]));
        let type2 = types.add_map(MapType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::String,
        ));
        assert!(type1.is_coercible_to(&types, &type2));

        // Struct -> Map[String, X] (mismatched types)
        let type1 = types.add_struct(StructType::new("Foo", [
            ("foo", PrimitiveTypeKind::String),
            ("bar", PrimitiveTypeKind::Integer),
            ("baz", PrimitiveTypeKind::String),
        ]));
        let type2 = types.add_map(MapType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::String,
        ));
        assert!(!type1.is_coercible_to(&types, &type2));

        // Struct -> Map[Int, X] (not a string key)
        let type1 = types.add_struct(StructType::new("Foo", [
            ("foo", PrimitiveTypeKind::String),
            ("bar", PrimitiveTypeKind::String),
            ("baz", PrimitiveTypeKind::String),
        ]));
        let type2 = types.add_map(MapType::new(
            PrimitiveTypeKind::Integer,
            PrimitiveTypeKind::String,
        ));
        assert!(!type1.is_coercible_to(&types, &type2));

        // Struct -> Object
        assert!(type1.is_coercible_to(&types, &Type::Object));

        // Struct -> Object?
        assert!(type1.is_coercible_to(&types, &Type::OptionalObject));

        // Struct? -> Object?
        let type1 = types
            .add_struct(StructType::new("Foo", [("foo", PrimitiveTypeKind::String)]))
            .optional();
        assert!(type1.is_coercible_to(&types, &Type::OptionalObject));

        // Struct? -> Object
        assert!(!type1.is_coercible_to(&types, &Type::Object));
    }

    #[test]
    fn union_type_coercion() {
        let mut types = Types::new();
        // Union -> anything (ok)
        for kind in [
            PrimitiveTypeKind::Boolean,
            PrimitiveTypeKind::Directory,
            PrimitiveTypeKind::File,
            PrimitiveTypeKind::Float,
            PrimitiveTypeKind::Integer,
            PrimitiveTypeKind::String,
        ] {
            assert!(Type::Union.is_coercible_to(&types, &kind.into()));
            assert!(Type::Union.is_coercible_to(&types, &PrimitiveType::optional(kind).into()));
            assert!(!Type::from(kind).is_coercible_to(&types, &Type::Union));
        }

        for optional in [true, false] {
            // Union -> Array[X], Union -> Array[X]?
            let ty = types.add_array(ArrayType::new(PrimitiveTypeKind::String));
            let ty = if optional { ty.optional() } else { ty };

            let coercible = Type::Union.is_coercible_to(&types, &ty);
            assert!(coercible);

            // Union -> Pair[X, Y], Union -> Pair[X, Y]?
            let ty = types.add_pair(PairType::new(
                PrimitiveTypeKind::String,
                PrimitiveTypeKind::Boolean,
            ));
            let ty = if optional { ty.optional() } else { ty };
            let coercible = Type::Union.is_coercible_to(&types, &ty);
            assert!(coercible);

            // Union -> Map[X, Y], Union -> Map[X, Y]?
            let ty = types.add_map(MapType::new(
                PrimitiveTypeKind::String,
                PrimitiveTypeKind::Boolean,
            ));
            let ty = if optional { ty.optional() } else { ty };
            let coercible = Type::Union.is_coercible_to(&types, &ty);
            assert!(coercible);

            // Union -> Struct, Union -> Struct?
            let ty = types.add_struct(StructType::new("Foo", [("foo", PrimitiveTypeKind::String)]));
            let ty = if optional { ty.optional() } else { ty };
            let coercible = Type::Union.is_coercible_to(&types, &ty);
            assert!(coercible);
        }
    }

    #[test]
    fn none_type_coercion() {
        let mut types = Types::new();
        // None -> optional type (ok)
        for kind in [
            PrimitiveTypeKind::Boolean,
            PrimitiveTypeKind::Directory,
            PrimitiveTypeKind::File,
            PrimitiveTypeKind::Float,
            PrimitiveTypeKind::Integer,
            PrimitiveTypeKind::String,
        ] {
            assert!(!Type::None.is_coercible_to(&types, &kind.into()));
            assert!(Type::None.is_coercible_to(&types, &PrimitiveType::optional(kind).into()));
            assert!(!Type::from(kind).is_coercible_to(&types, &Type::None));
        }

        for optional in [true, false] {
            // None -> Array[X], None -> Array[X]?
            let ty = types.add_array(ArrayType::new(PrimitiveTypeKind::String));
            let ty = if optional { ty.optional() } else { ty };
            let coercible = Type::None.is_coercible_to(&types, &ty);
            if optional {
                assert!(coercible);
            } else {
                assert!(!coercible);
            }

            // None -> Pair[X, Y], None -> Pair[X, Y]?
            let ty = types.add_pair(PairType::new(
                PrimitiveTypeKind::String,
                PrimitiveTypeKind::Boolean,
            ));
            let ty = if optional { ty.optional() } else { ty };
            let coercible = Type::None.is_coercible_to(&types, &ty);
            if optional {
                assert!(coercible);
            } else {
                assert!(!coercible);
            }

            // None -> Map[X, Y], None -> Map[X, Y]?
            let ty = types.add_map(MapType::new(
                PrimitiveTypeKind::String,
                PrimitiveTypeKind::Boolean,
            ));
            let ty = if optional { ty.optional() } else { ty };
            let coercible = Type::None.is_coercible_to(&types, &ty);
            if optional {
                assert!(coercible);
            } else {
                assert!(!coercible);
            }

            // None -> Struct, None -> Struct?
            let ty = types.add_struct(StructType::new("Foo", [("foo", PrimitiveTypeKind::String)]));
            let ty = if optional { ty.optional() } else { ty };
            let coercible = Type::None.is_coercible_to(&types, &ty);
            if optional {
                assert!(coercible);
            } else {
                assert!(!coercible);
            }
        }
    }

    #[test]
    fn primitive_type_equality() {
        let types = Types::new();

        for kind in [
            PrimitiveTypeKind::Boolean,
            PrimitiveTypeKind::Directory,
            PrimitiveTypeKind::File,
            PrimitiveTypeKind::Float,
            PrimitiveTypeKind::Integer,
            PrimitiveTypeKind::String,
        ] {
            assert!(PrimitiveType::new(kind).type_eq(&types, &PrimitiveType::new(kind)));
            assert!(!PrimitiveType::optional(kind).type_eq(&types, &PrimitiveType::new(kind)));
            assert!(!PrimitiveType::new(kind).type_eq(&types, &PrimitiveType::optional(kind)));
            assert!(PrimitiveType::optional(kind).type_eq(&types, &PrimitiveType::optional(kind)));
            assert!(!Type::from(PrimitiveType::new(kind)).type_eq(&types, &Type::Object));
            assert!(!Type::from(PrimitiveType::new(kind)).type_eq(&types, &Type::OptionalObject));
            assert!(!Type::from(PrimitiveType::new(kind)).type_eq(&types, &Type::Union));
            assert!(!Type::from(PrimitiveType::new(kind)).type_eq(&types, &Type::None));
        }
    }

    #[test]
    fn array_type_equality() {
        let mut types = Types::new();

        // Array[String] == Array[String]
        let a = types.add_array(ArrayType::new(PrimitiveTypeKind::String));
        let b = types.add_array(ArrayType::new(PrimitiveTypeKind::String));
        assert!(a.type_eq(&types, &b));
        assert!(!a.optional().type_eq(&types, &b));
        assert!(!a.type_eq(&types, &b.optional()));
        assert!(a.optional().type_eq(&types, &b.optional()));

        // Array[Array[String]] == Array[Array[String]
        let a = types.add_array(ArrayType::new(a));
        let b = types.add_array(ArrayType::new(b));
        assert!(a.type_eq(&types, &b));

        // Array[Array[Array[String]]]+ == Array[Array[Array[String]]+
        let a = types.add_array(ArrayType::non_empty(a));
        let b = types.add_array(ArrayType::non_empty(b));
        assert!(a.type_eq(&types, &b));

        // Array[String] != Array[String]+
        let a = types.add_array(ArrayType::new(PrimitiveTypeKind::String));
        let b = types.add_array(ArrayType::non_empty(PrimitiveTypeKind::String));
        assert!(!a.type_eq(&types, &b));

        // Array[String] != Array[Int]
        let a = types.add_array(ArrayType::new(PrimitiveTypeKind::String));
        let b = types.add_array(ArrayType::new(PrimitiveTypeKind::Integer));
        assert!(!a.type_eq(&types, &b));

        assert!(!a.type_eq(&types, &Type::Object));
        assert!(!a.type_eq(&types, &Type::OptionalObject));
        assert!(!a.type_eq(&types, &Type::Union));
        assert!(!a.type_eq(&types, &Type::None));
    }

    #[test]
    fn pair_type_equality() {
        let mut types = Types::new();

        // Pair[String, Int] == Pair[String, Int]
        let a = types.add_pair(PairType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::Integer,
        ));
        let b = types.add_pair(PairType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::Integer,
        ));
        assert!(a.type_eq(&types, &b));
        assert!(!a.optional().type_eq(&types, &b));
        assert!(!a.type_eq(&types, &b.optional()));
        assert!(a.optional().type_eq(&types, &b.optional()));

        // Pair[Pair[String, Int], Pair[String, Int]] == Pair[Pair[String, Int],
        // Pair[String, Int]]
        let a = types.add_pair(PairType::new(a, a));
        let b = types.add_pair(PairType::new(b, b));
        assert!(a.type_eq(&types, &b));

        // Pair[String, Int] != Pair[String, Int]?
        let a = types.add_pair(PairType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::Integer,
        ));
        let b = types
            .add_pair(PairType::new(
                PrimitiveTypeKind::String,
                PrimitiveTypeKind::Integer,
            ))
            .optional();
        assert!(!a.type_eq(&types, &b));

        assert!(!a.type_eq(&types, &Type::Object));
        assert!(!a.type_eq(&types, &Type::OptionalObject));
        assert!(!a.type_eq(&types, &Type::Union));
        assert!(!a.type_eq(&types, &Type::None));
    }

    #[test]
    fn map_type_equality() {
        let mut types = Types::new();

        // Map[String, Int] == Map[String, Int]
        let a = types.add_map(MapType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::Integer,
        ));
        let b = types.add_map(MapType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::Integer,
        ));
        assert!(a.type_eq(&types, &b));
        assert!(!a.optional().type_eq(&types, &b));
        assert!(!a.type_eq(&types, &b.optional()));
        assert!(a.optional().type_eq(&types, &b.optional()));

        // Map[File, Map[String, Int]] == Map[File, Map[String, Int]]
        let a = types.add_map(MapType::new(PrimitiveTypeKind::File, a));
        let b = types.add_map(MapType::new(PrimitiveTypeKind::File, b));
        assert!(a.type_eq(&types, &b));

        // Map[String, Int] != Map[Int, String]
        let a = types.add_map(MapType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::Integer,
        ));
        let b = types.add_map(MapType::new(
            PrimitiveTypeKind::Integer,
            PrimitiveTypeKind::String,
        ));
        assert!(!a.type_eq(&types, &b));

        assert!(!a.type_eq(&types, &Type::Object));
        assert!(!a.type_eq(&types, &Type::OptionalObject));
        assert!(!a.type_eq(&types, &Type::Union));
        assert!(!a.type_eq(&types, &Type::None));
    }

    #[test]
    fn struct_type_equality() {
        let mut types = Types::new();

        let a = types.add_struct(StructType::new("Foo", [("foo", PrimitiveTypeKind::String)]));
        assert!(a.type_eq(&types, &a));
        assert!(!a.optional().type_eq(&types, &a));
        assert!(!a.type_eq(&types, &a.optional()));
        assert!(a.optional().type_eq(&types, &a.optional()));

        let b = types.add_struct(StructType::new("Foo", [("foo", PrimitiveTypeKind::String)]));
        assert!(!a.type_eq(&types, &b));
    }

    #[test]
    fn object_type_equality() {
        let types = Types::new();
        assert!(Type::Object.type_eq(&types, &Type::Object));
        assert!(!Type::OptionalObject.type_eq(&types, &Type::Object));
        assert!(!Type::Object.type_eq(&types, &Type::OptionalObject));
        assert!(Type::OptionalObject.type_eq(&types, &Type::OptionalObject));
    }

    #[test]
    fn union_type_equality() {
        let types = Types::new();
        assert!(Type::Union.type_eq(&types, &Type::Union));
        assert!(!Type::None.type_eq(&types, &Type::Union));
        assert!(!Type::Union.type_eq(&types, &Type::None));
        assert!(Type::None.type_eq(&types, &Type::None));
    }
}
