//! Represents type constraints to standard library functions.

use std::fmt;

use crate::types::CompoundType;
use crate::types::CompoundTypeDef;
use crate::types::Optional;
use crate::types::PrimitiveType;
use crate::types::PrimitiveTypeKind;
use crate::types::Type;
use crate::types::Types;

/// A trait implemented by type constraints.
pub trait Constraint: fmt::Debug + Send + Sync {
    /// Gets a description of the constraint.
    fn description(&self) -> &'static str;

    /// Determines if the given type satisfies the constraint.
    ///
    /// Returns `true` if the constraint is satisfied or false if not.
    fn satisfied(&self, types: &Types, ty: Type) -> bool;
}

/// Represents a constraint that ensures the type is optional.
#[derive(Debug, Copy, Clone)]
pub struct OptionalTypeConstraint;

impl Constraint for OptionalTypeConstraint {
    fn description(&self) -> &'static str {
        "any optional type"
    }

    fn satisfied(&self, _: &Types, ty: Type) -> bool {
        ty.is_optional()
    }
}

/// Represents a constraint that ensure the type can be used in a file size
/// calculation.
///
/// The constraint checks that the type is a compound type that recursively
/// contains a `File` or `Directory` type.
#[derive(Debug, Copy, Clone)]
pub struct SizeableConstraint;

impl Constraint for SizeableConstraint {
    fn description(&self) -> &'static str {
        "any compound type that recursively contains a `File` or `Directory`"
    }

    fn satisfied(&self, types: &Types, ty: Type) -> bool {
        /// Determines if the given primitive type is sizable.
        fn primitive_type_is_sizable(ty: PrimitiveType) -> bool {
            matches!(
                ty.kind(),
                PrimitiveTypeKind::File | PrimitiveTypeKind::Directory
            )
        }

        /// Determines if the given compound type is sizable.
        fn compound_type_is_sizable(types: &Types, ty: CompoundType) -> bool {
            match types.type_definition(ty.definition()) {
                CompoundTypeDef::Array(ty) => type_is_sizable(types, ty.element_type()),
                CompoundTypeDef::Pair(ty) => {
                    type_is_sizable(types, ty.first_type())
                        | type_is_sizable(types, ty.second_type())
                }
                CompoundTypeDef::Map(ty) => {
                    type_is_sizable(types, ty.key_type()) | type_is_sizable(types, ty.value_type())
                }
                CompoundTypeDef::Struct(s) => {
                    s.members().values().any(|ty| type_is_sizable(types, *ty))
                }
                CompoundTypeDef::CallOutput(_) => false,
            }
        }

        /// Determines if the given type is sizable.
        fn type_is_sizable(types: &Types, ty: Type) -> bool {
            match ty {
                Type::Primitive(ty) => primitive_type_is_sizable(ty),
                Type::Compound(ty) => compound_type_is_sizable(types, ty),
                Type::Object | Type::OptionalObject => {
                    // Note: checking the types of an object's members is a runtime constraint
                    true
                }
                // Treat unions as sizable as they can only be checked at runtime
                Type::Union | Type::None => true,
                Type::Task | Type::Hints | Type::Input | Type::Output => false,
            }
        }

        type_is_sizable(types, ty)
    }
}

/// Represents a constraint that ensures the type is any structure.
#[derive(Debug, Copy, Clone)]
pub struct StructConstraint;

impl Constraint for StructConstraint {
    fn description(&self) -> &'static str {
        "any structure"
    }

    fn satisfied(&self, types: &Types, ty: Type) -> bool {
        if let Type::Compound(ty) = ty {
            if let CompoundTypeDef::Struct(_) = types.type_definition(ty.definition()) {
                return true;
            }
        }

        false
    }
}

/// Represents a constraint that ensures the type is JSON serializable.
#[derive(Debug, Copy, Clone)]
pub struct JsonSerializableConstraint;

impl Constraint for JsonSerializableConstraint {
    fn description(&self) -> &'static str {
        "any JSON-serializable type"
    }

    fn satisfied(&self, types: &Types, ty: Type) -> bool {
        /// Determines if the given compound type is JSON serializable.
        fn compound_type_is_serializable(types: &Types, ty: CompoundType) -> bool {
            match types.type_definition(ty.definition()) {
                CompoundTypeDef::Array(ty) => type_is_serializable(types, ty.element_type()),
                CompoundTypeDef::Pair(_) => false,
                CompoundTypeDef::Map(ty) => {
                    !ty.key_type().is_optional()
                        && matches!(ty.key_type(), Type::Primitive(ty) if ty.kind() == PrimitiveTypeKind::String)
                        && type_is_serializable(types, ty.value_type())
                }
                CompoundTypeDef::Struct(s) => s
                    .members()
                    .values()
                    .all(|ty| type_is_serializable(types, *ty)),
                CompoundTypeDef::CallOutput(_) => false,
            }
        }

        /// Determines if the given type is JSON serializable.
        fn type_is_serializable(types: &Types, ty: Type) -> bool {
            match ty {
                // Treat objects and unions as sizable as they can only be checked at runtime
                Type::Primitive(_)
                | Type::Object
                | Type::OptionalObject
                | Type::Union
                | Type::None => true,
                Type::Task | Type::Hints | Type::Input | Type::Output => false,
                Type::Compound(ty) => compound_type_is_serializable(types, ty),
            }
        }

        type_is_serializable(types, ty)
    }
}

/// Represents a constraint that ensures the type is a required primitive type.
#[derive(Debug, Copy, Clone)]
pub struct RequiredPrimitiveTypeConstraint;

impl Constraint for RequiredPrimitiveTypeConstraint {
    fn description(&self) -> &'static str {
        "any required primitive type"
    }

    fn satisfied(&self, _: &Types, ty: Type) -> bool {
        match ty {
            Type::Primitive(ty) => !ty.is_optional(),
            // Treat unions as primitive as they can only be checked at runtime
            Type::Union => true,
            Type::Compound(_)
            | Type::Object
            | Type::OptionalObject
            | Type::None
            | Type::Task
            | Type::Hints
            | Type::Input
            | Type::Output => false,
        }
    }
}

/// Represents a constraint that ensures the type is any primitive type.
#[derive(Debug, Copy, Clone)]
pub struct AnyPrimitiveTypeConstraint;

impl Constraint for AnyPrimitiveTypeConstraint {
    fn description(&self) -> &'static str {
        "any primitive type"
    }

    fn satisfied(&self, _: &Types, ty: Type) -> bool {
        match ty {
            Type::Primitive(_) => true,
            // Treat unions as primitive as they can only be checked at runtime
            Type::Union | Type::None => true,
            Type::Compound(_)
            | Type::Object
            | Type::OptionalObject
            | Type::Task
            | Type::Hints
            | Type::Input
            | Type::Output => false,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::types::ArrayType;
    use crate::types::MapType;
    use crate::types::PairType;
    use crate::types::PrimitiveType;
    use crate::types::StructType;
    use crate::types::Types;

    #[test]
    fn test_optional_constraint() {
        let constraint = OptionalTypeConstraint;
        let mut types = Types::default();

        assert!(constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::Boolean).into()
        ));
        assert!(constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::Integer).into()
        ));
        assert!(constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::Float).into()
        ));
        assert!(constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::String).into()
        ));
        assert!(constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::File).into()
        ));
        assert!(constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::Directory).into()
        ));
        assert!(!constraint.satisfied(&types, PrimitiveTypeKind::Boolean.into()));
        assert!(!constraint.satisfied(&types, PrimitiveTypeKind::Integer.into()));
        assert!(!constraint.satisfied(&types, PrimitiveTypeKind::Float.into()));
        assert!(!constraint.satisfied(&types, PrimitiveTypeKind::String.into()));
        assert!(!constraint.satisfied(&types, PrimitiveTypeKind::File.into()));
        assert!(!constraint.satisfied(&types, PrimitiveTypeKind::Directory.into()));
        assert!(!constraint.satisfied(&types, Type::Object));
        assert!(constraint.satisfied(&types, Type::OptionalObject));
        assert!(!constraint.satisfied(&types, Type::Union));

        let ty = types.add_array(ArrayType::new(PrimitiveType::optional(
            PrimitiveTypeKind::Boolean,
        )));
        assert!(!constraint.satisfied(&types, ty));
        assert!(constraint.satisfied(&types, ty.optional()));

        let ty = types.add_pair(PairType::new(
            PrimitiveTypeKind::Boolean,
            PrimitiveType::optional(PrimitiveTypeKind::Boolean),
        ));
        assert!(!constraint.satisfied(&types, ty));
        assert!(constraint.satisfied(&types, ty.optional()));

        let ty = types.add_map(MapType::new(
            PrimitiveTypeKind::String,
            PrimitiveType::optional(PrimitiveTypeKind::Boolean),
        ));
        assert!(!constraint.satisfied(&types, ty));
        assert!(constraint.satisfied(&types, ty.optional()));

        let ty = types.add_struct(StructType::new("Foo", [("foo", PrimitiveTypeKind::String)]));
        assert!(!constraint.satisfied(&types, ty));
        assert!(constraint.satisfied(&types, ty.optional()));
    }

    #[test]
    fn test_sizable_constraint() {
        let constraint = SizeableConstraint;
        let mut types = Types::default();

        assert!(!constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::Boolean).into()
        ));
        assert!(!constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::Integer).into()
        ));
        assert!(!constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::Float).into()
        ));
        assert!(!constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::String).into()
        ));
        assert!(constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::File).into()
        ));
        assert!(constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::Directory).into()
        ));
        assert!(!constraint.satisfied(&types, PrimitiveTypeKind::Boolean.into()));
        assert!(!constraint.satisfied(&types, PrimitiveTypeKind::Integer.into()));
        assert!(!constraint.satisfied(&types, PrimitiveTypeKind::Float.into()));
        assert!(!constraint.satisfied(&types, PrimitiveTypeKind::String.into()));
        assert!(constraint.satisfied(&types, PrimitiveTypeKind::File.into()));
        assert!(constraint.satisfied(&types, PrimitiveTypeKind::Directory.into()));
        assert!(constraint.satisfied(&types, Type::Object));
        assert!(constraint.satisfied(&types, Type::OptionalObject));
        assert!(constraint.satisfied(&types, Type::Union));

        let ty = types.add_array(ArrayType::new(PrimitiveTypeKind::String));
        assert!(!constraint.satisfied(&types, ty));

        let ty = types
            .add_array(ArrayType::new(PrimitiveTypeKind::File))
            .optional();
        assert!(constraint.satisfied(&types, ty));

        let ty = types.add_pair(PairType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::String,
        ));
        assert!(!constraint.satisfied(&types, ty));

        let ty = types.add_pair(PairType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::File,
        ));
        assert!(constraint.satisfied(&types, ty));

        let ty = types
            .add_pair(PairType::new(
                PrimitiveTypeKind::Directory,
                PrimitiveTypeKind::String,
            ))
            .optional();
        assert!(constraint.satisfied(&types, ty));

        let array = types.add_array(ArrayType::new(PrimitiveTypeKind::String));
        let ty = types
            .add_map(MapType::new(PrimitiveTypeKind::String, array))
            .optional();
        assert!(!constraint.satisfied(&types, ty));

        let array = types
            .add_array(ArrayType::new(PrimitiveTypeKind::File))
            .optional();
        let ty = types.add_map(MapType::new(PrimitiveTypeKind::String, array));
        assert!(constraint.satisfied(&types, ty));

        let ty = types
            .add_map(MapType::new(
                PrimitiveTypeKind::Directory,
                PrimitiveTypeKind::String,
            ))
            .optional();
        assert!(constraint.satisfied(&types, ty));

        let ty = types.add_struct(StructType::new("Foo", [("foo", PrimitiveTypeKind::String)]));
        assert!(!constraint.satisfied(&types, ty));

        let ty = types
            .add_struct(StructType::new("Foo", [("foo", PrimitiveTypeKind::File)]))
            .optional();
        assert!(constraint.satisfied(&types, ty));

        let ty = types.add_struct(StructType::new("Foo", [(
            "foo",
            PrimitiveTypeKind::Directory,
        )]));
        assert!(constraint.satisfied(&types, ty));
    }

    #[test]
    fn test_struct_constraint() {
        let constraint = StructConstraint;
        let mut types = Types::default();

        assert!(!constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::Boolean).into()
        ));
        assert!(!constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::Integer).into()
        ));
        assert!(!constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::Float).into()
        ));
        assert!(!constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::String).into()
        ));
        assert!(!constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::File).into()
        ));
        assert!(!constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::Directory).into()
        ));
        assert!(!constraint.satisfied(&types, PrimitiveTypeKind::Boolean.into()));
        assert!(!constraint.satisfied(&types, PrimitiveTypeKind::Integer.into()));
        assert!(!constraint.satisfied(&types, PrimitiveTypeKind::Float.into()));
        assert!(!constraint.satisfied(&types, PrimitiveTypeKind::String.into()));
        assert!(!constraint.satisfied(&types, PrimitiveTypeKind::File.into()));
        assert!(!constraint.satisfied(&types, PrimitiveTypeKind::Directory.into()));
        assert!(!constraint.satisfied(&types, Type::Object));
        assert!(!constraint.satisfied(&types, Type::OptionalObject));
        assert!(!constraint.satisfied(&types, Type::Union));

        let ty = types.add_array(ArrayType::non_empty(PrimitiveTypeKind::String));
        assert!(!constraint.satisfied(&types, ty));

        let ty = types
            .add_pair(PairType::new(
                PrimitiveTypeKind::String,
                PrimitiveTypeKind::String,
            ))
            .optional();
        assert!(!constraint.satisfied(&types, ty));

        let ty = types.add_map(MapType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::String,
        ));
        assert!(!constraint.satisfied(&types, ty));

        let ty = types
            .add_struct(StructType::new("Foo", [("foo", PrimitiveTypeKind::String)]))
            .optional();
        assert!(constraint.satisfied(&types, ty));
    }

    #[test]
    fn test_json_constraint() {
        let constraint = JsonSerializableConstraint;
        let mut types = Types::default();

        assert!(constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::Boolean).into()
        ));
        assert!(constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::Integer).into()
        ));
        assert!(constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::Float).into()
        ));
        assert!(constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::String).into()
        ));
        assert!(constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::File).into()
        ));
        assert!(constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::Directory).into()
        ));
        assert!(constraint.satisfied(&types, PrimitiveTypeKind::Boolean.into()));
        assert!(constraint.satisfied(&types, PrimitiveTypeKind::Integer.into()));
        assert!(constraint.satisfied(&types, PrimitiveTypeKind::Float.into()));
        assert!(constraint.satisfied(&types, PrimitiveTypeKind::String.into()));
        assert!(constraint.satisfied(&types, PrimitiveTypeKind::File.into()));
        assert!(constraint.satisfied(&types, PrimitiveTypeKind::Directory.into()));
        assert!(constraint.satisfied(&types, Type::Object));
        assert!(constraint.satisfied(&types, Type::OptionalObject));
        assert!(constraint.satisfied(&types, Type::Union));

        let ty = types
            .add_array(ArrayType::new(PrimitiveTypeKind::String))
            .optional();
        assert!(constraint.satisfied(&types, ty));

        let ty = types.add_pair(PairType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::String,
        ));
        assert!(!constraint.satisfied(&types, ty));

        let ty = types
            .add_map(MapType::new(
                PrimitiveTypeKind::String,
                PrimitiveTypeKind::String,
            ))
            .optional();
        assert!(constraint.satisfied(&types, ty));

        let ty = types.add_map(MapType::new(
            PrimitiveTypeKind::Integer,
            PrimitiveTypeKind::String,
        ));
        assert!(!constraint.satisfied(&types, ty));

        let ty = types
            .add_struct(StructType::new("Foo", [("foo", PrimitiveTypeKind::String)]))
            .optional();
        assert!(constraint.satisfied(&types, ty));
    }

    #[test]
    fn test_required_primitive_constraint() {
        let constraint = RequiredPrimitiveTypeConstraint;
        let mut types = Types::default();

        assert!(!constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::Boolean).into()
        ));
        assert!(!constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::Integer).into()
        ));
        assert!(!constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::Float).into()
        ));
        assert!(!constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::String).into()
        ));
        assert!(!constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::File).into()
        ));
        assert!(!constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::Directory).into()
        ));
        assert!(constraint.satisfied(&types, PrimitiveTypeKind::Boolean.into()));
        assert!(constraint.satisfied(&types, PrimitiveTypeKind::Integer.into()));
        assert!(constraint.satisfied(&types, PrimitiveTypeKind::Float.into()));
        assert!(constraint.satisfied(&types, PrimitiveTypeKind::String.into()));
        assert!(constraint.satisfied(&types, PrimitiveTypeKind::File.into()));
        assert!(constraint.satisfied(&types, PrimitiveTypeKind::Directory.into()));
        assert!(!constraint.satisfied(&types, Type::Object));
        assert!(!constraint.satisfied(&types, Type::OptionalObject));
        assert!(constraint.satisfied(&types, Type::Union));
        assert!(!constraint.satisfied(&types, Type::None));

        let ty = types.add_array(ArrayType::non_empty(PrimitiveTypeKind::String));
        assert!(!constraint.satisfied(&types, ty));

        let ty = types
            .add_pair(PairType::new(
                PrimitiveTypeKind::String,
                PrimitiveTypeKind::String,
            ))
            .optional();
        assert!(!constraint.satisfied(&types, ty));

        let ty = types.add_map(MapType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::String,
        ));
        assert!(!constraint.satisfied(&types, ty));

        let ty = types
            .add_struct(StructType::new("Foo", [("foo", PrimitiveTypeKind::String)]))
            .optional();
        assert!(!constraint.satisfied(&types, ty));
    }

    #[test]
    fn test_any_primitive_constraint() {
        let constraint = AnyPrimitiveTypeConstraint;
        let mut types = Types::default();

        assert!(constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::Boolean).into()
        ));
        assert!(constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::Integer).into()
        ));
        assert!(constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::Float).into()
        ));
        assert!(constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::String).into()
        ));
        assert!(constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::File).into()
        ));
        assert!(constraint.satisfied(
            &types,
            PrimitiveType::optional(PrimitiveTypeKind::Directory).into()
        ));
        assert!(constraint.satisfied(&types, PrimitiveTypeKind::Boolean.into()));
        assert!(constraint.satisfied(&types, PrimitiveTypeKind::Integer.into()));
        assert!(constraint.satisfied(&types, PrimitiveTypeKind::Float.into()));
        assert!(constraint.satisfied(&types, PrimitiveTypeKind::String.into()));
        assert!(constraint.satisfied(&types, PrimitiveTypeKind::File.into()));
        assert!(constraint.satisfied(&types, PrimitiveTypeKind::Directory.into()));
        assert!(!constraint.satisfied(&types, Type::Object));
        assert!(!constraint.satisfied(&types, Type::OptionalObject));
        assert!(constraint.satisfied(&types, Type::Union));
        assert!(constraint.satisfied(&types, Type::None));

        let ty = types.add_array(ArrayType::non_empty(PrimitiveTypeKind::String));
        assert!(!constraint.satisfied(&types, ty));

        let ty = types
            .add_pair(PairType::new(
                PrimitiveTypeKind::String,
                PrimitiveTypeKind::String,
            ))
            .optional();
        assert!(!constraint.satisfied(&types, ty));

        let ty = types.add_map(MapType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::String,
        ));
        assert!(!constraint.satisfied(&types, ty));

        let ty = types
            .add_struct(StructType::new("Foo", [("foo", PrimitiveTypeKind::String)]))
            .optional();
        assert!(!constraint.satisfied(&types, ty));
    }
}
