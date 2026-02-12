//! Represents type constraints to standard library functions.

use std::fmt;

use crate::types::Coercible;
use crate::types::CompoundType;
use crate::types::CustomType;
use crate::types::PrimitiveType;
use crate::types::Type;

/// A trait implemented by type constraints.
pub trait Constraint: fmt::Debug + Send + Sync {
    /// Gets a description of the constraint.
    fn description(&self) -> &'static str;

    /// Determines if the given type satisfies the constraint.
    ///
    /// Returns `true` if the constraint is satisfied or false if not.
    fn satisfied(&self, ty: &Type) -> bool;
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

    fn satisfied(&self, ty: &Type) -> bool {
        /// Determines if the given primitive type is sizable.
        fn primitive_type_is_sizable(ty: PrimitiveType) -> bool {
            matches!(ty, PrimitiveType::File | PrimitiveType::Directory)
        }

        /// Determines if the given compound type is sizable.
        fn compound_type_is_sizable(ty: &CompoundType) -> bool {
            match ty {
                CompoundType::Array(ty) => type_is_sizable(ty.element_type()),
                CompoundType::Pair(ty) => {
                    type_is_sizable(ty.left_type()) | type_is_sizable(ty.right_type())
                }
                CompoundType::Map(ty) => {
                    type_is_sizable(ty.key_type()) | type_is_sizable(ty.value_type())
                }
                CompoundType::Custom(CustomType::Struct(s)) => {
                    s.members().values().any(type_is_sizable)
                }
                CompoundType::Custom(CustomType::Enum(_)) => false,
            }
        }

        /// Determines if the given type is sizable.
        fn type_is_sizable(ty: &Type) -> bool {
            match ty {
                Type::Primitive(ty, _) => primitive_type_is_sizable(*ty),
                Type::Compound(ty, _) => compound_type_is_sizable(ty),
                Type::Object | Type::OptionalObject => {
                    // Note: checking the types of an object's members is a runtime constraint
                    true
                }
                // Treat unions as sizable as they can only be checked at runtime
                Type::Union | Type::None => true,
                Type::Hidden(_) | Type::Call(_) | Type::TypeNameRef(_) => false,
            }
        }

        type_is_sizable(ty)
    }
}

/// Represents a constraint that ensures the type is any structure.
#[derive(Debug, Copy, Clone)]
pub struct StructConstraint;

impl Constraint for StructConstraint {
    fn description(&self) -> &'static str {
        "any structure"
    }

    fn satisfied(&self, ty: &Type) -> bool {
        matches!(
            ty,
            Type::Compound(CompoundType::Custom(CustomType::Struct(_)), _)
        )
    }
}

/// Represents a constraint that ensures the type is any structure that contains
/// only primitive types.
#[derive(Debug, Copy, Clone)]
pub struct PrimitiveStructConstraint;

impl Constraint for PrimitiveStructConstraint {
    fn description(&self) -> &'static str {
        "any structure containing only primitive types"
    }

    fn satisfied(&self, ty: &Type) -> bool {
        if let Type::Compound(CompoundType::Custom(CustomType::Struct(ty)), _) = ty {
            return ty
                .members()
                .values()
                .all(|ty| matches!(ty, Type::Primitive(..)));
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

    fn satisfied(&self, ty: &Type) -> bool {
        /// Determines if the given compound type is JSON serializable.
        fn compound_type_is_serializable(ty: &CompoundType) -> bool {
            match ty {
                CompoundType::Array(ty) => type_is_serializable(ty.element_type()),
                CompoundType::Pair(_) => false,
                CompoundType::Map(ty) => {
                    ty.key_type().is_coercible_to(&PrimitiveType::String.into())
                        && type_is_serializable(ty.value_type())
                }
                CompoundType::Custom(CustomType::Struct(s)) => {
                    s.members().values().all(type_is_serializable)
                }
                CompoundType::Custom(CustomType::Enum(_)) => {
                    // Enums always serialize as a string representing the
                    // choice name.
                    true
                }
            }
        }

        /// Determines if the given type is JSON serializable.
        fn type_is_serializable(ty: &Type) -> bool {
            match ty {
                // Treat objects and unions as sizable as they can only be checked at runtime
                Type::Primitive(..)
                | Type::Object
                | Type::OptionalObject
                | Type::Union
                | Type::None => true,
                Type::Compound(ty, _) => compound_type_is_serializable(ty),
                Type::Hidden(_) | Type::Call(_) | Type::TypeNameRef(_) => false,
            }
        }

        type_is_serializable(ty)
    }
}

/// Represents a constraint that ensures the type is a primitive type.
#[derive(Debug, Copy, Clone)]
pub struct PrimitiveTypeConstraint;

impl Constraint for PrimitiveTypeConstraint {
    fn description(&self) -> &'static str {
        "any primitive type"
    }

    fn satisfied(&self, ty: &Type) -> bool {
        match ty {
            Type::Primitive(..) => true,
            // Treat unions as primitive as they can only be checked at runtime
            Type::Union | Type::None => true,
            Type::Compound(..)
            | Type::Object
            | Type::OptionalObject
            | Type::Hidden(_)
            | Type::Call(_)
            | Type::TypeNameRef(_) => false,
        }
    }
}

/// Represents a constraint that ensures the type is any enumeration choice.
#[derive(Debug, Copy, Clone)]
pub struct EnumChoiceConstraint;

impl Constraint for EnumChoiceConstraint {
    fn description(&self) -> &'static str {
        "any enum choice"
    }

    fn satisfied(&self, ty: &Type) -> bool {
        matches!(
            ty,
            Type::Compound(CompoundType::Custom(CustomType::Enum(_)), _)
        )
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::types::ArrayType;
    use crate::types::MapType;
    use crate::types::Optional;
    use crate::types::PairType;
    use crate::types::PrimitiveType;
    use crate::types::StructType;

    #[test]
    fn test_sizable_constraint() {
        let constraint = SizeableConstraint;
        assert!(!constraint.satisfied(&Type::from(PrimitiveType::Boolean).optional()));
        assert!(!constraint.satisfied(&Type::from(PrimitiveType::Integer).optional()));
        assert!(!constraint.satisfied(&Type::from(PrimitiveType::Float).optional()));
        assert!(!constraint.satisfied(&Type::from(PrimitiveType::String).optional()));
        assert!(constraint.satisfied(&Type::from(PrimitiveType::File).optional()));
        assert!(constraint.satisfied(&Type::from(PrimitiveType::Directory).optional()));
        assert!(!constraint.satisfied(&PrimitiveType::Boolean.into()));
        assert!(!constraint.satisfied(&PrimitiveType::Integer.into()));
        assert!(!constraint.satisfied(&PrimitiveType::Float.into()));
        assert!(!constraint.satisfied(&PrimitiveType::String.into()));
        assert!(constraint.satisfied(&PrimitiveType::File.into()));
        assert!(constraint.satisfied(&PrimitiveType::Directory.into()));
        assert!(constraint.satisfied(&Type::OptionalObject));
        assert!(constraint.satisfied(&Type::Object));
        assert!(constraint.satisfied(&Type::Union));
        assert!(!constraint.satisfied(&ArrayType::new(PrimitiveType::String).into()));
        assert!(constraint.satisfied(&ArrayType::new(PrimitiveType::File).into()));
        assert!(
            !constraint
                .satisfied(&PairType::new(PrimitiveType::String, PrimitiveType::String).into())
        );
        assert!(
            constraint.satisfied(&PairType::new(PrimitiveType::String, PrimitiveType::File).into())
        );
        assert!(
            constraint.satisfied(
                &Type::from(PairType::new(
                    PrimitiveType::Directory,
                    PrimitiveType::String
                ))
                .optional()
            )
        );
        assert!(
            !constraint.satisfied(
                &Type::from(MapType::new(
                    PrimitiveType::String,
                    ArrayType::new(PrimitiveType::String)
                ))
                .optional()
            )
        );
        assert!(
            constraint.satisfied(
                &MapType::new(
                    PrimitiveType::String,
                    Type::from(ArrayType::new(PrimitiveType::File)).optional()
                )
                .into()
            )
        );
        assert!(
            constraint.satisfied(
                &Type::from(MapType::new(
                    PrimitiveType::Directory,
                    PrimitiveType::String
                ))
                .optional()
            )
        );
        assert!(
            !constraint.satisfied(&StructType::new("Foo", [("foo", PrimitiveType::String)]).into())
        );
        assert!(constraint.satisfied(
            &Type::from(StructType::new("Foo", [("foo", PrimitiveType::File)])).optional()
        ));
        assert!(
            constraint
                .satisfied(&StructType::new("Foo", [("foo", PrimitiveType::Directory,)]).into())
        );
    }

    #[test]
    fn test_struct_constraint() {
        let constraint = StructConstraint;
        assert!(!constraint.satisfied(&Type::from(PrimitiveType::Boolean).optional()));
        assert!(!constraint.satisfied(&Type::from(PrimitiveType::Integer).optional()));
        assert!(!constraint.satisfied(&Type::from(PrimitiveType::Float).optional()));
        assert!(!constraint.satisfied(&Type::from(PrimitiveType::String).optional()));
        assert!(!constraint.satisfied(&Type::from(PrimitiveType::File).optional()));
        assert!(!constraint.satisfied(&Type::from(PrimitiveType::Directory).optional()));
        assert!(!constraint.satisfied(&PrimitiveType::Boolean.into()));
        assert!(!constraint.satisfied(&PrimitiveType::Integer.into()));
        assert!(!constraint.satisfied(&PrimitiveType::Float.into()));
        assert!(!constraint.satisfied(&PrimitiveType::String.into()));
        assert!(!constraint.satisfied(&PrimitiveType::File.into()));
        assert!(!constraint.satisfied(&PrimitiveType::Directory.into()));
        assert!(!constraint.satisfied(&Type::OptionalObject));
        assert!(!constraint.satisfied(&Type::Object));
        assert!(!constraint.satisfied(&Type::Union));
        assert!(!constraint.satisfied(&ArrayType::non_empty(PrimitiveType::String).into()));
        assert!(!constraint.satisfied(
            &Type::from(PairType::new(PrimitiveType::String, PrimitiveType::String)).optional()
        ));
        assert!(
            !constraint
                .satisfied(&MapType::new(PrimitiveType::String, PrimitiveType::String,).into())
        );
        assert!(constraint.satisfied(
            &Type::from(StructType::new("Foo", [("foo", PrimitiveType::String)])).optional()
        ));
    }

    #[test]
    fn test_json_constraint() {
        let constraint = JsonSerializableConstraint;
        assert!(constraint.satisfied(&Type::from(PrimitiveType::Boolean).optional()));
        assert!(constraint.satisfied(&Type::from(PrimitiveType::Integer).optional()));
        assert!(constraint.satisfied(&Type::from(PrimitiveType::Float).optional()));
        assert!(constraint.satisfied(&Type::from(PrimitiveType::String).optional()));
        assert!(constraint.satisfied(&Type::from(PrimitiveType::File).optional()));
        assert!(constraint.satisfied(&Type::from(PrimitiveType::Directory).optional()));
        assert!(constraint.satisfied(&PrimitiveType::Boolean.into()));
        assert!(constraint.satisfied(&PrimitiveType::Integer.into()));
        assert!(constraint.satisfied(&PrimitiveType::Float.into()));
        assert!(constraint.satisfied(&PrimitiveType::String.into()));
        assert!(constraint.satisfied(&PrimitiveType::File.into()));
        assert!(constraint.satisfied(&PrimitiveType::Directory.into()));
        assert!(constraint.satisfied(&Type::OptionalObject));
        assert!(constraint.satisfied(&Type::Object));
        assert!(constraint.satisfied(&Type::Union));
        assert!(
            constraint.satisfied(&Type::from(ArrayType::new(PrimitiveType::String)).optional())
        );
        assert!(
            !constraint
                .satisfied(&PairType::new(PrimitiveType::String, PrimitiveType::String,).into())
        );
        assert!(constraint.satisfied(
            &Type::from(MapType::new(PrimitiveType::String, PrimitiveType::String)).optional()
        ));
        assert!(
            !constraint
                .satisfied(&MapType::new(PrimitiveType::Integer, PrimitiveType::String,).into())
        );
        assert!(constraint.satisfied(
            &Type::from(StructType::new("Foo", [("foo", PrimitiveType::String)])).optional()
        ));
    }

    #[test]
    fn test_primitive_constraint() {
        let constraint = PrimitiveTypeConstraint;
        assert!(constraint.satisfied(&Type::from(PrimitiveType::Boolean).optional()));
        assert!(constraint.satisfied(&Type::from(PrimitiveType::Integer).optional()));
        assert!(constraint.satisfied(&Type::from(PrimitiveType::Float).optional()));
        assert!(constraint.satisfied(&Type::from(PrimitiveType::String).optional()));
        assert!(constraint.satisfied(&Type::from(PrimitiveType::File).optional()));
        assert!(constraint.satisfied(&Type::from(PrimitiveType::Directory).optional()));
        assert!(constraint.satisfied(&PrimitiveType::Boolean.into()));
        assert!(constraint.satisfied(&PrimitiveType::Integer.into()));
        assert!(constraint.satisfied(&PrimitiveType::Float.into()));
        assert!(constraint.satisfied(&PrimitiveType::String.into()));
        assert!(constraint.satisfied(&PrimitiveType::File.into()));
        assert!(constraint.satisfied(&PrimitiveType::Directory.into()));
        assert!(!constraint.satisfied(&Type::OptionalObject));
        assert!(!constraint.satisfied(&Type::Object));
        assert!(constraint.satisfied(&Type::Union));
        assert!(constraint.satisfied(&Type::None));
        assert!(!constraint.satisfied(&ArrayType::non_empty(PrimitiveType::String).into()));
        assert!(!constraint.satisfied(
            &Type::from(PairType::new(PrimitiveType::String, PrimitiveType::String)).optional()
        ));
        assert!(
            !constraint
                .satisfied(&MapType::new(PrimitiveType::String, PrimitiveType::String,).into())
        );
        assert!(!constraint.satisfied(
            &Type::from(StructType::new("Foo", [("foo", PrimitiveType::String)])).optional()
        ));
    }
}
