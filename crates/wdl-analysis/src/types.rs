//! Representation of the WDL type system.

use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;

use indexmap::IndexMap;
use wdl_ast::Diagnostic;
use wdl_ast::Span;

use crate::document::Input;
use crate::document::Output;

pub mod v1;

/// Used to display a slice of types.
pub fn display_types(slice: &[Type]) -> impl fmt::Display + use<'_> {
    /// Used to display a slice of types.
    struct Display<'a>(&'a [Type]);

    impl fmt::Display for Display<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            for (i, ty) in self.0.iter().enumerate() {
                if i > 0 {
                    if self.0.len() == 2 {
                        write!(f, " ")?;
                    } else {
                        write!(f, ", ")?;
                    }

                    if i == self.0.len() - 1 {
                        write!(f, "or ")?;
                    }
                }

                write!(f, "type `{ty}`")?;
            }

            Ok(())
        }
    }

    Display(slice)
}

/// A trait implemented on type name resolvers.
pub trait TypeNameResolver {
    /// Resolves the given type name to a type.
    fn resolve(&mut self, name: &str, span: Span) -> Result<Type, Diagnostic>;
}

/// A trait implemented on types that may be optional.
pub trait Optional {
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
    fn is_coercible_to(&self, target: &Self) -> bool;
}

/// Represents a primitive WDL type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimitiveType {
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

impl Coercible for PrimitiveType {
    fn is_coercible_to(&self, target: &Self) -> bool {
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

impl fmt::Display for PrimitiveType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Boolean => write!(f, "Boolean")?,
            Self::Integer => write!(f, "Int")?,
            Self::Float => write!(f, "Float")?,
            Self::String => write!(f, "String")?,
            Self::File => write!(f, "File")?,
            Self::Directory => write!(f, "Directory")?,
        }

        Ok(())
    }
}

/// Represents a hidden type in WDL.
///
/// Hidden types are special types used internally for type checking but
/// are not directly expressible in WDL source code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HiddenType {
    /// A hidden type for `hints` that is available in task hints sections.
    Hints,
    /// A hidden type for `input` that is available in task hints sections.
    Input,
    /// A hidden type for `output` that is available in task hints sections.
    Output,
    /// A hidden type for `task` that is available in requirements,
    /// hints, and runtime sections before constraint evaluation.
    TaskPreEvaluation,
    /// A hidden type for `task` that is available in command and output
    /// sections after constraint evaluation.
    TaskPostEvaluation,
    /// A hidden type for `task.previous` that contains the previous
    /// attempt's computed requirements. Available in WDL 1.3+ in both
    /// pre-evaluation (requirements, hints, runtime) and post-evaluation
    /// (command, output) contexts.
    PreviousTaskData,
}

impl fmt::Display for HiddenType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hints => write!(f, "hints"),
            Self::Input => write!(f, "input"),
            Self::Output => write!(f, "output"),
            Self::TaskPreEvaluation | Self::TaskPostEvaluation => write!(f, "task"),
            Self::PreviousTaskData => write!(f, "task.previous"),
        }
    }
}

/// Represents a WDL type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    /// The type is a primitive type.
    ///
    /// The second field is whether or not the primitive type is optional.
    Primitive(PrimitiveType, bool),
    /// The type is a compound type.
    ///
    /// The second field is whether or not the compound type is optional.
    Compound(CompoundType, bool),
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
    /// A special hidden type that is not directly expressible in WDL source.
    ///
    /// Hidden types are used for type checking special values like `task`,
    /// `task.previous`, `hints`, `input`, and `output`.
    Hidden(HiddenType),
    /// The type is a call output.
    Call(CallType),
    /// A reference to a custom type name (struct or enum).
    TypeNameRef(CustomType),
}

impl Type {
    /// Casts the type to a primitive type.
    ///
    /// Returns `None` if the type is not primitive.
    pub fn as_primitive(&self) -> Option<PrimitiveType> {
        match self {
            Self::Primitive(ty, _) => Some(*ty),
            _ => None,
        }
    }

    /// Casts the type to a compound type.
    ///
    /// Returns `None` if the type is not a compound type.
    pub fn as_compound(&self) -> Option<&CompoundType> {
        match self {
            Self::Compound(ty, _) => Some(ty),
            _ => None,
        }
    }

    /// Converts the type to an array type.
    ///
    /// Returns `None` if the type is not an array type.
    pub fn as_array(&self) -> Option<&ArrayType> {
        match self {
            Self::Compound(CompoundType::Array(ty), _) => Some(ty),
            _ => None,
        }
    }

    /// Converts the type to a pair type.
    ///
    /// Returns `None` if the type is not a pair type.
    pub fn as_pair(&self) -> Option<&PairType> {
        match self {
            Self::Compound(CompoundType::Pair(ty), _) => Some(ty),
            _ => None,
        }
    }

    /// Converts the type to a map type.
    ///
    /// Returns `None` if the type is not a map type.
    pub fn as_map(&self) -> Option<&MapType> {
        match self {
            Self::Compound(CompoundType::Map(ty), _) => Some(ty),
            _ => None,
        }
    }

    /// Converts the type to a struct type.
    ///
    /// Returns `None` if the type is not a struct type.
    pub fn as_struct(&self) -> Option<&StructType> {
        match self {
            Self::Compound(CompoundType::Custom(CustomType::Struct(ty)), _) => Some(ty),
            _ => None,
        }
    }

    /// Converts the type to an enum type.
    ///
    /// Returns `None` if the type is not an enum type.
    pub fn as_enum(&self) -> Option<&EnumType> {
        match self {
            Self::Compound(CompoundType::Custom(CustomType::Enum(ty)), _) => Some(ty),
            _ => None,
        }
    }

    /// Converts the type to a custom type.
    ///
    /// Returns `None` if the type is not a custom type.
    pub fn as_custom(&self) -> Option<&CustomType> {
        match self {
            Self::Compound(CompoundType::Custom(ty), _) => Some(ty),
            _ => None,
        }
    }

    /// Converts the type to a type name reference.
    ///
    /// Returns `None` if the type is not a type name reference.
    pub fn as_type_name_ref(&self) -> Option<&CustomType> {
        match self {
            Self::TypeNameRef(custom_ty) => Some(custom_ty),
            _ => None,
        }
    }

    /// Converts the type to a call type
    ///
    /// Returns `None` if the type if not a call type.
    pub fn as_call(&self) -> Option<&CallType> {
        match self {
            Self::Call(ty) => Some(ty),
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

    /// Promotes a type from a scatter statement into the parent scope.
    ///
    /// For most types, this wraps them in an array. For call types, this
    /// promotes each output type into an array.
    pub fn promote_scatter(&self) -> Self {
        // For calls, the outputs of the call are promoted instead of the call itself
        if let Self::Call(ty) = self {
            return Self::Call(ty.promote_scatter());
        }

        Type::Compound(ArrayType::new(self.clone()).into(), false)
    }

    /// Calculates a common type between this type and the given type.
    ///
    /// Returns `None` if the types have no common type.
    pub fn common_type(&self, other: &Type) -> Option<Type> {
        // If the other type is union, then the common type would be this type
        if other.is_union() {
            return Some(self.clone());
        }

        // If this type is union, then the common type would be the other type
        if self.is_union() {
            return Some(other.clone());
        }

        // If the other type is `None`, then the common type would be an optional this
        // type
        if other.is_none() {
            return Some(self.optional());
        }

        // If this type is `None`, then the common type would be an optional other type
        if self.is_none() {
            return Some(other.optional());
        }

        // Check for the other type being coercible to this type
        if other.is_coercible_to(self) {
            return Some(self.clone());
        }

        // Check for this type being coercible to the other type
        if self.is_coercible_to(other) {
            return Some(other.clone());
        }

        // Check for a compound type that might have a common type within it
        if let (Some(this), Some(other)) = (self.as_compound(), other.as_compound())
            && let Some(ty) = this.common_type(other)
        {
            return Some(Self::Compound(ty, self.is_optional()));
        }

        // Check for a call type to have a common type with itself
        if let (Some(this), Some(other)) = (self.as_call(), self.as_call())
            && this == other
        {
            return Some(Self::Call(this.clone()));
        }

        None
    }

    /// Attempts to transform the type into the analogous type name reference.
    ///
    /// This is only supported for custom types (structs and enums).
    pub fn type_name_ref(&self) -> Option<Type> {
        match self {
            Type::Compound(CompoundType::Custom(ty), _) => Some(Type::TypeNameRef(ty.clone())),
            _ => None,
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Primitive(ty, optional) => {
                ty.fmt(f)?;
                if *optional { write!(f, "?") } else { Ok(()) }
            }
            Self::Compound(ty, optional) => {
                ty.fmt(f)?;
                if *optional { write!(f, "?") } else { Ok(()) }
            }
            Self::Object => {
                write!(f, "Object")
            }
            Self::OptionalObject => {
                write!(f, "Object?")
            }
            Self::Union => write!(f, "Union"),
            Self::None => write!(f, "None"),
            Self::Hidden(ty) => ty.fmt(f),
            Self::Call(ty) => ty.fmt(f),
            Self::TypeNameRef(ty) => ty.fmt(f),
        }
    }
}

impl Optional for Type {
    fn is_optional(&self) -> bool {
        match self {
            Self::Primitive(_, optional) => *optional,
            Self::Compound(_, optional) => *optional,
            Self::OptionalObject | Self::None => true,
            Self::Object | Self::Union | Self::Hidden(_) | Self::Call(_) | Self::TypeNameRef(_) => {
                false
            }
        }
    }

    fn optional(&self) -> Self {
        match self {
            Self::Primitive(ty, _) => Self::Primitive(*ty, true),
            Self::Compound(ty, _) => Self::Compound(ty.clone(), true),
            Self::Object => Self::OptionalObject,
            Self::Union => Self::None,
            Self::Call(ty) => Self::Call(ty.optional()),
            ty => ty.clone(),
        }
    }

    fn require(&self) -> Self {
        match self {
            Self::Primitive(ty, _) => Self::Primitive(*ty, false),
            Self::Compound(ty, _) => Self::Compound(ty.clone(), false),
            Self::OptionalObject => Self::Object,
            Self::None => Self::Union,
            ty => ty.clone(),
        }
    }
}

impl Coercible for Type {
    fn is_coercible_to(&self, target: &Self) -> bool {
        if self.eq(target) {
            return true;
        }

        match (self, target) {
            (Self::Primitive(src, src_opt), Self::Primitive(target, target_opt)) => {
                // An optional type cannot coerce into a required type
                if *src_opt && !*target_opt {
                    return false;
                }

                src.is_coercible_to(target)
            }
            (Self::Compound(src, src_opt), Self::Compound(target, target_opt)) => {
                // An optional type cannot coerce into a required type
                if *src_opt && !*target_opt {
                    return false;
                }

                src.is_coercible_to(target)
            }

            // Object -> Object, Object -> Object?, Object? -> Object?
            (Self::Object, Self::Object)
            | (Self::Object, Self::OptionalObject)
            | (Self::OptionalObject, Self::OptionalObject) => true,

            // Map[X, Y] -> Object, Map[X, Y] -> Object?, Map[X, Y]? -> Object? where: X -> String
            //
            // Struct -> Object, Struct -> Object?, Struct? -> Object?
            (Self::Compound(src, false), Self::Object)
            | (Self::Compound(src, false), Self::OptionalObject)
            | (Self::Compound(src, _), Self::OptionalObject) => match src {
                CompoundType::Map(src) => {
                    src.key_type.is_coercible_to(&PrimitiveType::String.into())
                }
                CompoundType::Custom(CustomType::Struct(_)) => true,
                _ => false,
            },

            // Object -> Map[X, Y], Object -> Map[X, Y]?, Object? -> Map[X, Y]? where: String -> X
            // and all object members are coercible to Y
            //
            // Object -> Struct, Object -> Struct?, Object? -> Struct? where: object keys match
            // struct member names and object values are coercible to struct member types
            (Self::Object, Self::Compound(target, _))
            | (Self::OptionalObject, Self::Compound(target, true)) => {
                match target {
                    CompoundType::Map(target) => {
                        Type::from(PrimitiveType::String).is_coercible_to(&target.key_type)
                    }
                    CompoundType::Custom(CustomType::Struct(_)) => {
                        // Note: checking object keys and values is a runtime constraint
                        true
                    }
                    _ => false,
                }
            }

            // Union is always coercible to the target (and vice versa)
            (Self::Union, _) | (_, Self::Union) => true,

            // None is coercible to an optional type
            (Self::None, ty) if ty.is_optional() => true,

            // String -> Enum
            (
                Self::Primitive(PrimitiveType::String, _),
                Self::Compound(CompoundType::Custom(CustomType::Enum(_)), _),
            ) => true,

            // Not coercible
            _ => false,
        }
    }
}

impl From<PrimitiveType> for Type {
    fn from(value: PrimitiveType) -> Self {
        Self::Primitive(value, false)
    }
}

impl From<CompoundType> for Type {
    fn from(value: CompoundType) -> Self {
        Self::Compound(value, false)
    }
}

impl From<ArrayType> for Type {
    fn from(value: ArrayType) -> Self {
        Self::Compound(value.into(), false)
    }
}

impl From<PairType> for Type {
    fn from(value: PairType) -> Self {
        Self::Compound(value.into(), false)
    }
}

impl From<MapType> for Type {
    fn from(value: MapType) -> Self {
        Self::Compound(value.into(), false)
    }
}

impl From<StructType> for Type {
    fn from(value: StructType) -> Self {
        Self::Compound(value.into(), false)
    }
}

impl From<EnumType> for Type {
    fn from(value: EnumType) -> Self {
        Self::Compound(value.into(), false)
    }
}

impl From<CallType> for Type {
    fn from(value: CallType) -> Self {
        Self::Call(value)
    }
}

/// Represents a custom type (struct or enum).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomType {
    /// The type is a struct (e.g. `Foo`).
    Struct(Arc<StructType>),
    /// The type is an enum.
    Enum(Arc<EnumType>),
}

impl CustomType {
    /// Gets the name of the custom type.
    pub fn name(&self) -> &Arc<String> {
        match self {
            Self::Struct(ty) => ty.name(),
            Self::Enum(ty) => ty.name(),
        }
    }

    /// Converts the custom type to a struct type.
    ///
    /// Returns `None` if the custom type is not a struct.
    pub fn as_struct(&self) -> Option<&StructType> {
        match self {
            Self::Struct(ty) => Some(ty),
            _ => None,
        }
    }

    /// Converts the custom type to an enum type.
    ///
    /// Returns `None` if the custom type is not an enum.
    pub fn as_enum(&self) -> Option<&EnumType> {
        match self {
            Self::Enum(ty) => Some(ty),
            _ => None,
        }
    }
}

impl std::fmt::Display for CustomType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CustomType::Struct(ty) => ty.fmt(f),
            CustomType::Enum(ty) => ty.fmt(f),
        }
    }
}

impl From<StructType> for CustomType {
    fn from(value: StructType) -> Self {
        Self::Struct(value.into())
    }
}

impl From<EnumType> for CustomType {
    fn from(value: EnumType) -> Self {
        Self::Enum(value.into())
    }
}

/// Represents a compound type definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompoundType {
    /// The type is an `Array`.
    Array(ArrayType),
    /// The type is a `Pair`.
    Pair(Arc<PairType>),
    /// The type is a `Map`.
    Map(Arc<MapType>),
    /// The type is a custom type (struct or enum).
    Custom(CustomType),
}

impl CompoundType {
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
            Self::Custom(ty) => ty.as_struct(),
            _ => None,
        }
    }

    /// Converts the compound type to an enum type.
    ///
    /// Returns `None` if the compound type is not an enum type.
    pub fn as_enum(&self) -> Option<&EnumType> {
        match self {
            Self::Custom(ty) => ty.as_enum(),
            _ => None,
        }
    }

    /// Converts the compound type to a custom type.
    ///
    /// Returns `None` if the compound type is not a custom type.
    pub fn as_custom(&self) -> Option<&CustomType> {
        match self {
            Self::Custom(ty) => Some(ty),
            _ => None,
        }
    }

    /// Calculates a common type between two compound types.
    ///
    /// This method does not attempt coercion; it only attempts to find common
    /// inner types for the same outer type.
    fn common_type(&self, other: &Self) -> Option<CompoundType> {
        // Check to see if the types are both `Array`, `Pair`, or `Map`; if so, attempt
        // to find a common type for their inner types
        match (self, other) {
            (Self::Array(this), Self::Array(other)) => {
                let element_type = this.element_type.common_type(&other.element_type)?;
                Some(ArrayType::new(element_type).into())
            }
            (Self::Pair(this), Self::Pair(other)) => {
                let left_type = this.left_type.common_type(&other.left_type)?;
                let right_type = this.right_type.common_type(&other.right_type)?;
                Some(PairType::new(left_type, right_type).into())
            }
            (Self::Map(this), Self::Map(other)) => {
                let key_type = this.key_type.common_type(&other.key_type)?;
                let value_type = this.value_type.common_type(&other.value_type)?;
                Some(MapType::new(key_type, value_type).into())
            }
            _ => None,
        }
    }
}

impl fmt::Display for CompoundType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Array(ty) => ty.fmt(f),
            Self::Pair(ty) => ty.fmt(f),
            Self::Map(ty) => ty.fmt(f),
            Self::Custom(ty) => match ty {
                CustomType::Struct(ty) => ty.fmt(f),
                CustomType::Enum(ty) => ty.fmt(f),
            },
        }
    }
}

impl Coercible for CompoundType {
    fn is_coercible_to(&self, target: &Self) -> bool {
        match (self, target) {
            // Array[X] -> Array[Y], Array[X] -> Array[Y]?, Array[X]? -> Array[Y]?, Array[X]+ ->
            // Array[Y] where: X -> Y
            (Self::Array(src), Self::Array(target)) => src.is_coercible_to(target),

            // Pair[W, X] -> Pair[Y, Z], Pair[W, X] -> Pair[Y, Z]?, Pair[W, X]? -> Pair[Y, Z]?
            // where: W -> Y and X -> Z
            (Self::Pair(src), Self::Pair(target)) => src.is_coercible_to(target),

            // Map[W, X] -> Map[Y, Z], Map[W, X] -> Map[Y, Z]?, Map[W, X]? -> Map[Y, Z]? where: W ->
            // Y and X -> Z
            (Self::Map(src), Self::Map(target)) => src.is_coercible_to(target),

            // Struct -> Struct, Struct -> Struct?, Struct? -> Struct? where: all member names match
            // and all member types coerce
            (Self::Custom(CustomType::Struct(src)), Self::Custom(CustomType::Struct(target))) => {
                src.is_coercible_to(target)
            }

            // Enum -> Enum where: same enum type
            (Self::Custom(CustomType::Enum(src)), Self::Custom(CustomType::Enum(target))) => {
                src.is_coercible_to(target)
            }

            // Map[X, Y] -> Struct, Map[X, Y] -> Struct?, Map[X, Y]? -> Struct? where: X -> String,
            // keys match member names, and Y -> member type
            (Self::Map(src), Self::Custom(CustomType::Struct(target))) => {
                if !src.key_type.is_coercible_to(&PrimitiveType::String.into()) {
                    return false;
                }

                // Ensure the value type is coercible to every struct member type
                if !target
                    .members
                    .values()
                    .all(|ty| src.value_type.is_coercible_to(ty))
                {
                    return false;
                }

                // Note: checking map keys is a runtime value constraint
                true
            }

            // Struct -> Map[X, Y], Struct -> Map[X, Y]?, Struct? -> Map[X, Y]? where: String -> X
            // and member types -> Y
            (Self::Custom(CustomType::Struct(src)), Self::Map(target)) => {
                if !Type::from(PrimitiveType::String).is_coercible_to(&target.key_type) {
                    return false;
                }

                // Ensure all the struct members are coercible to the value type
                if !src
                    .members
                    .values()
                    .all(|ty| ty.is_coercible_to(&target.value_type))
                {
                    return false;
                }

                true
            }

            _ => false,
        }
    }
}

impl From<ArrayType> for CompoundType {
    fn from(value: ArrayType) -> Self {
        Self::Array(value)
    }
}

impl From<PairType> for CompoundType {
    fn from(value: PairType) -> Self {
        Self::Pair(value.into())
    }
}

impl From<MapType> for CompoundType {
    fn from(value: MapType) -> Self {
        Self::Map(value.into())
    }
}

impl From<StructType> for CompoundType {
    fn from(value: StructType) -> Self {
        Self::Custom(CustomType::from(value))
    }
}

impl From<EnumType> for CompoundType {
    fn from(value: EnumType) -> Self {
        Self::Custom(CustomType::from(value))
    }
}

impl From<CustomType> for CompoundType {
    fn from(value: CustomType) -> Self {
        Self::Custom(value)
    }
}

/// Represents the type of an `Array`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrayType {
    /// The element type of the array.
    element_type: Arc<Type>,
    /// Whether or not the array type is non-empty.
    non_empty: bool,
}

impl ArrayType {
    /// Constructs a new array type.
    pub fn new(element_type: impl Into<Type>) -> Self {
        Self {
            element_type: Arc::new(element_type.into()),
            non_empty: false,
        }
    }

    /// Constructs a new non-empty array type.
    pub fn non_empty(element_type: impl Into<Type>) -> Self {
        Self {
            element_type: Arc::new(element_type.into()),
            non_empty: true,
        }
    }

    /// Gets the array's element type.
    pub fn element_type(&self) -> &Type {
        &self.element_type
    }

    /// Determines if the array type is non-empty.
    pub fn is_non_empty(&self) -> bool {
        self.non_empty
    }

    /// Consumes the array type and removes the non-empty (`+`) qualifier.
    pub fn unqualified(self) -> ArrayType {
        Self {
            element_type: self.element_type,
            non_empty: false,
        }
    }
}

impl fmt::Display for ArrayType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Array[{ty}]", ty = self.element_type)?;

        if self.non_empty {
            write!(f, "+")?;
        }

        Ok(())
    }
}

impl Coercible for ArrayType {
    fn is_coercible_to(&self, target: &Self) -> bool {
        // Note: non-empty constraints are enforced at runtime and are not checked here.
        self.element_type.is_coercible_to(&target.element_type)
    }
}

/// Represents the type of a `Pair`.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    pub fn left_type(&self) -> &Type {
        &self.left_type
    }

    /// Gets the pairs's right type.
    pub fn right_type(&self) -> &Type {
        &self.right_type
    }
}

impl fmt::Display for PairType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Pair[{left}, {right}]",
            left = self.left_type,
            right = self.right_type
        )?;

        Ok(())
    }
}

impl Coercible for PairType {
    fn is_coercible_to(&self, target: &Self) -> bool {
        self.left_type.is_coercible_to(&target.left_type)
            && self.right_type.is_coercible_to(&target.right_type)
    }
}

/// Represents the type of a `Map`.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    pub fn key_type(&self) -> &Type {
        &self.key_type
    }

    /// Gets the maps's value type.
    pub fn value_type(&self) -> &Type {
        &self.value_type
    }
}

impl fmt::Display for MapType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Map[{key}, {value}]",
            key = self.key_type,
            value = self.value_type
        )?;

        Ok(())
    }
}

impl Coercible for MapType {
    fn is_coercible_to(&self, target: &Self) -> bool {
        self.key_type.is_coercible_to(&target.key_type)
            && self.value_type.is_coercible_to(&target.value_type)
    }
}

/// Represents the type of a struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructType {
    /// The name of the struct.
    name: Arc<String>,
    /// The members of the struct.
    members: IndexMap<String, Type>,
}

impl StructType {
    /// Constructs a new struct type definition.
    pub fn new<N, T>(name: impl Into<String>, members: impl IntoIterator<Item = (N, T)>) -> Self
    where
        N: Into<String>,
        T: Into<Type>,
    {
        Self {
            name: Arc::new(name.into()),
            members: members
                .into_iter()
                .map(|(n, ty)| (n.into(), ty.into()))
                .collect(),
        }
    }

    /// Gets the name of the struct.
    pub fn name(&self) -> &Arc<String> {
        &self.name
    }

    /// Gets the members of the struct.
    pub fn members(&self) -> &IndexMap<String, Type> {
        &self.members
    }
}

impl fmt::Display for StructType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{name}", name = self.name)
    }
}

impl Coercible for StructType {
    fn is_coercible_to(&self, target: &Self) -> bool {
        if self.members.len() != target.members.len() {
            return false;
        }

        self.members.iter().all(|(k, v)| {
            target
                .members
                .get(k)
                .map(|target| v.is_coercible_to(target))
                .unwrap_or(false)
        })
    }
}

/// Represents the type of an enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumType {
    /// The name of the enum.
    name: Arc<String>,
    /// The common coerced type computed from all variant values.
    inner_value_type: Type,
    /// The variants.
    variants: Arc<Vec<String>>,
}

impl EnumType {
    /// Constructs a new enum type with a known coerced type.
    ///
    /// Validates that all variant types are coercible to the provided type.
    ///
    /// Returns an error if any variant cannot be coerced.
    pub fn new(
        enum_name: impl Into<String>,
        enum_span: Span,
        explicit_inner_type: Type,
        variants: Vec<(String, Type)>,
        variant_spans: &[Span],
    ) -> Result<Self, Diagnostic> {
        assert_eq!(variants.len(), variant_spans.len());
        let enum_name = enum_name.into();

        // Validate that all variant types are coercible to the value type
        for (variant_idx, (variant_name, variant_type)) in variants.iter().enumerate() {
            if !variant_type.is_coercible_to(&explicit_inner_type) {
                return Err(crate::diagnostics::enum_variant_does_not_coerce_to_type(
                    &enum_name,
                    enum_span,
                    variant_name,
                    variant_spans[variant_idx],
                    &explicit_inner_type,
                    variant_type,
                ));
            }
        }

        let variants = variants.into_iter().map(|(name, _)| name).collect();

        Ok(Self {
            name: Arc::new(enum_name),
            inner_value_type: explicit_inner_type,
            variants: Arc::new(variants),
        })
    }

    /// Attempts to create a new enum type by computing the common inner type
    /// through coercion.
    ///
    /// Finds the common inner type among all variant types. If the enum has no
    /// variants, the coerced inner type is [`Type::Union`].
    ///
    /// Returns an error if no common type can be found among the variants.
    pub fn infer(
        enum_name: impl Into<String>,
        variants: Vec<(String, Type)>,
        variant_spans: &[Span],
    ) -> Result<Self, Diagnostic> {
        assert_eq!(variants.len(), variant_spans.len());
        let enum_name = enum_name.into();

        let common_inner_type = if variants.is_empty() {
            Type::Union
        } else {
            let mut variants = variants.iter().cloned();
            let mut last_variant_idx = 0;
            // SAFETY: we just checked above that `variants` isn't empty.
            let mut common_inner_type = variants.next().unwrap().1;

            for variant in variants {
                match common_inner_type.common_type(&variant.1) {
                    Some(common) => {
                        last_variant_idx += 1;
                        common_inner_type = common
                    }
                    None => {
                        return Err(crate::diagnostics::no_common_inferred_type_for_enum(
                            &enum_name,
                            &common_inner_type,
                            variant_spans[last_variant_idx],
                            &variant.1,
                            variant_spans[last_variant_idx + 1],
                        ));
                    }
                }
            }

            common_inner_type
        };

        let variants = variants.into_iter().map(|(name, _)| name).collect();

        Ok(Self {
            name: Arc::new(enum_name),
            inner_value_type: common_inner_type,
            variants: Arc::new(variants),
        })
    }

    /// Gets the name of the enum.
    pub fn name(&self) -> &Arc<String> {
        &self.name
    }

    /// Gets the inner value type that all variants coerce to.
    pub fn inner_value_type(&self) -> &Type {
        &self.inner_value_type
    }

    /// Gets the variants with their types.
    pub fn variants(&self) -> &Vec<String> {
        &self.variants
    }
}

impl fmt::Display for EnumType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{name}", name = self.name)
    }
}

impl Coercible for EnumType {
    fn is_coercible_to(&self, _: &Self) -> bool {
        false
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
#[derive(Debug, Clone, Eq)]
pub struct CallType {
    /// The call kind.
    kind: CallKind,
    /// The namespace of the call.
    namespace: Option<Arc<String>>,
    /// The name of the task or workflow that was called.
    name: Arc<String>,
    /// The set of specified inputs in the call.
    specified: Arc<HashSet<String>>,
    /// The input types to the call.
    inputs: Arc<IndexMap<String, Input>>,
    /// The output types from the call.
    outputs: Arc<IndexMap<String, Output>>,
}

impl CallType {
    /// Constructs a new call type given the task or workflow name being called.
    pub fn new(
        kind: CallKind,
        name: impl Into<String>,
        specified: Arc<HashSet<String>>,
        inputs: Arc<IndexMap<String, Input>>,
        outputs: Arc<IndexMap<String, Output>>,
    ) -> Self {
        Self {
            kind,
            namespace: None,
            name: Arc::new(name.into()),
            specified,
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
        specified: Arc<HashSet<String>>,
        inputs: Arc<IndexMap<String, Input>>,
        outputs: Arc<IndexMap<String, Output>>,
    ) -> Self {
        Self {
            kind,
            namespace: Some(Arc::new(namespace.into())),
            name: Arc::new(name.into()),
            specified,
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

    /// Gets the set of inputs specified in the call.
    pub fn specified(&self) -> &HashSet<String> {
        &self.specified
    }

    /// Gets the inputs of the called workflow or task.
    pub fn inputs(&self) -> &IndexMap<String, Input> {
        &self.inputs
    }

    /// Gets the outputs of the called workflow or task.
    pub fn outputs(&self) -> &IndexMap<String, Output> {
        &self.outputs
    }

    /// Makes all outputs of the call type optional.
    pub fn optional(&self) -> Self {
        let mut ty = self.clone();
        for output in Arc::make_mut(&mut ty.outputs).values_mut() {
            *output = Output::new(output.ty().optional(), output.name_span());
        }

        ty
    }

    /// Promotes the call type into a scatter statement.
    pub fn promote_scatter(&self) -> Self {
        let mut ty = self.clone();
        for output in Arc::make_mut(&mut ty.outputs).values_mut() {
            *output = Output::new(output.ty().promote_scatter(), output.name_span());
        }

        ty
    }
}

impl Coercible for CallType {
    fn is_coercible_to(&self, _: &Self) -> bool {
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

impl PartialEq for CallType {
    fn eq(&self, other: &Self) -> bool {
        // Each call type instance is unique, so just compare pointer
        std::ptr::eq(self, other)
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn primitive_type_display() {
        assert_eq!(PrimitiveType::Boolean.to_string(), "Boolean");
        assert_eq!(PrimitiveType::Integer.to_string(), "Int");
        assert_eq!(PrimitiveType::Float.to_string(), "Float");
        assert_eq!(PrimitiveType::String.to_string(), "String");
        assert_eq!(PrimitiveType::File.to_string(), "File");
        assert_eq!(PrimitiveType::Directory.to_string(), "Directory");
        assert_eq!(
            Type::from(PrimitiveType::Boolean).optional().to_string(),
            "Boolean?"
        );
        assert_eq!(
            Type::from(PrimitiveType::Integer).optional().to_string(),
            "Int?"
        );
        assert_eq!(
            Type::from(PrimitiveType::Float).optional().to_string(),
            "Float?"
        );
        assert_eq!(
            Type::from(PrimitiveType::String).optional().to_string(),
            "String?"
        );
        assert_eq!(
            Type::from(PrimitiveType::File).optional().to_string(),
            "File?"
        );
        assert_eq!(
            Type::from(PrimitiveType::Directory).optional().to_string(),
            "Directory?"
        );
    }

    #[test]
    fn array_type_display() {
        assert_eq!(
            ArrayType::new(PrimitiveType::String).to_string(),
            "Array[String]"
        );
        assert_eq!(
            ArrayType::non_empty(PrimitiveType::String).to_string(),
            "Array[String]+"
        );

        let ty: Type = ArrayType::new(ArrayType::new(PrimitiveType::String)).into();
        assert_eq!(ty.to_string(), "Array[Array[String]]");

        let ty = Type::from(ArrayType::non_empty(
            Type::from(ArrayType::non_empty(
                Type::from(PrimitiveType::String).optional(),
            ))
            .optional(),
        ))
        .optional();
        assert_eq!(ty.to_string(), "Array[Array[String?]+?]+?");
    }

    #[test]
    fn pair_type_display() {
        assert_eq!(
            PairType::new(PrimitiveType::String, PrimitiveType::Boolean).to_string(),
            "Pair[String, Boolean]"
        );

        let ty: Type = PairType::new(
            ArrayType::new(PrimitiveType::String),
            ArrayType::new(PrimitiveType::String),
        )
        .into();
        assert_eq!(ty.to_string(), "Pair[Array[String], Array[String]]");

        let ty = Type::from(PairType::new(
            Type::from(ArrayType::non_empty(
                Type::from(PrimitiveType::File).optional(),
            ))
            .optional(),
            Type::from(ArrayType::non_empty(
                Type::from(PrimitiveType::File).optional(),
            ))
            .optional(),
        ))
        .optional();
        assert_eq!(ty.to_string(), "Pair[Array[File?]+?, Array[File?]+?]?");
    }

    #[test]
    fn map_type_display() {
        assert_eq!(
            MapType::new(PrimitiveType::String, PrimitiveType::Boolean).to_string(),
            "Map[String, Boolean]"
        );

        let ty: Type = MapType::new(
            PrimitiveType::Boolean,
            ArrayType::new(PrimitiveType::String),
        )
        .into();
        assert_eq!(ty.to_string(), "Map[Boolean, Array[String]]");

        let ty: Type = Type::from(MapType::new(
            PrimitiveType::String,
            Type::from(ArrayType::non_empty(
                Type::from(PrimitiveType::File).optional(),
            ))
            .optional(),
        ))
        .optional();
        assert_eq!(ty.to_string(), "Map[String, Array[File?]+?]?");
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
        assert_eq!(Type::Object.to_string(), "Object");
        assert_eq!(Type::OptionalObject.to_string(), "Object?");
    }

    #[test]
    fn union_type_display() {
        assert_eq!(Type::Union.to_string(), "Union");
    }

    #[test]
    fn none_type_display() {
        assert_eq!(Type::None.to_string(), "None");
    }

    #[test]
    fn primitive_type_coercion() {
        // All types should be coercible to self, and required should coerce to optional
        // (but not vice versa)
        for ty in [
            Type::from(PrimitiveType::Boolean),
            PrimitiveType::Directory.into(),
            PrimitiveType::File.into(),
            PrimitiveType::Float.into(),
            PrimitiveType::Integer.into(),
            PrimitiveType::String.into(),
        ] {
            assert!(ty.is_coercible_to(&ty));
            assert!(ty.optional().is_coercible_to(&ty.optional()));
            assert!(ty.is_coercible_to(&ty.optional()));
            assert!(!ty.optional().is_coercible_to(&ty));
        }

        // Check the valid coercions
        assert!(PrimitiveType::String.is_coercible_to(&PrimitiveType::File));
        assert!(PrimitiveType::String.is_coercible_to(&PrimitiveType::Directory));
        assert!(PrimitiveType::Integer.is_coercible_to(&PrimitiveType::Float));
        assert!(PrimitiveType::File.is_coercible_to(&PrimitiveType::String));
        assert!(PrimitiveType::Directory.is_coercible_to(&PrimitiveType::String));
        assert!(!PrimitiveType::Float.is_coercible_to(&PrimitiveType::Integer));
    }

    #[test]
    fn object_type_coercion() {
        assert!(Type::Object.is_coercible_to(&Type::Object));
        assert!(Type::Object.is_coercible_to(&Type::OptionalObject));
        assert!(Type::OptionalObject.is_coercible_to(&Type::OptionalObject));
        assert!(!Type::OptionalObject.is_coercible_to(&Type::Object));

        // Object? -> Map[String, X]
        let ty = MapType::new(PrimitiveType::String, PrimitiveType::String).into();
        assert!(!Type::OptionalObject.is_coercible_to(&ty));

        // Object? -> Map[File, X]
        let ty = MapType::new(PrimitiveType::File, PrimitiveType::String).into();
        assert!(!Type::OptionalObject.is_coercible_to(&ty));

        // Object -> Map[Int, X] (key not coercible from string)
        let ty = MapType::new(PrimitiveType::Integer, PrimitiveType::String).into();
        assert!(!Type::Object.is_coercible_to(&ty));

        // Object -> Map[String, X]?
        let ty = Type::from(MapType::new(PrimitiveType::String, PrimitiveType::String)).optional();
        assert!(Type::Object.is_coercible_to(&ty));

        // Object -> Map[File, X]?
        let ty = Type::from(MapType::new(PrimitiveType::File, PrimitiveType::String)).optional();
        assert!(Type::Object.is_coercible_to(&ty));

        // Object? -> Map[String, X]?
        let ty = Type::from(MapType::new(PrimitiveType::String, PrimitiveType::String)).optional();
        assert!(Type::OptionalObject.is_coercible_to(&ty));

        // Object? -> Map[String, X]
        let ty = MapType::new(PrimitiveType::String, PrimitiveType::String).into();
        assert!(!Type::OptionalObject.is_coercible_to(&ty));

        // Object -> Struct
        let ty = StructType::new("Foo", [("foo", PrimitiveType::String)]).into();
        assert!(Type::Object.is_coercible_to(&ty));

        // Object -> Struct?
        let ty = Type::from(StructType::new("Foo", [("foo", PrimitiveType::String)])).optional();
        assert!(Type::Object.is_coercible_to(&ty));

        // Object? -> Struct?
        let ty = Type::from(StructType::new("Foo", [("foo", PrimitiveType::String)])).optional();
        assert!(Type::OptionalObject.is_coercible_to(&ty));

        // Object? -> Struct
        let ty = StructType::new("Foo", [("foo", PrimitiveType::String)]).into();
        assert!(!Type::OptionalObject.is_coercible_to(&ty));
    }

    #[test]
    fn array_type_coercion() {
        // Array[X] -> Array[Y]
        assert!(
            ArrayType::new(PrimitiveType::String)
                .is_coercible_to(&ArrayType::new(PrimitiveType::String))
        );
        assert!(
            ArrayType::new(PrimitiveType::File)
                .is_coercible_to(&ArrayType::new(PrimitiveType::String))
        );
        assert!(
            ArrayType::new(PrimitiveType::String)
                .is_coercible_to(&ArrayType::new(PrimitiveType::File))
        );

        // Array[X] -> Array[Y?]
        let type1: Type = ArrayType::new(PrimitiveType::String).into();
        let type2 = ArrayType::new(Type::from(PrimitiveType::File).optional()).into();
        assert!(type1.is_coercible_to(&type2));
        assert!(!type2.is_coercible_to(&type1));

        // Array[Array[X]] -> Array[Array[Y]]
        let type1: Type = ArrayType::new(type1).into();
        let type2 = ArrayType::new(type2).into();
        assert!(type1.is_coercible_to(&type2));
        assert!(!type2.is_coercible_to(&type1));

        // Array[X]+ -> Array[Y]
        let type1: Type = ArrayType::non_empty(PrimitiveType::String).into();
        let type2 = ArrayType::new(Type::from(PrimitiveType::File).optional()).into();
        assert!(type1.is_coercible_to(&type2));
        assert!(!type2.is_coercible_to(&type1));

        // Array[X]+ -> Array[X?]
        let type1: Type = ArrayType::non_empty(PrimitiveType::String).into();
        let type2 = ArrayType::new(Type::from(PrimitiveType::String).optional()).into();
        assert!(type1.is_coercible_to(&type2));
        assert!(!type2.is_coercible_to(&type1));

        // Array[X] -> Array[X]
        let type1: Type = ArrayType::new(PrimitiveType::String).into();
        let type2 = ArrayType::new(PrimitiveType::String).into();
        assert!(type1.is_coercible_to(&type2));
        assert!(type2.is_coercible_to(&type1));

        // Array[X]? -> Array[X]?
        let type1 = Type::from(ArrayType::new(PrimitiveType::String)).optional();
        let type2 = Type::from(ArrayType::new(PrimitiveType::String)).optional();
        assert!(type1.is_coercible_to(&type2));
        assert!(type2.is_coercible_to(&type1));

        // Array[X] -> Array[X]?
        let type1: Type = ArrayType::new(PrimitiveType::String).into();
        let type2 = Type::from(ArrayType::new(PrimitiveType::String)).optional();
        assert!(type1.is_coercible_to(&type2));
        assert!(!type2.is_coercible_to(&type1));
    }

    #[test]
    fn pair_type_coercion() {
        // Pair[W, X] -> Pair[Y, Z]
        assert!(
            PairType::new(PrimitiveType::String, PrimitiveType::String)
                .is_coercible_to(&PairType::new(PrimitiveType::String, PrimitiveType::String))
        );
        assert!(
            PairType::new(PrimitiveType::String, PrimitiveType::String).is_coercible_to(
                &PairType::new(PrimitiveType::File, PrimitiveType::Directory)
            )
        );
        assert!(
            PairType::new(PrimitiveType::File, PrimitiveType::Directory)
                .is_coercible_to(&PairType::new(PrimitiveType::String, PrimitiveType::String))
        );

        // Pair[W, X] -> Pair[Y?, Z?]
        let type1: Type = PairType::new(PrimitiveType::String, PrimitiveType::String).into();
        let type2 = PairType::new(
            Type::from(PrimitiveType::File).optional(),
            Type::from(PrimitiveType::Directory).optional(),
        )
        .into();
        assert!(type1.is_coercible_to(&type2));
        assert!(!type2.is_coercible_to(&type1));

        // Pair[Pair[W, X], Pair[W, X]] -> Pair[Pair[Y, Z], Pair[Y, Z]]
        let type1: Type = PairType::new(type1.clone(), type1).into();
        let type2 = PairType::new(type2.clone(), type2).into();
        assert!(type1.is_coercible_to(&type2));
        assert!(!type2.is_coercible_to(&type1));

        // Pair[W, X] -> Pair[W, X]
        let type1: Type = PairType::new(PrimitiveType::String, PrimitiveType::String).into();
        let type2 = PairType::new(PrimitiveType::String, PrimitiveType::String).into();
        assert!(type1.is_coercible_to(&type2));
        assert!(type2.is_coercible_to(&type1));

        // Pair[W, X]? -> Pair[W, X]?
        let type1 =
            Type::from(PairType::new(PrimitiveType::String, PrimitiveType::String)).optional();
        let type2 =
            Type::from(PairType::new(PrimitiveType::String, PrimitiveType::String)).optional();
        assert!(type1.is_coercible_to(&type2));
        assert!(type2.is_coercible_to(&type1));

        // Pair[W, X] -> Pair[W, X]?
        let type1: Type = PairType::new(PrimitiveType::String, PrimitiveType::String).into();
        let type2 =
            Type::from(PairType::new(PrimitiveType::String, PrimitiveType::String)).optional();
        assert!(type1.is_coercible_to(&type2));
        assert!(!type2.is_coercible_to(&type1));
    }

    #[test]
    fn map_type_coercion() {
        // Map[W, X] -> Map[Y, Z]
        assert!(
            MapType::new(PrimitiveType::String, PrimitiveType::String)
                .is_coercible_to(&MapType::new(PrimitiveType::String, PrimitiveType::String))
        );
        assert!(
            MapType::new(PrimitiveType::String, PrimitiveType::String)
                .is_coercible_to(&MapType::new(PrimitiveType::File, PrimitiveType::Directory))
        );
        assert!(
            MapType::new(PrimitiveType::File, PrimitiveType::Directory)
                .is_coercible_to(&MapType::new(PrimitiveType::String, PrimitiveType::String))
        );

        // Map[W, X] -> Map[Y?, Z?]
        let type1: Type = MapType::new(PrimitiveType::String, PrimitiveType::String).into();
        let type2 = MapType::new(
            Type::from(PrimitiveType::File).optional(),
            Type::from(PrimitiveType::Directory).optional(),
        )
        .into();
        assert!(type1.is_coercible_to(&type2));
        assert!(!type2.is_coercible_to(&type1));

        // Map[P, Map[W, X]] -> Map[Q, Map[Y, Z]]
        let type1: Type = MapType::new(PrimitiveType::String, type1).into();
        let type2 = MapType::new(PrimitiveType::Directory, type2).into();
        assert!(type1.is_coercible_to(&type2));
        assert!(!type2.is_coercible_to(&type1));

        // Map[W, X] -> Map[W, X]
        let type1: Type = MapType::new(PrimitiveType::String, PrimitiveType::String).into();
        let type2 = MapType::new(PrimitiveType::String, PrimitiveType::String).into();
        assert!(type1.is_coercible_to(&type2));
        assert!(type2.is_coercible_to(&type1));

        // Map[W, X]? -> Map[W, X]?
        let type1: Type =
            Type::from(MapType::new(PrimitiveType::String, PrimitiveType::String)).optional();
        let type2: Type =
            Type::from(MapType::new(PrimitiveType::String, PrimitiveType::String)).optional();
        assert!(type1.is_coercible_to(&type2));
        assert!(type2.is_coercible_to(&type1));

        // Map[W, X] -> Map[W, X]?
        let type1: Type = MapType::new(PrimitiveType::String, PrimitiveType::String).into();
        let type2 =
            Type::from(MapType::new(PrimitiveType::String, PrimitiveType::String)).optional();
        assert!(type1.is_coercible_to(&type2));
        assert!(!type2.is_coercible_to(&type1));

        // Map[String, Int] -> Struct
        let type1: Type = MapType::new(PrimitiveType::String, PrimitiveType::Integer).into();
        let type2 = StructType::new(
            "Foo",
            [
                ("foo", PrimitiveType::Integer),
                ("bar", PrimitiveType::Integer),
                ("baz", PrimitiveType::Integer),
            ],
        )
        .into();
        assert!(type1.is_coercible_to(&type2));

        // Map[File, Int] -> Struct
        let type1: Type = MapType::new(PrimitiveType::File, PrimitiveType::Integer).into();
        let type2 = StructType::new(
            "Foo",
            [
                ("foo", PrimitiveType::Integer),
                ("bar", PrimitiveType::Integer),
                ("baz", PrimitiveType::Integer),
            ],
        )
        .into();
        assert!(type1.is_coercible_to(&type2));

        // Map[String, Int] -> Struct (mismatched fields)
        let type1: Type = MapType::new(PrimitiveType::String, PrimitiveType::Integer).into();
        let type2 = StructType::new(
            "Foo",
            [
                ("foo", PrimitiveType::Integer),
                ("bar", PrimitiveType::String),
                ("baz", PrimitiveType::Integer),
            ],
        )
        .into();
        assert!(!type1.is_coercible_to(&type2));

        // Map[Int, Int] -> Struct
        let type1: Type = MapType::new(PrimitiveType::Integer, PrimitiveType::Integer).into();
        let type2 = StructType::new(
            "Foo",
            [
                ("foo", PrimitiveType::Integer),
                ("bar", PrimitiveType::Integer),
                ("baz", PrimitiveType::Integer),
            ],
        )
        .into();
        assert!(!type1.is_coercible_to(&type2));

        // Map[String, Int] -> Object
        let type1: Type = MapType::new(PrimitiveType::String, PrimitiveType::Integer).into();
        assert!(type1.is_coercible_to(&Type::Object));

        // Map[String, Int] -> Object?
        let type1: Type = MapType::new(PrimitiveType::String, PrimitiveType::Integer).into();
        assert!(type1.is_coercible_to(&Type::OptionalObject));

        // Map[String, Int]? -> Object?
        let type1: Type =
            Type::from(MapType::new(PrimitiveType::String, PrimitiveType::Integer)).optional();
        assert!(type1.is_coercible_to(&Type::OptionalObject));

        // Map[File, Int] -> Object
        let type1: Type = MapType::new(PrimitiveType::File, PrimitiveType::Integer).into();
        assert!(type1.is_coercible_to(&Type::Object));

        // Map[File, Int] -> Object?
        let type1: Type = MapType::new(PrimitiveType::File, PrimitiveType::Integer).into();
        assert!(type1.is_coercible_to(&Type::OptionalObject));

        // Map[File, Int]? -> Object?
        let type1: Type =
            Type::from(MapType::new(PrimitiveType::File, PrimitiveType::Integer)).optional();
        assert!(type1.is_coercible_to(&Type::OptionalObject));

        // Map[String, Int]? -> Object
        let type1: Type =
            Type::from(MapType::new(PrimitiveType::String, PrimitiveType::Integer)).optional();
        assert!(!type1.is_coercible_to(&Type::Object));

        // Map[File, Int]? -> Object
        let type1: Type =
            Type::from(MapType::new(PrimitiveType::File, PrimitiveType::Integer)).optional();
        assert!(!type1.is_coercible_to(&Type::Object));

        // Map[Integer, Int] -> Object
        let type1: Type = MapType::new(PrimitiveType::Integer, PrimitiveType::Integer).into();
        assert!(!type1.is_coercible_to(&Type::Object));
    }

    #[test]
    fn struct_type_coercion() {
        // S -> S (identical)
        let type1: Type = StructType::new(
            "Foo",
            [
                ("foo", PrimitiveType::String),
                ("bar", PrimitiveType::String),
                ("baz", PrimitiveType::Integer),
            ],
        )
        .into();
        let type2 = StructType::new(
            "Foo",
            [
                ("foo", PrimitiveType::String),
                ("bar", PrimitiveType::String),
                ("baz", PrimitiveType::Integer),
            ],
        )
        .into();
        assert!(type1.is_coercible_to(&type2));
        assert!(type2.is_coercible_to(&type1));

        // S -> S?
        let type1: Type = StructType::new(
            "Foo",
            [
                ("foo", PrimitiveType::String),
                ("bar", PrimitiveType::String),
                ("baz", PrimitiveType::Integer),
            ],
        )
        .into();
        let type2 = Type::from(StructType::new(
            "Foo",
            [
                ("foo", PrimitiveType::String),
                ("bar", PrimitiveType::String),
                ("baz", PrimitiveType::Integer),
            ],
        ))
        .optional();
        assert!(type1.is_coercible_to(&type2));
        assert!(!type2.is_coercible_to(&type1));

        // S? -> S?
        let type1: Type = Type::from(StructType::new(
            "Foo",
            [
                ("foo", PrimitiveType::String),
                ("bar", PrimitiveType::String),
                ("baz", PrimitiveType::Integer),
            ],
        ))
        .optional();
        let type2 = Type::from(StructType::new(
            "Foo",
            [
                ("foo", PrimitiveType::String),
                ("bar", PrimitiveType::String),
                ("baz", PrimitiveType::Integer),
            ],
        ))
        .optional();
        assert!(type1.is_coercible_to(&type2));
        assert!(type2.is_coercible_to(&type1));

        // S -> S (coercible fields)
        let type1: Type = StructType::new(
            "Foo",
            [
                ("foo", PrimitiveType::String),
                ("bar", PrimitiveType::String),
                ("baz", PrimitiveType::Integer),
            ],
        )
        .into();
        let type2 = StructType::new(
            "Bar",
            [
                ("foo", PrimitiveType::File),
                ("bar", PrimitiveType::Directory),
                ("baz", PrimitiveType::Float),
            ],
        )
        .into();
        assert!(type1.is_coercible_to(&type2));
        assert!(!type2.is_coercible_to(&type1));

        // S -> S (mismatched fields)
        let type1: Type = StructType::new(
            "Foo",
            [
                ("foo", PrimitiveType::String),
                ("bar", PrimitiveType::String),
                ("baz", PrimitiveType::Integer),
            ],
        )
        .into();
        let type2 = StructType::new("Bar", [("baz", PrimitiveType::Float)]).into();
        assert!(!type1.is_coercible_to(&type2));
        assert!(!type2.is_coercible_to(&type1));

        // Struct -> Map[String, String]
        let type1: Type = StructType::new(
            "Foo",
            [
                ("foo", PrimitiveType::String),
                ("bar", PrimitiveType::String),
                ("baz", PrimitiveType::String),
            ],
        )
        .into();
        let type2 = MapType::new(PrimitiveType::String, PrimitiveType::String).into();
        assert!(type1.is_coercible_to(&type2));

        // Struct -> Map[File, String]
        let type1: Type = StructType::new(
            "Foo",
            [
                ("foo", PrimitiveType::String),
                ("bar", PrimitiveType::String),
                ("baz", PrimitiveType::String),
            ],
        )
        .into();
        let type2 = MapType::new(PrimitiveType::File, PrimitiveType::String).into();
        assert!(type1.is_coercible_to(&type2));

        // Struct -> Map[String, X] (mismatched types)
        let type1: Type = StructType::new(
            "Foo",
            [
                ("foo", PrimitiveType::String),
                ("bar", PrimitiveType::Integer),
                ("baz", PrimitiveType::String),
            ],
        )
        .into();
        let type2 = MapType::new(PrimitiveType::String, PrimitiveType::String).into();
        assert!(!type1.is_coercible_to(&type2));

        // Struct -> Map[Int, String] (key not coercible from String)
        let type1: Type = StructType::new(
            "Foo",
            [
                ("foo", PrimitiveType::String),
                ("bar", PrimitiveType::String),
                ("baz", PrimitiveType::String),
            ],
        )
        .into();
        let type2 = MapType::new(PrimitiveType::Integer, PrimitiveType::String).into();
        assert!(!type1.is_coercible_to(&type2));

        // Struct -> Object
        assert!(type1.is_coercible_to(&Type::Object));

        // Struct -> Object?
        assert!(type1.is_coercible_to(&Type::OptionalObject));

        // Struct? -> Object?
        let type1: Type =
            Type::from(StructType::new("Foo", [("foo", PrimitiveType::String)])).optional();
        assert!(type1.is_coercible_to(&Type::OptionalObject));

        // Struct? -> Object
        assert!(!type1.is_coercible_to(&Type::Object));
    }

    #[test]
    fn union_type_coercion() {
        // Union -> anything (ok)
        for ty in [
            Type::from(PrimitiveType::Boolean),
            PrimitiveType::Directory.into(),
            PrimitiveType::File.into(),
            PrimitiveType::Float.into(),
            PrimitiveType::Integer.into(),
            PrimitiveType::String.into(),
        ] {
            assert!(Type::Union.is_coercible_to(&ty));
            assert!(Type::Union.is_coercible_to(&ty.optional()));
            assert!(ty.is_coercible_to(&Type::Union));
        }

        for optional in [true, false] {
            // Union -> Array[X], Union -> Array[X]?
            let ty: Type = ArrayType::new(PrimitiveType::String).into();
            let ty = if optional { ty.optional() } else { ty };

            let coercible = Type::Union.is_coercible_to(&ty);
            assert!(coercible);

            // Union -> Pair[X, Y], Union -> Pair[X, Y]?
            let ty: Type = PairType::new(PrimitiveType::String, PrimitiveType::Boolean).into();
            let ty = if optional { ty.optional() } else { ty };
            let coercible = Type::Union.is_coercible_to(&ty);
            assert!(coercible);

            // Union -> Map[X, Y], Union -> Map[X, Y]?
            let ty: Type = MapType::new(PrimitiveType::String, PrimitiveType::Boolean).into();
            let ty = if optional { ty.optional() } else { ty };
            let coercible = Type::Union.is_coercible_to(&ty);
            assert!(coercible);

            // Union -> Struct, Union -> Struct?
            let ty: Type = StructType::new("Foo", [("foo", PrimitiveType::String)]).into();
            let ty = if optional { ty.optional() } else { ty };
            let coercible = Type::Union.is_coercible_to(&ty);
            assert!(coercible);
        }
    }

    #[test]
    fn none_type_coercion() {
        // None -> optional type (ok)
        for ty in [
            Type::from(PrimitiveType::Boolean),
            PrimitiveType::Directory.into(),
            PrimitiveType::File.into(),
            PrimitiveType::Float.into(),
            PrimitiveType::Integer.into(),
            PrimitiveType::String.into(),
        ] {
            assert!(!Type::None.is_coercible_to(&ty));
            assert!(Type::None.is_coercible_to(&ty.optional()));
            assert!(!ty.is_coercible_to(&Type::None));
        }

        for optional in [true, false] {
            // None -> Array[X], None -> Array[X]?
            let ty: Type = ArrayType::new(PrimitiveType::String).into();
            let ty = if optional { ty.optional() } else { ty };
            let coercible = Type::None.is_coercible_to(&ty);
            if optional {
                assert!(coercible);
            } else {
                assert!(!coercible);
            }

            // None -> Pair[X, Y], None -> Pair[X, Y]?
            let ty: Type = PairType::new(PrimitiveType::String, PrimitiveType::Boolean).into();
            let ty = if optional { ty.optional() } else { ty };
            let coercible = Type::None.is_coercible_to(&ty);
            if optional {
                assert!(coercible);
            } else {
                assert!(!coercible);
            }

            // None -> Map[X, Y], None -> Map[X, Y]?
            let ty: Type = MapType::new(PrimitiveType::String, PrimitiveType::Boolean).into();
            let ty = if optional { ty.optional() } else { ty };
            let coercible = Type::None.is_coercible_to(&ty);
            if optional {
                assert!(coercible);
            } else {
                assert!(!coercible);
            }

            // None -> Struct, None -> Struct?
            let ty: Type = StructType::new("Foo", [("foo", PrimitiveType::String)]).into();
            let ty = if optional { ty.optional() } else { ty };
            let coercible = Type::None.is_coercible_to(&ty);
            if optional {
                assert!(coercible);
            } else {
                assert!(!coercible);
            }
        }
    }

    #[test]
    fn primitive_equality() {
        for ty in [
            Type::from(PrimitiveType::Boolean),
            PrimitiveType::Directory.into(),
            PrimitiveType::File.into(),
            PrimitiveType::Float.into(),
            PrimitiveType::Integer.into(),
            PrimitiveType::String.into(),
        ] {
            assert!(ty.eq(&ty));
            assert!(!ty.optional().eq(&ty));
            assert!(!ty.eq(&ty.optional()));
            assert!(ty.optional().eq(&ty.optional()));
            assert!(!ty.eq(&Type::Object));
            assert!(!ty.eq(&Type::OptionalObject));
            assert!(!ty.eq(&Type::Union));
            assert!(!ty.eq(&Type::None));
        }
    }

    #[test]
    fn array_equality() {
        // Array[String] == Array[String]
        let a: Type = ArrayType::new(PrimitiveType::String).into();
        let b: Type = ArrayType::new(PrimitiveType::String).into();
        assert!(a.eq(&b));
        assert!(!a.optional().eq(&b));
        assert!(!a.eq(&b.optional()));
        assert!(a.optional().eq(&b.optional()));

        // Array[Array[String]] == Array[Array[String]
        let a: Type = ArrayType::new(a).into();
        let b: Type = ArrayType::new(b).into();
        assert!(a.eq(&b));

        // Array[Array[Array[String]]]+ == Array[Array[Array[String]]+
        let a: Type = ArrayType::non_empty(a).into();
        let b: Type = ArrayType::non_empty(b).into();
        assert!(a.eq(&b));

        // Array[String] != Array[String]+
        let a: Type = ArrayType::new(PrimitiveType::String).into();
        let b: Type = ArrayType::non_empty(PrimitiveType::String).into();
        assert!(!a.eq(&b));

        // Array[String] != Array[Int]
        let a: Type = ArrayType::new(PrimitiveType::String).into();
        let b: Type = ArrayType::new(PrimitiveType::Integer).into();
        assert!(!a.eq(&b));

        assert!(!a.eq(&Type::Object));
        assert!(!a.eq(&Type::OptionalObject));
        assert!(!a.eq(&Type::Union));
        assert!(!a.eq(&Type::None));
    }

    #[test]
    fn pair_equality() {
        // Pair[String, Int] == Pair[String, Int]
        let a: Type = PairType::new(PrimitiveType::String, PrimitiveType::Integer).into();
        let b: Type = PairType::new(PrimitiveType::String, PrimitiveType::Integer).into();
        assert!(a.eq(&b));
        assert!(!a.optional().eq(&b));
        assert!(!a.eq(&b.optional()));
        assert!(a.optional().eq(&b.optional()));

        // Pair[Pair[String, Int], Pair[String, Int]] == Pair[Pair[String, Int],
        // Pair[String, Int]]
        let a: Type = PairType::new(a.clone(), a).into();
        let b: Type = PairType::new(b.clone(), b).into();
        assert!(a.eq(&b));

        // Pair[String, Int] != Pair[String, Int]?
        let a: Type = PairType::new(PrimitiveType::String, PrimitiveType::Integer).into();
        let b: Type =
            Type::from(PairType::new(PrimitiveType::String, PrimitiveType::Integer)).optional();
        assert!(!a.eq(&b));

        assert!(!a.eq(&Type::Object));
        assert!(!a.eq(&Type::OptionalObject));
        assert!(!a.eq(&Type::Union));
        assert!(!a.eq(&Type::None));
    }

    #[test]
    fn map_equality() {
        // Map[String, Int] == Map[String, Int]
        let a: Type = MapType::new(PrimitiveType::String, PrimitiveType::Integer).into();
        let b = MapType::new(PrimitiveType::String, PrimitiveType::Integer).into();
        assert!(a.eq(&b));
        assert!(!a.optional().eq(&b));
        assert!(!a.eq(&b.optional()));
        assert!(a.optional().eq(&b.optional()));

        // Map[File, Map[String, Int]] == Map[File, Map[String, Int]]
        let a: Type = MapType::new(PrimitiveType::File, a).into();
        let b = MapType::new(PrimitiveType::File, b).into();
        assert!(a.eq(&b));

        // Map[String, Int] != Map[Int, String]
        let a: Type = MapType::new(PrimitiveType::String, PrimitiveType::Integer).into();
        let b = MapType::new(PrimitiveType::Integer, PrimitiveType::String).into();
        assert!(!a.eq(&b));

        assert!(!a.eq(&Type::Object));
        assert!(!a.eq(&Type::OptionalObject));
        assert!(!a.eq(&Type::Union));
        assert!(!a.eq(&Type::None));
    }

    #[test]
    fn struct_equality() {
        let a: Type = StructType::new("Foo", [("foo", PrimitiveType::String)]).into();
        assert!(a.eq(&a));
        assert!(!a.optional().eq(&a));
        assert!(!a.eq(&a.optional()));
        assert!(a.optional().eq(&a.optional()));

        let b: Type = StructType::new("Foo", [("foo", PrimitiveType::String)]).into();
        assert!(a.eq(&b));
        let b: Type = StructType::new("Bar", [("foo", PrimitiveType::String)]).into();
        assert!(!a.eq(&b));
    }

    #[test]
    fn object_equality() {
        assert!(Type::Object.eq(&Type::Object));
        assert!(!Type::OptionalObject.eq(&Type::Object));
        assert!(!Type::Object.eq(&Type::OptionalObject));
        assert!(Type::OptionalObject.eq(&Type::OptionalObject));
    }

    #[test]
    fn union_equality() {
        assert!(Type::Union.eq(&Type::Union));
        assert!(!Type::None.eq(&Type::Union));
        assert!(!Type::Union.eq(&Type::None));
        assert!(Type::None.eq(&Type::None));
    }

    #[test]
    fn enum_type_new_with_explicit_type() {
        // Create enum with explicit `String` type, all variants coerce to `String`.
        let result = EnumType::new(
            "Status",
            Span::new(0, 0),
            PrimitiveType::String.into(),
            vec![
                ("Pending".into(), PrimitiveType::String.into()),
                ("Running".into(), PrimitiveType::String.into()),
                ("Complete".into(), PrimitiveType::String.into()),
            ],
            &[Span::new(0, 0), Span::new(0, 0), Span::new(0, 0)][..],
        );

        let status = result.unwrap();
        assert_eq!(status.name().as_ref(), "Status");
        assert_eq!(
            status.inner_value_type(),
            &Type::from(PrimitiveType::String)
        );
        assert_eq!(status.variants().len(), 3);
    }

    #[test]
    fn enum_type_new_fails_when_not_coercible() {
        // Try to create enum with `Int` type but `String` variants.
        let result = EnumType::new(
            "Bad",
            Span::new(0, 0),
            PrimitiveType::Integer.into(),
            vec![
                ("First".into(), PrimitiveType::String.into()),
                ("Second".into(), PrimitiveType::Integer.into()),
            ],
            &[Span::new(0, 0), Span::new(0, 0)][..],
        );

        assert!(
            matches!(result, Err(diagnostic) if diagnostic.message() == "cannot coerce variant `First` in enum `Bad` from type `String` to type `Int`")
        );
    }

    #[test]
    fn enum_type_infer_finds_common_type() {
        // All `Int` variants should infer `Int` type.
        let result = EnumType::infer(
            "Priority",
            vec![
                ("Low".into(), PrimitiveType::Integer.into()),
                ("Medium".into(), PrimitiveType::Integer.into()),
                ("High".into(), PrimitiveType::Integer.into()),
            ],
            &[Span::new(0, 0), Span::new(0, 0), Span::new(0, 0)],
        );

        let priority = result.unwrap();
        assert_eq!(priority.name().as_str(), "Priority");
        assert_eq!(
            priority.inner_value_type(),
            &Type::from(PrimitiveType::Integer)
        );
        assert_eq!(priority.variants().len(), 3);
    }

    #[test]
    fn enum_type_infer_coerces_int_to_float() {
        // Mix of `Int` and `Float` should coerce to `Float`.
        let result = EnumType::infer(
            "Mixed",
            vec![
                ("IntValue".into(), PrimitiveType::Integer.into()),
                ("FloatValue".into(), PrimitiveType::Float.into()),
            ],
            &[Span::new(0, 0), Span::new(0, 0)],
        );

        let mixed = result.unwrap();
        assert_eq!(mixed.name().as_str(), "Mixed");
        assert_eq!(mixed.inner_value_type(), &Type::from(PrimitiveType::Float));
        assert_eq!(mixed.variants().len(), 2);
    }

    #[test]
    fn enum_type_infer_fails_without_common_type() {
        // `String` and `Int` have no common type.
        let result = EnumType::infer(
            "Bad",
            vec![
                ("StringVal".into(), PrimitiveType::String.into()),
                ("IntVal".into(), PrimitiveType::Integer.into()),
            ],
            &[Span::new(0, 0), Span::new(0, 0)][..],
        );

        assert!(
            matches!(result, Err(diagnostic) if diagnostic.message() == "cannot infer a common type for enum `Bad`")
        );
    }

    #[test]
    fn enum_type_empty_has_union_type() {
        // Empty enum should have `Union` type.
        let result = EnumType::infer("Empty", Vec::<(String, Type)>::new(), &[]);

        let empty = result.unwrap();
        assert_eq!(empty.name().as_str(), "Empty");
        assert_eq!(empty.inner_value_type(), &Type::Union);
        assert_eq!(empty.variants().len(), 0);
    }

    #[test]
    fn enum_type_display() {
        let enum_type = EnumType::new(
            "Color",
            Span::new(0, 0),
            PrimitiveType::String.into(),
            vec![("Red".into(), PrimitiveType::String.into())],
            &[Span::new(0, 0)][..],
        )
        .unwrap();
        assert_eq!(enum_type.to_string(), "Color");
    }

    #[test]
    fn enum_type_not_coercible_to_other_enums() {
        let color = EnumType::new(
            "Color",
            Span::new(0, 0),
            PrimitiveType::String.into(),
            vec![("Red".into(), PrimitiveType::String.into())],
            &[Span::new(0, 0)][..],
        )
        .unwrap();
        let status = EnumType::new(
            "Status",
            Span::new(0, 0),
            PrimitiveType::String.into(),
            vec![("Active".into(), PrimitiveType::String.into())],
            &[Span::new(0, 0)][..],
        )
        .unwrap();
        assert!(!color.is_coercible_to(&status));
    }
}
