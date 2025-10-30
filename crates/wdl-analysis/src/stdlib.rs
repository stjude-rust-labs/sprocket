//! Representation of WDL standard library functions.

use std::cell::Cell;
use std::fmt;
use std::fmt::Write;
use std::sync::LazyLock;

use indexmap::IndexMap;
use indexmap::IndexSet;
use wdl_ast::SupportedVersion;
use wdl_ast::version::V1;

use crate::types::ArrayType;
use crate::types::Coercible;
use crate::types::CompoundType;
use crate::types::MapType;
use crate::types::Optional;
use crate::types::PairType;
use crate::types::PrimitiveType;
use crate::types::Type;

mod constraints;

pub use constraints::*;

/// The maximum number of allowable type parameters in a function signature.
///
/// This is intentionally set low to limit the amount of space needed to store
/// associated data.
///
/// Accessing `STDLIB` will panic if a signature is defined that exceeds this
/// number.
pub const MAX_TYPE_PARAMETERS: usize = 4;

#[allow(clippy::missing_docs_in_private_items)]
const _: () = assert!(
    MAX_TYPE_PARAMETERS < usize::BITS as usize,
    "the maximum number of type parameters cannot exceed the number of bits in usize"
);

/// The maximum (inclusive) number of parameters to any standard library
/// function.
///
/// A function cannot be defined with more than this number of parameters and
/// accessing `STDLIB` will panic if a signature is defined that exceeds this
/// number.
///
/// As new standard library functions are implemented, the maximum will be
/// increased.
pub const MAX_PARAMETERS: usize = 4;

/// A helper function for writing uninferred type parameter constraints to a
/// given writer.
fn write_uninferred_constraints(
    s: &mut impl fmt::Write,
    params: &TypeParameters<'_>,
) -> Result<(), fmt::Error> {
    for (i, (name, constraint)) in params
        .referenced()
        .filter_map(|(p, ty)| {
            // Only consider uninferred type parameters that are constrained
            if ty.is_some() {
                return None;
            }

            Some((p.name, p.constraint()?))
        })
        .enumerate()
    {
        if i == 0 {
            s.write_str(" where ")?;
        } else if i > 1 {
            s.write_str(", ")?;
        }

        write!(s, "`{name}`: {desc}", desc = constraint.description())?;
    }

    Ok(())
}

/// An error that may occur when binding arguments to a standard library
/// function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionBindError {
    /// The function isn't supported for the specified version of WDL.
    RequiresVersion(SupportedVersion),
    /// There are too few arguments to bind the call.
    ///
    /// The value is the minimum number of arguments required.
    TooFewArguments(usize),
    /// There are too many arguments to bind the call.
    ///
    /// The value is the maximum number of arguments allowed.
    TooManyArguments(usize),
    /// An argument type was mismatched.
    ArgumentTypeMismatch {
        /// The index of the mismatched argument.
        index: usize,
        /// The expected type for the argument.
        expected: String,
    },
    /// The function call arguments were ambiguous.
    Ambiguous {
        /// The first conflicting function signature.
        first: String,
        /// The second conflicting function signature.
        second: String,
    },
}

/// Represents a generic type to a standard library function.
#[derive(Debug, Clone)]
pub enum GenericType {
    /// The type is a type parameter (e.g. `X`).
    Parameter(&'static str),
    /// The type is a type parameter, but unqualified; for example, if the type
    /// parameter was bound to type `X?`, then the unqualified type would be
    /// `X`.
    UnqualifiedParameter(&'static str),
    /// The type is a generic `Array`.
    Array(GenericArrayType),
    /// The type is a generic `Pair`.
    Pair(GenericPairType),
    /// The type is a generic `Map`.
    Map(GenericMapType),
    /// The type is the value type extracted from an enum variant.
    EnumValue(GenericEnumValueType),
}

impl GenericType {
    /// Returns an object that implements `Display` for formatting the type.
    pub fn display<'a>(&'a self, params: &'a TypeParameters<'a>) -> impl fmt::Display + 'a {
        #[allow(clippy::missing_docs_in_private_items)]
        struct Display<'a> {
            params: &'a TypeParameters<'a>,
            ty: &'a GenericType,
        }

        impl fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self.ty {
                    GenericType::Parameter(name) | GenericType::UnqualifiedParameter(name) => {
                        let (_, ty) = self.params.get(name).expect("the name should be present");
                        match ty {
                            Some(ty) => {
                                if let GenericType::UnqualifiedParameter(_) = self.ty {
                                    ty.require().fmt(f)
                                } else {
                                    ty.fmt(f)
                                }
                            }
                            None => {
                                write!(f, "{name}")
                            }
                        }
                    }
                    GenericType::Array(ty) => ty.display(self.params).fmt(f),
                    GenericType::Pair(ty) => ty.display(self.params).fmt(f),
                    GenericType::Map(ty) => ty.display(self.params).fmt(f),
                    GenericType::EnumValue(ty) => ty.display(self.params).fmt(f),
                }
            }
        }

        Display { params, ty: self }
    }

    /// Infers any type parameters from the generic type.
    fn infer_type_parameters(
        &self,
        ty: &Type,
        params: &mut TypeParameters<'_>,
        ignore_constraints: bool,
    ) {
        match self {
            Self::Parameter(name) | Self::UnqualifiedParameter(name) => {
                // Verify the type satisfies any constraint
                let (param, _) = params.get(name).expect("should have parameter");

                if !ignore_constraints
                    && let Some(constraint) = param.constraint()
                    && !constraint.satisfied(ty)
                {
                    return;
                }

                params.set_inferred_type(name, ty.clone());
            }
            Self::Array(array) => array.infer_type_parameters(ty, params, ignore_constraints),
            Self::Pair(pair) => pair.infer_type_parameters(ty, params, ignore_constraints),
            Self::Map(map) => map.infer_type_parameters(ty, params, ignore_constraints),
            Self::EnumValue(_) => {
                // NOTE: this is an intentional no-op—the value type is derived
                // from the variant parameter, not inferred from arguments.
            }
        }
    }

    /// Realizes the generic type.
    fn realize(&self, params: &TypeParameters<'_>) -> Option<Type> {
        match self {
            Self::Parameter(name) => {
                params
                    .get(name)
                    .expect("type parameter should be present")
                    .1
            }
            Self::UnqualifiedParameter(name) => params
                .get(name)
                .expect("type parameter should be present")
                .1
                .map(|ty| ty.require()),
            Self::Array(ty) => ty.realize(params),
            Self::Pair(ty) => ty.realize(params),
            Self::Map(ty) => ty.realize(params),
            Self::EnumValue(ty) => ty.realize(params),
        }
    }

    /// Asserts that the type parameters referenced by the type are valid.
    ///
    /// # Panics
    ///
    /// Panics if referenced type parameter is invalid.
    fn assert_type_parameters(&self, parameters: &[TypeParameter]) {
        match self {
            Self::Parameter(n) | Self::UnqualifiedParameter(n) => assert!(
                parameters.iter().any(|p| p.name == *n),
                "generic type references unknown type parameter `{n}`"
            ),
            Self::Array(a) => a.assert_type_parameters(parameters),
            Self::Pair(p) => p.assert_type_parameters(parameters),
            Self::Map(m) => m.assert_type_parameters(parameters),
            Self::EnumValue(e) => e.assert_type_parameters(parameters),
        }
    }
}

impl From<GenericArrayType> for GenericType {
    fn from(value: GenericArrayType) -> Self {
        Self::Array(value)
    }
}

impl From<GenericPairType> for GenericType {
    fn from(value: GenericPairType) -> Self {
        Self::Pair(value)
    }
}

impl From<GenericMapType> for GenericType {
    fn from(value: GenericMapType) -> Self {
        Self::Map(value)
    }
}

impl From<GenericEnumValueType> for GenericType {
    fn from(value: GenericEnumValueType) -> Self {
        Self::EnumValue(value)
    }
}

/// Represents a generic `Array` type.
#[derive(Debug, Clone)]
pub struct GenericArrayType {
    /// The array's element type.
    element_type: Box<FunctionalType>,
    /// Whether or not the array is non-empty.
    non_empty: bool,
}

impl GenericArrayType {
    /// Constructs a new generic array type.
    pub fn new(element_type: impl Into<FunctionalType>) -> Self {
        Self {
            element_type: Box::new(element_type.into()),
            non_empty: false,
        }
    }

    /// Constructs a new non-empty generic array type.
    pub fn non_empty(element_type: impl Into<FunctionalType>) -> Self {
        Self {
            element_type: Box::new(element_type.into()),
            non_empty: true,
        }
    }

    /// Gets the array's element type.
    pub fn element_type(&self) -> &FunctionalType {
        &self.element_type
    }

    /// Determines if the array type is non-empty.
    pub fn is_non_empty(&self) -> bool {
        self.non_empty
    }

    /// Returns an object that implements `Display` for formatting the type.
    pub fn display<'a>(&'a self, params: &'a TypeParameters<'a>) -> impl fmt::Display + 'a {
        #[allow(clippy::missing_docs_in_private_items)]
        struct Display<'a> {
            params: &'a TypeParameters<'a>,
            ty: &'a GenericArrayType,
        }

        impl fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "Array[")?;
                self.ty.element_type.display(self.params).fmt(f)?;
                write!(f, "]")?;

                if self.ty.is_non_empty() {
                    write!(f, "+")?;
                }

                Ok(())
            }
        }

        Display { params, ty: self }
    }

    /// Infers any type parameters from the generic type.
    fn infer_type_parameters(
        &self,
        ty: &Type,
        params: &mut TypeParameters<'_>,
        ignore_constraints: bool,
    ) {
        match ty {
            Type::Union => {
                self.element_type
                    .infer_type_parameters(&Type::Union, params, ignore_constraints);
            }
            Type::Compound(CompoundType::Array(ty), false) => {
                self.element_type.infer_type_parameters(
                    ty.element_type(),
                    params,
                    ignore_constraints,
                );
            }
            _ => {}
        }
    }

    /// Realizes the generic type to an `Array`.
    fn realize(&self, params: &TypeParameters<'_>) -> Option<Type> {
        let ty = self.element_type.realize(params)?;
        if self.non_empty {
            Some(ArrayType::non_empty(ty).into())
        } else {
            Some(ArrayType::new(ty).into())
        }
    }

    /// Asserts that the type parameters referenced by the type are valid.
    ///
    /// # Panics
    ///
    /// Panics if referenced type parameter is invalid.
    fn assert_type_parameters(&self, parameters: &[TypeParameter]) {
        self.element_type.assert_type_parameters(parameters);
    }
}

/// Represents a generic `Pair` type.
#[derive(Debug, Clone)]
pub struct GenericPairType {
    /// The type of the left element of the pair.
    left_type: Box<FunctionalType>,
    /// The type of the right element of the pair.
    right_type: Box<FunctionalType>,
}

impl GenericPairType {
    /// Constructs a new generic pair type.
    pub fn new(
        left_type: impl Into<FunctionalType>,
        right_type: impl Into<FunctionalType>,
    ) -> Self {
        Self {
            left_type: Box::new(left_type.into()),
            right_type: Box::new(right_type.into()),
        }
    }

    /// Gets the pairs's left type.
    pub fn left_type(&self) -> &FunctionalType {
        &self.left_type
    }

    /// Gets the pairs's right type.
    pub fn right_type(&self) -> &FunctionalType {
        &self.right_type
    }

    /// Returns an object that implements `Display` for formatting the type.
    pub fn display<'a>(&'a self, params: &'a TypeParameters<'a>) -> impl fmt::Display + 'a {
        #[allow(clippy::missing_docs_in_private_items)]
        struct Display<'a> {
            params: &'a TypeParameters<'a>,
            ty: &'a GenericPairType,
        }

        impl fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "Pair[")?;
                self.ty.left_type.display(self.params).fmt(f)?;
                write!(f, ", ")?;
                self.ty.right_type.display(self.params).fmt(f)?;
                write!(f, "]")
            }
        }

        Display { params, ty: self }
    }

    /// Infers any type parameters from the generic type.
    fn infer_type_parameters(
        &self,
        ty: &Type,
        params: &mut TypeParameters<'_>,
        ignore_constraints: bool,
    ) {
        match ty {
            Type::Union => {
                self.left_type
                    .infer_type_parameters(&Type::Union, params, ignore_constraints);
                self.right_type
                    .infer_type_parameters(&Type::Union, params, ignore_constraints);
            }
            Type::Compound(CompoundType::Pair(ty), false) => {
                self.left_type
                    .infer_type_parameters(ty.left_type(), params, ignore_constraints);
                self.right_type
                    .infer_type_parameters(ty.right_type(), params, ignore_constraints);
            }
            _ => {}
        }
    }

    /// Realizes the generic type to a `Pair`.
    fn realize(&self, params: &TypeParameters<'_>) -> Option<Type> {
        let left_type = self.left_type.realize(params)?;
        let right_type = self.right_type.realize(params)?;
        Some(PairType::new(left_type, right_type).into())
    }

    /// Asserts that the type parameters referenced by the type are valid.
    ///
    /// # Panics
    ///
    /// Panics if referenced type parameter is invalid.
    fn assert_type_parameters(&self, parameters: &[TypeParameter]) {
        self.left_type.assert_type_parameters(parameters);
        self.right_type.assert_type_parameters(parameters);
    }
}

/// Represents a generic `Map` type.
#[derive(Debug, Clone)]
pub struct GenericMapType {
    /// The key type of the map.
    key_type: Box<FunctionalType>,
    /// The value type of the map.
    value_type: Box<FunctionalType>,
}

impl GenericMapType {
    /// Constructs a new generic map type.
    pub fn new(key_type: impl Into<FunctionalType>, value_type: impl Into<FunctionalType>) -> Self {
        Self {
            key_type: Box::new(key_type.into()),
            value_type: Box::new(value_type.into()),
        }
    }

    /// Gets the maps's key type.
    pub fn key_type(&self) -> &FunctionalType {
        &self.key_type
    }

    /// Gets the maps's value type.
    pub fn value_type(&self) -> &FunctionalType {
        &self.value_type
    }

    /// Returns an object that implements `Display` for formatting the type.
    pub fn display<'a>(&'a self, params: &'a TypeParameters<'a>) -> impl fmt::Display + 'a {
        #[allow(clippy::missing_docs_in_private_items)]
        struct Display<'a> {
            params: &'a TypeParameters<'a>,
            ty: &'a GenericMapType,
        }

        impl fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "Map[")?;
                self.ty.key_type.display(self.params).fmt(f)?;
                write!(f, ", ")?;
                self.ty.value_type.display(self.params).fmt(f)?;
                write!(f, "]")
            }
        }

        Display { params, ty: self }
    }

    /// Infers any type parameters from the generic type.
    fn infer_type_parameters(
        &self,
        ty: &Type,
        params: &mut TypeParameters<'_>,
        ignore_constraints: bool,
    ) {
        match ty {
            Type::Union => {
                self.key_type
                    .infer_type_parameters(&Type::Union, params, ignore_constraints);
                self.value_type
                    .infer_type_parameters(&Type::Union, params, ignore_constraints);
            }
            Type::Compound(CompoundType::Map(ty), false) => {
                self.key_type
                    .infer_type_parameters(ty.key_type(), params, ignore_constraints);
                self.value_type
                    .infer_type_parameters(ty.value_type(), params, ignore_constraints);
            }
            _ => {}
        }
    }

    /// Realizes the generic type to a `Map`.
    fn realize(&self, params: &TypeParameters<'_>) -> Option<Type> {
        let key_type = self.key_type.realize(params)?;
        let value_type = self.value_type.realize(params)?;
        Some(MapType::new(key_type, value_type).into())
    }

    /// Asserts that the type parameters referenced by the type are valid.
    ///
    /// # Panics
    ///
    /// Panics if referenced type parameter is invalid.
    fn assert_type_parameters(&self, parameters: &[TypeParameter]) {
        self.key_type.assert_type_parameters(parameters);
        self.value_type.assert_type_parameters(parameters);
    }
}

/// Represents the value type of an enum variant.
#[derive(Debug, Clone)]
pub struct GenericEnumValueType {
    /// The enum variant type parameter name.
    variant_param: &'static str,
}

impl GenericEnumValueType {
    /// Constructs a new generic enum variant type.
    pub fn new(variant_param: &'static str) -> Self {
        Self { variant_param }
    }

    /// Gets the variant parameter name.
    pub fn variant_param(&self) -> &'static str {
        self.variant_param
    }

    /// Returns an object that implements `Display` for formatting the type.
    pub fn display<'a>(&'a self, params: &'a TypeParameters<'a>) -> impl fmt::Display + 'a {
        #[allow(clippy::missing_docs_in_private_items)]
        struct Display<'a> {
            params: &'a TypeParameters<'a>,
            ty: &'a GenericEnumValueType,
        }

        impl fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let (_, variant_ty) = self
                    .params
                    .get(self.ty.variant_param)
                    .expect("variant parameter should be present");

                match variant_ty {
                    Some(Type::Compound(CompoundType::Enum(enum_ty), _)) => {
                        enum_ty.value_type().fmt(f)
                    }
                    // NOTE: non-enums should gracefully fail.
                    _ => write!(f, "T"),
                }
            }
        }

        Display { params, ty: self }
    }

    /// Realizes the generic type to the enum's value type.
    fn realize(&self, params: &TypeParameters<'_>) -> Option<Type> {
        let (_, variant_ty) = params
            .get(self.variant_param)
            .expect("variant parameter should be present");

        match variant_ty {
            Some(Type::Compound(CompoundType::Enum(enum_ty), _)) => {
                Some(enum_ty.value_type().clone())
            }
            // NOTE: non-enums should gracefully fail.
            _ => None,
        }
    }

    /// Asserts that the type parameters referenced by the type are valid.
    fn assert_type_parameters(&self, parameters: &[TypeParameter]) {
        assert!(
            parameters.iter().any(|p| p.name == self.variant_param),
            "generic enum variant type references unknown type parameter `{}`",
            self.variant_param
        );
    }
}

/// Represents a collection of type parameters.
#[derive(Debug, Clone)]
pub struct TypeParameters<'a> {
    /// The collection of type parameters.
    parameters: &'a [TypeParameter],
    /// The inferred types for the type parameters.
    inferred_types: [Option<Type>; MAX_TYPE_PARAMETERS],
    /// A bitset of type parameters that have been referenced since the last
    /// call to `reset`.
    referenced: Cell<usize>,
}

impl<'a> TypeParameters<'a> {
    /// Constructs a new type parameters collection using `None` as the
    /// calculated parameter types.
    ///
    /// # Panics
    ///
    /// Panics if the count of the given type parameters exceeds the maximum
    /// allowed.
    pub fn new(parameters: &'a [TypeParameter]) -> Self {
        assert!(
            parameters.len() <= MAX_TYPE_PARAMETERS,
            "no more than {MAX_TYPE_PARAMETERS} type parameters is supported"
        );

        Self {
            parameters,
            inferred_types: [const { None }; MAX_TYPE_PARAMETERS],
            referenced: Cell::new(0),
        }
    }

    /// Gets a type parameter and its inferred type from the collection.
    ///
    /// Returns `None` if the name is not a type parameter.
    ///
    /// This method also marks the type parameter as referenced.
    pub fn get(&self, name: &str) -> Option<(&TypeParameter, Option<Type>)> {
        let index = self.parameters.iter().position(|p| p.name == name)?;

        // Mark the parameter as referenced
        self.referenced.set(self.referenced.get() | (1 << index));

        Some((&self.parameters[index], self.inferred_types[index].clone()))
    }

    /// Reset any referenced type parameters.
    pub fn reset(&self) {
        self.referenced.set(0);
    }

    /// Gets an iterator of the type parameters that have been referenced since
    /// the last reset.
    pub fn referenced(&self) -> impl Iterator<Item = (&TypeParameter, Option<Type>)> + use<'_> {
        let mut bits = self.referenced.get();
        std::iter::from_fn(move || {
            if bits == 0 {
                return None;
            }

            let index = bits.trailing_zeros() as usize;
            let parameter = &self.parameters[index];
            let ty = self.inferred_types[index].clone();
            bits ^= bits & bits.overflowing_neg().0;
            Some((parameter, ty))
        })
    }

    /// Sets the inferred type of a type parameter.
    ///
    /// Note that a type parameter can only be inferred once; subsequent
    /// attempts to set the inferred type will be ignored.
    ///
    /// # Panics
    ///
    /// Panics if the given name is not a type parameter.
    fn set_inferred_type(&mut self, name: &str, ty: Type) {
        let index = self
            .parameters
            .iter()
            .position(|p| p.name == name)
            .unwrap_or_else(|| panic!("unknown type parameter `{name}`"));

        self.inferred_types[index].get_or_insert(ty);
    }
}

/// Represents a type of a function parameter or return.
#[derive(Debug, Clone)]
pub enum FunctionalType {
    /// The parameter type is a concrete WDL type.
    Concrete(Type),
    /// The parameter type is a generic type.
    Generic(GenericType),
}

impl FunctionalType {
    /// Determines if the type is generic.
    pub fn is_generic(&self) -> bool {
        matches!(self, Self::Generic(_))
    }

    /// Returns the concrete type.
    ///
    /// Returns `None` if the type is not concrete.
    pub fn concrete_type(&self) -> Option<&Type> {
        match self {
            Self::Concrete(ty) => Some(ty),
            Self::Generic(_) => None,
        }
    }

    /// Returns an object that implements `Display` for formatting the type.
    pub fn display<'a>(&'a self, params: &'a TypeParameters<'a>) -> impl fmt::Display + 'a {
        #[allow(clippy::missing_docs_in_private_items)]
        struct Display<'a> {
            params: &'a TypeParameters<'a>,
            ty: &'a FunctionalType,
        }

        impl fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self.ty {
                    FunctionalType::Concrete(ty) => ty.fmt(f),
                    FunctionalType::Generic(ty) => ty.display(self.params).fmt(f),
                }
            }
        }

        Display { params, ty: self }
    }

    /// Infers any type parameters if the type is generic.
    fn infer_type_parameters(
        &self,
        ty: &Type,
        params: &mut TypeParameters<'_>,
        ignore_constraints: bool,
    ) {
        if let Self::Generic(generic) = self {
            generic.infer_type_parameters(ty, params, ignore_constraints);
        }
    }

    /// Realizes the type if the type is generic.
    fn realize(&self, params: &TypeParameters<'_>) -> Option<Type> {
        match self {
            FunctionalType::Concrete(ty) => Some(ty.clone()),
            FunctionalType::Generic(ty) => ty.realize(params),
        }
    }

    /// Asserts that the type parameters referenced by the type are valid.
    ///
    /// # Panics
    ///
    /// Panics if referenced type parameter is invalid.
    fn assert_type_parameters(&self, parameters: &[TypeParameter]) {
        if let FunctionalType::Generic(ty) = self {
            ty.assert_type_parameters(parameters)
        }
    }
}

impl From<Type> for FunctionalType {
    fn from(value: Type) -> Self {
        Self::Concrete(value)
    }
}

impl From<PrimitiveType> for FunctionalType {
    fn from(value: PrimitiveType) -> Self {
        Self::Concrete(value.into())
    }
}

impl From<GenericType> for FunctionalType {
    fn from(value: GenericType) -> Self {
        Self::Generic(value)
    }
}

impl From<GenericArrayType> for FunctionalType {
    fn from(value: GenericArrayType) -> Self {
        Self::Generic(GenericType::Array(value))
    }
}

impl From<GenericPairType> for FunctionalType {
    fn from(value: GenericPairType) -> Self {
        Self::Generic(GenericType::Pair(value))
    }
}

impl From<GenericMapType> for FunctionalType {
    fn from(value: GenericMapType) -> Self {
        Self::Generic(GenericType::Map(value))
    }
}

impl From<GenericEnumValueType> for FunctionalType {
    fn from(value: GenericEnumValueType) -> Self {
        Self::Generic(GenericType::EnumValue(value))
    }
}

/// Represents a type parameter to a function.
#[derive(Debug)]
pub struct TypeParameter {
    /// The name of the type parameter.
    name: &'static str,
    /// The type parameter constraint.
    constraint: Option<Box<dyn Constraint>>,
}

impl TypeParameter {
    /// Creates a new type parameter without a constraint.
    pub fn any(name: &'static str) -> Self {
        Self {
            name,
            constraint: None,
        }
    }

    /// Creates a new type parameter with the given constraint.
    pub fn new(name: &'static str, constraint: impl Constraint + 'static) -> Self {
        Self {
            name,
            constraint: Some(Box::new(constraint)),
        }
    }

    /// Gets the name of the type parameter.
    pub fn name(&self) -> &str {
        self.name
    }

    /// Gets the constraint of the type parameter.
    pub fn constraint(&self) -> Option<&dyn Constraint> {
        self.constraint.as_deref()
    }
}

/// Represents the kind of binding for arguments to a function.
#[derive(Debug, Clone)]
enum BindingKind {
    /// The binding was an equivalence binding, meaning all of the provided
    /// arguments had type equivalence with corresponding concrete parameters.
    ///
    /// The value is the bound return type of the function.
    Equivalence(Type),
    /// The binding was a coercion binding, meaning at least one of the provided
    /// arguments needed to be coerced.
    ///
    /// The value it the bound return type of the function.
    Coercion(Type),
}

impl BindingKind {
    /// Gets the binding's return type.
    pub fn ret(&self) -> &Type {
        match self {
            Self::Equivalence(ty) | Self::Coercion(ty) => ty,
        }
    }
}

/// Represents a parameter to a standard library function.
#[derive(Debug)]
pub struct FunctionParameter {
    /// The name of the parameter.
    name: &'static str,
    /// The type of the parameter.
    ty: FunctionalType,
    /// The description of the parameter.
    description: &'static str,
}

impl FunctionParameter {
    /// Gets the name of the parameter.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Gets the type of the parameter.
    pub fn ty(&self) -> &FunctionalType {
        &self.ty
    }

    /// Gets the description of the parameter.
    #[allow(dead_code)]
    pub fn description(&self) -> &'static str {
        self.description
    }
}

/// Represents a WDL function signature.
#[derive(Debug)]
pub struct FunctionSignature {
    /// The minimum required version for the function signature.
    minimum_version: Option<SupportedVersion>,
    /// The generic type parameters of the function.
    type_parameters: Vec<TypeParameter>,
    /// The number of required parameters of the function.
    required: Option<usize>,
    /// The parameters of the function.
    parameters: Vec<FunctionParameter>,
    /// The return type of the function.
    ret: FunctionalType,
    /// The function definition
    definition: Option<&'static str>,
}

impl FunctionSignature {
    /// Builds a function signature builder.
    pub fn builder() -> FunctionSignatureBuilder {
        FunctionSignatureBuilder::new()
    }

    /// Gets the minimum version required to call this function signature.
    pub fn minimum_version(&self) -> SupportedVersion {
        self.minimum_version
            .unwrap_or(SupportedVersion::V1(V1::Zero))
    }

    /// Gets the function's type parameters.
    pub fn type_parameters(&self) -> &[TypeParameter] {
        &self.type_parameters
    }

    /// Gets the function's parameters.
    pub fn parameters(&self) -> &[FunctionParameter] {
        &self.parameters
    }

    /// Gets the minimum number of required parameters.
    ///
    /// For a function without optional parameters, this will be the same as the
    /// number of parameters for the function.
    pub fn required(&self) -> usize {
        self.required.unwrap_or(self.parameters.len())
    }

    /// Gets the function's return type.
    pub fn ret(&self) -> &FunctionalType {
        &self.ret
    }

    /// Gets the function's definition.
    pub fn definition(&self) -> Option<&'static str> {
        self.definition
    }

    /// Determines if the function signature is generic.
    pub fn is_generic(&self) -> bool {
        self.generic_parameter_count() > 0 || self.ret.is_generic()
    }

    /// Gets the count of generic parameters for the function.
    pub fn generic_parameter_count(&self) -> usize {
        self.parameters.iter().filter(|p| p.ty.is_generic()).count()
    }

    /// Returns an object that implements `Display` for formatting the signature
    /// with the given function name.
    pub fn display<'a>(&'a self, params: &'a TypeParameters<'a>) -> impl fmt::Display + 'a {
        #[allow(clippy::missing_docs_in_private_items)]
        struct Display<'a> {
            params: &'a TypeParameters<'a>,
            sig: &'a FunctionSignature,
        }

        impl fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_char('(')?;

                self.params.reset();
                let required = self.sig.required();
                for (i, parameter) in self.sig.parameters.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }

                    if i >= required {
                        f.write_char('<')?;
                    }

                    write!(
                        f,
                        "{name}: {ty}",
                        name = parameter.name(),
                        ty = parameter.ty().display(self.params)
                    )?;

                    if i >= required {
                        f.write_char('>')?;
                    }
                }

                write!(f, ") -> {ret}", ret = self.sig.ret.display(self.params))?;
                write_uninferred_constraints(f, self.params)?;

                Ok(())
            }
        }

        Display { params, sig: self }
    }

    /// Infers the concrete types of any type parameters for the function
    /// signature.
    ///
    /// Returns the collection of type parameters.
    fn infer_type_parameters(
        &self,
        arguments: &[Type],
        ignore_constraints: bool,
    ) -> TypeParameters<'_> {
        let mut parameters = TypeParameters::new(&self.type_parameters);
        for (parameter, argument) in self.parameters.iter().zip(arguments.iter()) {
            parameter
                .ty
                .infer_type_parameters(argument, &mut parameters, ignore_constraints);
        }

        parameters
    }

    /// Determines if the there is an insufficient number of arguments to bind
    /// to this signature.
    fn insufficient_arguments(&self, arguments: &[Type]) -> bool {
        arguments.len() < self.required() || arguments.len() > self.parameters.len()
    }

    /// Binds the function signature to the given arguments.
    ///
    /// This function will infer the type parameters for the arguments and
    /// ensure that the argument types are equivalent to the parameter types.
    ///
    /// If an argument is not type equivalent, an attempt is made to coerce the
    /// type.
    ///
    /// Returns the realized type of the function's return type.
    fn bind(
        &self,
        version: SupportedVersion,
        arguments: &[Type],
    ) -> Result<BindingKind, FunctionBindError> {
        if version < self.minimum_version() {
            return Err(FunctionBindError::RequiresVersion(self.minimum_version()));
        }

        let required = self.required();
        if arguments.len() < required {
            return Err(FunctionBindError::TooFewArguments(required));
        }

        if arguments.len() > self.parameters.len() {
            return Err(FunctionBindError::TooManyArguments(self.parameters.len()));
        }

        // Ensure the argument types are correct for the function
        let mut coerced = false;
        let type_parameters = self.infer_type_parameters(arguments, false);
        for (i, (parameter, argument)) in self.parameters.iter().zip(arguments.iter()).enumerate() {
            match parameter.ty.realize(&type_parameters) {
                Some(ty) => {
                    // If a coercion hasn't occurred yet, check for type equivalence
                    // For the purpose of this check, also accept equivalence of `T` if the
                    // parameter type is `T?`; otherwise, fall back to coercion
                    if !coerced && argument != &ty && argument != &ty.require() {
                        coerced = true;
                    }

                    if coerced && !argument.is_coercible_to(&ty) {
                        return Err(FunctionBindError::ArgumentTypeMismatch {
                            index: i,
                            expected: format!("`{ty}`"),
                        });
                    }
                }
                None if argument.is_union() => {
                    // If the type is `Union`, accept it as indeterminate
                    continue;
                }
                None => {
                    // Otherwise, this is a type mismatch
                    type_parameters.reset();

                    let mut expected = String::new();

                    write!(
                        &mut expected,
                        "`{param}`",
                        param = parameter.ty.display(&type_parameters)
                    )
                    .unwrap();

                    write_uninferred_constraints(&mut expected, &type_parameters).unwrap();
                    return Err(FunctionBindError::ArgumentTypeMismatch { index: i, expected });
                }
            }
        }

        // Finally, realize the return type; if it fails to realize, it means there was
        // at least one uninferred type parameter; we return `Union` instead to indicate
        // that the return value is indeterminate.
        let ret = self.ret().realize(&type_parameters).unwrap_or(Type::Union);

        if coerced {
            Ok(BindingKind::Coercion(ret))
        } else {
            Ok(BindingKind::Equivalence(ret))
        }
    }
}

impl Default for FunctionSignature {
    fn default() -> Self {
        Self {
            minimum_version: None,
            type_parameters: Default::default(),
            required: Default::default(),
            parameters: Default::default(),
            ret: FunctionalType::Concrete(Type::Union),
            definition: None,
        }
    }
}

/// Represents a function signature builder.
#[derive(Debug, Default)]
pub struct FunctionSignatureBuilder(FunctionSignature);

impl FunctionSignatureBuilder {
    /// Constructs a new function signature builder.
    pub fn new() -> Self {
        Self(Default::default())
    }

    /// Sets the minimum required version for the function signature.
    pub fn min_version(mut self, version: SupportedVersion) -> Self {
        self.0.minimum_version = Some(version);
        self
    }

    /// Adds a constrained type parameter to the function signature.
    pub fn type_parameter(
        mut self,
        name: &'static str,
        constraint: impl Constraint + 'static,
    ) -> Self {
        self.0
            .type_parameters
            .push(TypeParameter::new(name, constraint));
        self
    }

    /// Adds an unconstrained type parameter to the function signature.
    pub fn any_type_parameter(mut self, name: &'static str) -> Self {
        self.0.type_parameters.push(TypeParameter::any(name));
        self
    }

    /// Adds a parameter to the function signature.
    pub fn parameter(
        mut self,
        name: &'static str,
        ty: impl Into<FunctionalType>,
        description: &'static str,
    ) -> Self {
        self.0.parameters.push(FunctionParameter {
            name,
            ty: ty.into(),
            description,
        });
        self
    }

    /// Sets the return value in the function signature.
    ///
    /// If this is not called, the function signature will return a `Union`
    /// type.
    pub fn ret(mut self, ret: impl Into<FunctionalType>) -> Self {
        self.0.ret = ret.into();
        self
    }

    /// Sets the number of required parameters in the function signature.
    pub fn required(mut self, required: usize) -> Self {
        self.0.required = Some(required);
        self
    }

    /// Sets the definition of the function.
    pub fn definition(mut self, definition: &'static str) -> Self {
        self.0.definition = Some(definition);
        self
    }

    /// Consumes the builder and produces the function signature.
    ///
    /// # Panics
    ///
    /// This method panics if the function signature is invalid.
    pub fn build(self) -> FunctionSignature {
        let sig = self.0;

        // Ensure the number of required parameters doesn't exceed the number of
        // parameters
        if let Some(required) = sig.required
            && required > sig.parameters.len()
        {
            panic!("number of required parameters exceeds the number of parameters");
        }

        assert!(
            sig.type_parameters.len() <= MAX_TYPE_PARAMETERS,
            "too many type parameters"
        );

        assert!(
            sig.parameters.len() <= MAX_PARAMETERS,
            "too many parameters"
        );

        // Ensure any generic type parameters indexes are in range for the parameters
        for parameter in sig.parameters.iter() {
            parameter.ty.assert_type_parameters(&sig.type_parameters)
        }

        sig.ret().assert_type_parameters(&sig.type_parameters);

        assert!(sig.definition.is_some(), "functions should have definition");

        sig
    }
}

/// Represents information relating to how a function binds to its arguments.
#[derive(Debug, Clone)]
pub struct Binding<'a> {
    /// The calculated return type from the function given the argument types.
    return_type: Type,
    /// The function overload index.
    ///
    /// For monomorphic functions, this will always be zero.
    index: usize,
    /// The signature that was bound.
    signature: &'a FunctionSignature,
}

impl Binding<'_> {
    /// Gets the calculated return type of the bound function.
    pub fn return_type(&self) -> &Type {
        &self.return_type
    }

    /// Gets the overload index.
    ///
    /// For monomorphic functions, this will always be zero.
    pub fn index(&self) -> usize {
        self.index
    }

    /// Gets the signature that was bound.
    pub fn signature(&self) -> &FunctionSignature {
        self.signature
    }
}

/// Represents a WDL function.
#[derive(Debug)]
pub enum Function {
    /// The function is monomorphic.
    Monomorphic(MonomorphicFunction),
    /// The function is polymorphic.
    Polymorphic(PolymorphicFunction),
}

impl Function {
    /// Gets the minimum WDL version required to call this function.
    pub fn minimum_version(&self) -> SupportedVersion {
        match self {
            Self::Monomorphic(f) => f.minimum_version(),
            Self::Polymorphic(f) => f.minimum_version(),
        }
    }

    /// Gets the minimum and maximum number of parameters the function has for
    /// the given WDL version.
    ///
    /// Returns `None` if the function is not supported for the given version.
    pub fn param_min_max(&self, version: SupportedVersion) -> Option<(usize, usize)> {
        match self {
            Self::Monomorphic(f) => f.param_min_max(version),
            Self::Polymorphic(f) => f.param_min_max(version),
        }
    }

    /// Binds the function to the given arguments.
    pub fn bind<'a>(
        &'a self,
        version: SupportedVersion,
        arguments: &[Type],
    ) -> Result<Binding<'a>, FunctionBindError> {
        match self {
            Self::Monomorphic(f) => f.bind(version, arguments),
            Self::Polymorphic(f) => f.bind(version, arguments),
        }
    }

    /// Realizes the return type of the function without constraints.
    ///
    /// This is typically called after a failure to bind a function so that the
    /// return type can be calculated despite the failure.
    ///
    /// As such, it attempts to realize any type parameters without constraints,
    /// as an unsatisfied constraint likely caused the bind failure.
    pub fn realize_unconstrained_return_type(&self, arguments: &[Type]) -> Type {
        match self {
            Self::Monomorphic(f) => {
                let type_parameters = f.signature.infer_type_parameters(arguments, true);
                f.signature
                    .ret()
                    .realize(&type_parameters)
                    .unwrap_or(Type::Union)
            }
            Self::Polymorphic(f) => {
                let mut ty = None;

                // For polymorphic functions, the calculated return type must be the same for
                // each overload
                for signature in &f.signatures {
                    let type_parameters = signature.infer_type_parameters(arguments, true);
                    let ret_ty = signature
                        .ret()
                        .realize(&type_parameters)
                        .unwrap_or(Type::Union);

                    if ty.get_or_insert(ret_ty.clone()) != &ret_ty {
                        return Type::Union;
                    }
                }

                ty.unwrap_or(Type::Union)
            }
        }
    }
}

/// Represents a monomorphic function.
///
/// In this context, a monomorphic function has only a single type (i.e.
/// signature).
#[derive(Debug)]
pub struct MonomorphicFunction {
    /// The signature of the function.
    signature: FunctionSignature,
}

impl MonomorphicFunction {
    /// Constructs a new monomorphic function.
    pub fn new(signature: FunctionSignature) -> Self {
        Self { signature }
    }

    /// Gets the minimum WDL version required to call this function.
    pub fn minimum_version(&self) -> SupportedVersion {
        self.signature.minimum_version()
    }

    /// Gets the minimum and maximum number of parameters the function has for
    /// the given WDL version.
    ///
    /// Returns `None` if the function is not supported for the given version.
    pub fn param_min_max(&self, version: SupportedVersion) -> Option<(usize, usize)> {
        if version < self.signature.minimum_version() {
            return None;
        }

        Some((self.signature.required(), self.signature.parameters.len()))
    }

    /// Gets the signature of the function.
    pub fn signature(&self) -> &FunctionSignature {
        &self.signature
    }

    /// Binds the function to the given arguments.
    pub fn bind<'a>(
        &'a self,
        version: SupportedVersion,
        arguments: &[Type],
    ) -> Result<Binding<'a>, FunctionBindError> {
        let return_type = self.signature.bind(version, arguments)?.ret().clone();
        Ok(Binding {
            return_type,
            index: 0,
            signature: &self.signature,
        })
    }
}

impl From<MonomorphicFunction> for Function {
    fn from(value: MonomorphicFunction) -> Self {
        Self::Monomorphic(value)
    }
}

/// Represents a polymorphic function.
///
/// In this context, a polymorphic function has more than one type (i.e.
/// signature); overload resolution is used to determine which signature binds
/// to the function call.
#[derive(Debug)]
pub struct PolymorphicFunction {
    /// The signatures of the function.
    signatures: Vec<FunctionSignature>,
}

impl PolymorphicFunction {
    /// Constructs a new polymorphic function.
    ///
    /// # Panics
    ///
    /// Panics if the number of signatures is less than two.
    pub fn new(signatures: Vec<FunctionSignature>) -> Self {
        assert!(
            signatures.len() > 1,
            "a polymorphic function must have at least two signatures"
        );

        Self { signatures }
    }

    /// Gets the minimum WDL version required to call this function.
    pub fn minimum_version(&self) -> SupportedVersion {
        self.signatures
            .iter()
            .fold(None, |v: Option<SupportedVersion>, s| {
                Some(
                    v.map(|v| v.min(s.minimum_version()))
                        .unwrap_or_else(|| s.minimum_version()),
                )
            })
            .expect("there should be at least one signature")
    }

    /// Gets the minimum and maximum number of parameters the function has for
    /// the given WDL version.
    ///
    /// Returns `None` if the function is not supported for the given version.
    pub fn param_min_max(&self, version: SupportedVersion) -> Option<(usize, usize)> {
        let mut min = usize::MAX;
        let mut max = 0;
        for sig in self
            .signatures
            .iter()
            .filter(|s| s.minimum_version() <= version)
        {
            min = std::cmp::min(min, sig.required());
            max = std::cmp::max(max, sig.parameters().len());
        }

        if min == usize::MAX {
            return None;
        }

        Some((min, max))
    }

    /// Gets the signatures of the function.
    pub fn signatures(&self) -> &[FunctionSignature] {
        &self.signatures
    }

    /// Binds the function to the given arguments.
    ///
    /// This performs overload resolution for the polymorphic function.
    pub fn bind<'a>(
        &'a self,
        version: SupportedVersion,
        arguments: &[Type],
    ) -> Result<Binding<'a>, FunctionBindError> {
        // Ensure that there is at least one signature with a matching minimum version.
        let min_version = self.minimum_version();
        if version < min_version {
            return Err(FunctionBindError::RequiresVersion(min_version));
        }

        // Next check the min/max parameter counts
        let (min, max) = self
            .param_min_max(version)
            .expect("should have at least one signature for the version");
        if arguments.len() < min {
            return Err(FunctionBindError::TooFewArguments(min));
        }

        if arguments.len() > max {
            return Err(FunctionBindError::TooManyArguments(max));
        }

        // Overload resolution precedence is from most specific to least specific:
        // * Non-generic exact match
        // * Non-generic with coercion
        // * Generic exact match
        // * Generic with coercion

        let mut max_mismatch_index = 0;
        let mut expected_types = IndexSet::new();

        for generic in [false, true] {
            let mut exact: Option<(usize, Type)> = None;
            let mut coercion1: Option<(usize, Type)> = None;
            let mut coercion2 = None;
            for (index, signature) in self.signatures.iter().enumerate().filter(|(_, s)| {
                s.is_generic() == generic
                    && s.minimum_version() <= version
                    && !s.insufficient_arguments(arguments)
            }) {
                match signature.bind(version, arguments) {
                    Ok(BindingKind::Equivalence(ty)) => {
                        // We cannot have more than one exact match
                        if let Some((previous, _)) = exact {
                            return Err(FunctionBindError::Ambiguous {
                                first: self.signatures[previous]
                                    .display(&TypeParameters::new(
                                        &self.signatures[previous].type_parameters,
                                    ))
                                    .to_string(),
                                second: self.signatures[index]
                                    .display(&TypeParameters::new(
                                        &self.signatures[index].type_parameters,
                                    ))
                                    .to_string(),
                            });
                        }

                        exact = Some((index, ty));
                    }
                    Ok(BindingKind::Coercion(ty)) => {
                        // If this is the first coercion, store it; otherwise, store the second
                        // coercion index; if there's more than one coercion, we'll report an error
                        // below after ensuring there's no exact match
                        if coercion1.is_none() {
                            coercion1 = Some((index, ty));
                        } else {
                            coercion2.get_or_insert(index);
                        }
                    }
                    Err(FunctionBindError::ArgumentTypeMismatch { index, expected }) => {
                        // We'll report an argument mismatch for the greatest argument index
                        if index > max_mismatch_index {
                            max_mismatch_index = index;
                            expected_types.clear();
                        }

                        if index == max_mismatch_index {
                            expected_types.insert(expected);
                        }
                    }
                    Err(
                        FunctionBindError::RequiresVersion(_)
                        | FunctionBindError::Ambiguous { .. }
                        | FunctionBindError::TooFewArguments(_)
                        | FunctionBindError::TooManyArguments(_),
                    ) => unreachable!("should not encounter these errors due to above filter"),
                }
            }

            if let Some((index, ty)) = exact {
                return Ok(Binding {
                    return_type: ty,
                    index,
                    signature: &self.signatures[index],
                });
            }

            // Ensure there wasn't more than one coercion
            if let Some(previous) = coercion2 {
                let index = coercion1.unwrap().0;
                return Err(FunctionBindError::Ambiguous {
                    first: self.signatures[previous]
                        .display(&TypeParameters::new(
                            &self.signatures[previous].type_parameters,
                        ))
                        .to_string(),
                    second: self.signatures[index]
                        .display(&TypeParameters::new(
                            &self.signatures[index].type_parameters,
                        ))
                        .to_string(),
                });
            }

            if let Some((index, ty)) = coercion1 {
                return Ok(Binding {
                    return_type: ty,
                    index,
                    signature: &self.signatures[index],
                });
            }
        }

        assert!(!expected_types.is_empty());

        let mut expected = String::new();
        for (i, ty) in expected_types.iter().enumerate() {
            if i > 0 {
                if expected_types.len() == 2 {
                    expected.push_str(" or ");
                } else if i == expected_types.len() - 1 {
                    expected.push_str(", or ");
                } else {
                    expected.push_str(", ");
                }
            }

            expected.push_str(ty);
        }

        Err(FunctionBindError::ArgumentTypeMismatch {
            index: max_mismatch_index,
            expected,
        })
    }
}

impl From<PolymorphicFunction> for Function {
    fn from(value: PolymorphicFunction) -> Self {
        Self::Polymorphic(value)
    }
}

/// A representation of the standard library.
#[derive(Debug)]
pub struct StandardLibrary {
    /// A map of function name to function definition.
    functions: IndexMap<&'static str, Function>,
    /// The type for `Array[Int]`.
    array_int: Type,
    /// The type for `Array[String]`.
    array_string: Type,
    /// The type for `Array[File]`.
    array_file: Type,
    /// The type for `Array[Object]`.
    array_object: Type,
    /// The type for `Array[String]+`.
    array_string_non_empty: Type,
    /// The type for `Array[Array[String]]`.
    array_array_string: Type,
    /// The type for `Map[String, String]`.
    map_string_string: Type,
    /// The type for `Map[String, Int]`.
    map_string_int: Type,
}

impl StandardLibrary {
    /// Gets a standard library function by name.
    pub fn function(&self, name: &str) -> Option<&Function> {
        self.functions.get(name)
    }

    /// Gets an iterator over all the functions in the standard library.
    pub fn functions(&self) -> impl ExactSizeIterator<Item = (&'static str, &Function)> {
        self.functions.iter().map(|(n, f)| (*n, f))
    }

    /// Gets the type for `Array[Int]`.
    pub fn array_int_type(&self) -> &Type {
        &self.array_int
    }

    /// Gets the type for `Array[String]`.
    pub fn array_string_type(&self) -> &Type {
        &self.array_string
    }

    /// Gets the type for `Array[File]`.
    pub fn array_file_type(&self) -> &Type {
        &self.array_file
    }

    /// Gets the type for `Array[Object]`.
    pub fn array_object_type(&self) -> &Type {
        &self.array_object
    }

    /// Gets the type for `Array[String]+`.
    pub fn array_string_non_empty_type(&self) -> &Type {
        &self.array_string_non_empty
    }

    /// Gets the type for `Array[Array[String]]`.
    pub fn array_array_string_type(&self) -> &Type {
        &self.array_array_string
    }

    /// Gets the type for `Map[String, String]`.
    pub fn map_string_string_type(&self) -> &Type {
        &self.map_string_string
    }

    /// Gets the type for `Map[String, Int]`.
    pub fn map_string_int_type(&self) -> &Type {
        &self.map_string_int
    }
}

/// Represents the WDL standard library.
pub static STDLIB: LazyLock<StandardLibrary> = LazyLock::new(|| {
    let array_int: Type = ArrayType::new(PrimitiveType::Integer).into();
    let array_string: Type = ArrayType::new(PrimitiveType::String).into();
    let array_file: Type = ArrayType::new(PrimitiveType::File).into();
    let array_object: Type = ArrayType::new(Type::Object).into();
    let array_string_non_empty: Type = ArrayType::non_empty(PrimitiveType::String).into();
    let array_array_string: Type = ArrayType::new(array_string.clone()).into();
    let map_string_string: Type = MapType::new(PrimitiveType::String, PrimitiveType::String).into();
    let map_string_int: Type = MapType::new(PrimitiveType::String, PrimitiveType::Integer).into();
    let mut functions = IndexMap::new();

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#floor
    assert!(
        functions
            .insert(
                "floor",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .parameter("value", PrimitiveType::Float, "The number to round.")
                        .ret(PrimitiveType::Integer)
                        .definition(
                            r#"
Rounds a floating point number **down** to the next lower integer.

**Parameters**:

1. `Float`: the number to round.

**Returns**: An integer.

Example: test_floor.wdl

```wdl
version 1.2

workflow test_floor {
  input {
    Int i1
  }

  Int i2 = i1 - 1
  Float f1 = i1
  Float f2 = i1 - 0.1
  
  output {
    Array[Boolean] all_true = [floor(f1) == i1, floor(f2) == i2]
  }
}
```"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#ceil
    assert!(
        functions
            .insert(
                "ceil",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .parameter("value", PrimitiveType::Float, "The number to round.")
                        .ret(PrimitiveType::Integer)
                        .definition(
                            r#"
Rounds a floating point number **up** to the next higher integer.

**Parameters**:

1. `Float`: the number to round.

**Returns**: An integer.

Example: test_ceil.wdl

```wdl
version 1.2

workflow test_ceil {
  input {
    Int i1
  }

  Int i2 = i1 + 1
  Float f1 = i1
  Float f2 = i1 + 0.1
  
  output {
    Array[Boolean] all_true = [ceil(f1) == i1, ceil(f2) == i2]
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#round
    assert!(
        functions
            .insert(
                "round",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .parameter("value", PrimitiveType::Float, "The number to round.")
                        .ret(PrimitiveType::Integer)
                        .definition(r#"
Rounds a floating point number to the nearest integer based on standard rounding rules ("round half up").

**Parameters**:

1. `Float`: the number to round.

**Returns**: An integer.

Example: test_round.wdl

```wdl
version 1.2

workflow test_round {
  input {
    Int i1
  }

  Int i2 = i1 + 1
  Float f1 = i1 + 0.49
  Float f2 = i1 + 0.50
  
  output {
    Array[Boolean] all_true = [round(f1) == i1, round(f2) == i2]
  }
}
```
"#
                    )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    const MIN_DEFINITION: &str = r#"
Returns the smaller of two values. If both values are `Int`s, the return value is an `Int`, otherwise it is a `Float`.

**Parameters**:

1. `Int|Float`: the first number to compare.
2. `Int|Float`: the second number to compare.

**Returns**: The smaller of the two arguments.

Example: test_min.wdl

```wdl
version 1.2

workflow test_min {
  input {
    Int value1
    Float value2
  }

  output {
    # these two expressions are equivalent
    Float min1 = if value1 < value2 then value1 else value2
    Float min2 = min(value1, value2)
  }
}
```
"#;

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#min
    assert!(
        functions
            .insert(
                "min",
                PolymorphicFunction::new(vec![
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .parameter("a", PrimitiveType::Integer, "The first number to compare.",)
                        .parameter("b", PrimitiveType::Integer, "The second number to compare.",)
                        .ret(PrimitiveType::Integer)
                        .definition(MIN_DEFINITION)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .parameter("a", PrimitiveType::Integer, "The first number to compare.",)
                        .parameter("b", PrimitiveType::Float, "The second number to compare.")
                        .ret(PrimitiveType::Float)
                        .definition(MIN_DEFINITION)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .parameter("a", PrimitiveType::Float, "The first number to compare.")
                        .parameter("b", PrimitiveType::Integer, "The second number to compare.",)
                        .ret(PrimitiveType::Float)
                        .definition(MIN_DEFINITION)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .parameter("a", PrimitiveType::Float, "The first number to compare.")
                        .parameter("b", PrimitiveType::Float, "The second number to compare.")
                        .ret(PrimitiveType::Float)
                        .definition(MIN_DEFINITION)
                        .build(),
                ])
                .into(),
            )
            .is_none()
    );

    const MAX_DEFINITION: &str = r#"
Returns the larger of two values. If both values are `Int`s, the return value is an `Int`, otherwise it is a `Float`.

**Parameters**:

1. `Int|Float`: the first number to compare.
2. `Int|Float`: the second number to compare.

**Returns**: The larger of the two arguments.

Example: test_max.wdl

```wdl
version 1.2

workflow test_max {
  input {
    Int value1
    Float value2
  }

  output {
    # these two expressions are equivalent
    Float min1 = if value1 > value2 then value1 else value2
    Float min2 = max(value1, value2)
  }
}
```
"#;

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#max
    assert!(
        functions
            .insert(
                "max",
                PolymorphicFunction::new(vec![
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .parameter("a", PrimitiveType::Integer, "The first number to compare.")
                        .parameter("b", PrimitiveType::Integer, "The second number to compare.")
                        .ret(PrimitiveType::Integer)
                        .definition(MAX_DEFINITION)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .parameter("a", PrimitiveType::Integer, "The first number to compare.")
                        .parameter("b", PrimitiveType::Float, "The second number to compare.")
                        .ret(PrimitiveType::Float)
                        .definition(MAX_DEFINITION)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .parameter("a", PrimitiveType::Float, "The first number to compare.")
                        .parameter("b", PrimitiveType::Integer, "The second number to compare.",)
                        .ret(PrimitiveType::Float)
                        .definition(MAX_DEFINITION)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .parameter("a", PrimitiveType::Float, "The first number to compare.")
                        .parameter("b", PrimitiveType::Float, "The second number to compare.")
                        .ret(PrimitiveType::Float)
                        .definition(MAX_DEFINITION)
                        .build(),
                ])
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#-find
    assert!(
        functions
            .insert(
                "find",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .parameter("input", PrimitiveType::String, "The input string to search.")
                        .parameter("pattern", PrimitiveType::String, "The pattern to search for.")
                        .ret(Type::from(PrimitiveType::String).optional())
                        .definition(
                            r#"
Given two `String` parameters `input` and `pattern`, searches for the occurrence of `pattern` within `input` and returns the first match or `None` if there are no matches. `pattern` is a [regular expression](https://en.wikipedia.org/wiki/Regular_expression) and is evaluated as a [POSIX Extended Regular Expression (ERE)](https://en.wikipedia.org/wiki/Regular_expression#POSIX_basic_and_extended).

Note that regular expressions are written using regular WDL strings, so backslash characters need to be double-escaped. For example:

```wdl
String? first_match = find("hello\tBob", "\t")
```

**Parameters**

1. `String`: the input string to search.
2. `String`: the pattern to search for.

**Returns**: The contents of the first match, or `None` if `pattern` does not match `input`.

Example: test_find_task.wdl

```wdl
version 1.2
workflow find_string {
  input {
    String in = "hello world"
    String pattern1 = "e..o"
    String pattern2 = "goodbye"
  }
  output {
    String? match1 = find(in, pattern1)  # "ello"
    String? match2 = find(in, pattern2)  # None
  }  
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#-matches
    assert!(
        functions
            .insert(
                "matches",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .parameter("input", PrimitiveType::String, "The input string to search.")
                        .parameter("pattern", PrimitiveType::String, "The pattern to search for.")
                        .ret(PrimitiveType::Boolean)
                        .definition(
                            r#"
Given two `String` parameters `input` and `pattern`, tests whether `pattern` matches `input` at least once. `pattern` is a [regular expression](https://en.wikipedia.org/wiki/Regular_expression) and is evaluated as a [POSIX Extended Regular Expression (ERE)](https://en.wikipedia.org/wiki/Regular_expression#POSIX_basic_and_extended).

To test whether `pattern` matches the entire `input`, make sure to begin and end the pattern with anchors. For example:

```wdl
Boolean full_match = matches("abc123", "^a.+3$")
```

Note that regular expressions are written using regular WDL strings, so backslash characters need to be double-escaped. For example:

```wdl
Boolean has_tab = matches("hello\tBob", "\t")
```

**Parameters**

1. `String`: the input string to search.
2. `String`: the pattern to search for.

**Returns**: `true` if `pattern` matches `input` at least once, otherwise `false`.

Example: test_matches_task.wdl

```wdl
version 1.2
workflow contains_string {
  input {
    File fastq
  }
  output {
    Boolean is_compressed = matches(basename(fastq), "\\.(gz|zip|zstd)")
    Boolean is_read1 = matches(basename(fastq), "_R1")
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#sub
    assert!(
        functions
            .insert(
                "sub",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .parameter("input", PrimitiveType::String, "The input string.")
                        .parameter("pattern", PrimitiveType::String, "The pattern to search for.")
                        .parameter("replace", PrimitiveType::String, "The replacement string.")
                        .ret(PrimitiveType::String)
                        .definition(
                            r#"
Given three `String` parameters `input`, `pattern`, `replace`, this function replaces all non-overlapping occurrences of `pattern` in `input` by `replace`. `pattern` is a [regular expression](https://en.wikipedia.org/wiki/Regular_expression) and is evaluated as a [POSIX Extended Regular Expression (ERE)](https://en.wikipedia.org/wiki/Regular_expression#POSIX_basic_and_extended).
Regular expressions are written using regular WDL strings, so backslash characters need to be double-escaped (e.g., "\t").

🗑 The option for execution engines to allow other regular expression grammars besides POSIX ERE is deprecated.

**Parameters**:

1. `String`: the input string.
2. `String`: the pattern to search for.
3. `String`: the replacement string.

**Returns**: the input string, with all occurrences of the pattern replaced by the replacement string.

Example: test_sub.wdl

```wdl
version 1.2

workflow test_sub {
  String chocolike = "I like chocolate when\nit's late"

  output {
    String chocolove = sub(chocolike, "like", "love") # I love chocolate when\nit's late
    String chocoearly = sub(chocolike, "late", "early") # I like chocoearly when\nit's early
    String chocolate = sub(chocolike, "late$", "early") # I like chocolate when\nit's early
    String chocoearlylate = sub(chocolike, "[^ ]late", "early") # I like chocearly when\nit's late
    String choco4 = sub(chocolike, " [:alpha:]{4} ", " 4444 ") # I 4444 chocolate 4444\nit's late
    String no_newline = sub(chocolike, "\n", " ") # "I like chocolate when it's late"
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.3/SPEC.md#-split
    assert!(
        functions
            .insert(
                "split",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Three))
                        .parameter("input", PrimitiveType::String, "The input string.")
                        .parameter("delimiter", PrimitiveType::String, "The delimiter to split on as a regular expression.")
                        .ret(array_string.clone())
                        .definition(
                            r#"
Given the two `String` parameters `input` and `delimiter`, this function splits the input string on the provided delimiter and stores the results in a `Array[String]`. `delimiter` is a [regular expression](https://en.wikipedia.org/wiki/Regular_expression) and is evaluated as a [POSIX Extended Regular Expression (ERE)](https://en.wikipedia.org/wiki/Regular_expression#POSIX_basic_and_extended).
Regular expressions are written using regular WDL strings, so backslash characters need to be double-escaped (e.g., `"\\t"`).

**Parameters**:

1. `String`: the input string.
2. `String`: the delimiter to split on as a regular expression.

**Returns**: the parts of the input string split by the delimiter. If the input delimiter does not match anything in the input string, an array containing a single entry of the input string is returned.

<details>
<summary>
Example: test_split.wdl

```wdl
version 1.3

workflow test_split {
  String in = "Here's an example\nthat takes up multiple lines"

  output {
    Array[String] split_by_word = split(in, " ")
    Array[String] split_by_newline = split(in, "\\n")
    Array[String] split_by_both = split(in, "\s")
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    const BASENAME_DEFINITION: &str = r#"
Returns the "basename" of a file or directory - the name after the last directory separator in the path. 

The optional second parameter specifies a literal suffix to remove from the file name. If the file name does not end with the specified suffix then it is ignored.

**Parameters**

1. `File|Directory`: Path of the file or directory to read. If the argument is a `String`, it is assumed to be a local file path relative to the current working directory of the task.
2. `String`: (Optional) Suffix to remove from the file name.

**Returns**: The file's basename as a `String`.

Example: test_basename.wdl

```wdl
version 1.2

workflow test_basename {
  output {
    Boolean is_true1 = basename("/path/to/file.txt") == "file.txt"
    Boolean is_true2 = basename("/path/to/file.txt", ".txt") == "file"
    Boolean is_true3 = basename("/path/to/dir") == "dir" 
  }
}
```
"#;

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#basename
    assert!(
        functions
            .insert(
                "basename",
                PolymorphicFunction::new(vec![
                    FunctionSignature::builder()
                        .required(1)
                        .parameter(
                            "path",
                            PrimitiveType::File,
                            "Path of the file or directory to read. If the argument is a \
                             `String`, it is assumed to be a local file path relative to the \
                             current working directory of the task.",
                        )
                        .parameter(
                            "suffix",
                            PrimitiveType::String,
                            "(Optional) Suffix to remove from the file name.",
                        )
                        .ret(PrimitiveType::String)
                        .definition(BASENAME_DEFINITION)
                        .build(),
                    // This overload isn't explicitly specified in the spec, but the spec
                    // allows for `String` where file/directory are accepted; an explicit
                    // `String` overload is required as `String` may coerce to either `File` or
                    // `Directory`, which is ambiguous.
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .required(1)
                        .parameter(
                            "path",
                            PrimitiveType::String,
                            "Path of the file or directory to read. If the argument is a \
                             `String`, it is assumed to be a local file path relative to the \
                             current working directory of the task."
                        )
                        .parameter(
                            "suffix",
                            PrimitiveType::String,
                            "(Optional) Suffix to remove from the file name."
                        )
                        .ret(PrimitiveType::String)
                        .definition(BASENAME_DEFINITION)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .required(1)
                        .parameter(
                            "path",
                            PrimitiveType::Directory,
                            "Path of the file or directory to read. If the argument is a \
                             `String`, it is assumed to be a local file path relative to the \
                             current working directory of the task.",
                        )
                        .parameter(
                            "suffix",
                            PrimitiveType::String,
                            "(Optional) Suffix to remove from the file name.",
                        )
                        .ret(PrimitiveType::String)
                        .definition(BASENAME_DEFINITION)
                        .build(),
                ])
                .into(),
            )
            .is_none()
    );

    const JOIN_PATHS_DEFINITION: &str = r#"
Joins together two or more paths into an absolute path in the host filesystem.

There are three variants of this function:

1. `File join_paths(File, String)`: Joins together exactly two paths. The first path may be either absolute or relative and must specify a directory; the second path is relative to the first path and may specify a file or directory.
2. `File join_paths(File, Array[String]+)`: Joins together any number of relative paths with a base path. The first argument may be either an absolute or a relative path and must specify a directory. The paths in the second array argument must all be relative. The *last* element may specify a file or directory; all other elements must specify a directory.
3. `File join_paths(Array[String]+)`: Joins together any number of paths. The array must not be empty. The *first* element of the array may be either absolute or relative; subsequent path(s) must be relative. The *last* element may specify a file or directory; all other elements must specify a directory.

An absolute path starts with `/` and indicates that the path is relative to the root of the environment in which the task is executed. Only the first path may be absolute. If any subsequent paths are absolute, it is an error.

A relative path does not start with `/` and indicates the path is relative to its parent directory. It is up to the execution engine to determine which directory to use as the parent when resolving relative paths; by default it is the working directory in which the task is executed.

**Parameters**

1. `File|Array[String]+`: Either a path or an array of paths.
2. `String|Array[String]+`: A relative path or paths; only allowed if the first argument is a `File`.

**Returns**: A `File` representing an absolute path that results from joining all the paths in order (left-to-right), and resolving the resulting path against the default parent directory if it is relative.

Example: join_paths_task.wdl

```wdl
version 1.2

task resolve_paths_task {
  input {
    File abs_file = "/usr"
    String abs_str = "/usr"
    String rel_dir_str = "bin"
    File rel_file = "echo"
    File rel_dir_file = "mydir"
    String rel_str = "mydata.txt"
  }

  # these are all equivalent to '/usr/bin/echo'
  File bin1 = join_paths(abs_file, [rel_dir_str, rel_file])
  File bin2 = join_paths(abs_str, [rel_dir_str, rel_file])
  File bin3 = join_paths([abs_str, rel_dir_str, rel_file])
  
  # the default behavior is that this resolves to 
  # '<working dir>/mydir/mydata.txt'
  File data = join_paths(rel_dir_file, rel_str)
  
  # this resolves to '<working dir>/bin/echo', which is non-existent
  File doesnt_exist = join_paths([rel_dir_str, rel_file])
  command <<<
    mkdir ~{rel_dir_file}
    ~{bin1} -n "hello" > ~{data}
  >>>

  output {
    Boolean bins_equal = (bin1 == bin2) && (bin1 == bin3)
    String result = read_string(data)
    File? missing_file = doesnt_exist
  }
  
  runtime {
    container: "ubuntu:latest"
  }
}
```
"#;

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#-join_paths
    assert!(
        functions
            .insert(
                "join_paths",
                PolymorphicFunction::new(vec![
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .parameter(
                            "base",
                            PrimitiveType::File,
                            "Either a path or an array of paths.",
                        )
                        .parameter(
                            "relative",
                            PrimitiveType::String,
                            "A relative path or paths; only allowed if the first argument is a \
                             `File`.",
                        )
                        .ret(PrimitiveType::File)
                        .definition(JOIN_PATHS_DEFINITION)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .parameter(
                            "base",
                            PrimitiveType::File,
                            "Either a path or an array of paths."
                        )
                        .parameter(
                            "relative",
                            array_string_non_empty.clone(),
                            "A relative path or paths; only allowed if the first argument is a \
                             `File`."
                        )
                        .ret(PrimitiveType::File)
                        .definition(JOIN_PATHS_DEFINITION)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .parameter(
                            "paths",
                            array_string_non_empty.clone(),
                            "Either a path or an array of paths."
                        )
                        .ret(PrimitiveType::File)
                        .definition(JOIN_PATHS_DEFINITION)
                        .build(),
                ])
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#glob
    assert!(
        functions
            .insert(
                "glob",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .parameter("pattern", PrimitiveType::String, "The glob string.")
                        .ret(array_file.clone())
                        .definition(
                            r#"
Returns the Bash expansion of the [glob string](https://en.wikipedia.org/wiki/Glob_(programming)) relative to the task's execution directory, and in the same order.

`glob` finds all of the files (but not the directories) in the same order as would be matched by running `echo <glob>` in Bash from the task's execution directory.

At least in standard Bash, glob expressions are not evaluated recursively, i.e., files in nested directories are not included. 

**Parameters**:

1. `String`: The glob string.

**Returns**: A array of all files matched by the glob.

Example: gen_files_task.wdl

```wdl
version 1.2

task gen_files {
  input {
    Int num_files
  }

  command <<<
    for i in 1..~{num_files}; do
      printf ${i} > a_file_${i}.txt
    done
    mkdir a_dir
    touch a_dir/a_inner.txt
  >>>

  output {
    Array[File] files = glob("a_*")
    Int glob_len = length(files)
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    const SIZE_DEFINITION: &str = r#"
Determines the size of a file, directory, or the sum total sizes of the files/directories contained within a compound value. The files may be optional values; `None` values have a size of `0.0`. By default, the size is returned in bytes unless the optional second argument is specified with a [unit](#units-of-storage)

In the second variant of the `size` function, the parameter type `X` represents any compound type that contains `File` or `File?` nested at any depth.

If the size cannot be represented in the specified unit because the resulting value is too large to fit in a `Float`, an error is raised. It is recommended to use a unit that will always be large enough to handle any expected inputs without numerical overflow.

**Parameters**

1. `File|File?|Directory|Directory?|X|X?`: A file, directory, or a compound value containing files/directories, for which to determine the size.
2. `String`: (Optional) The unit of storage; defaults to 'B'.

**Returns**: The size of the files/directories as a `Float`.

Example: file_sizes_task.wdl

```wdl
version 1.2

task file_sizes {
  command <<<
    printf "this file is 22 bytes\n" > created_file
  >>>

  File? missing_file = None

  output {
    File created_file = "created_file"
    Float missing_file_bytes = size(missing_file)
    Float created_file_bytes = size(created_file, "B")
    Float multi_file_kb = size([created_file, missing_file], "K")

    Map[String, Pair[Int, File]] nested = {
      "a": (10, created_file),
      "b": (50, missing_file)
    }
    Float nested_bytes = size(nested)
  }
  
  requirements {
    container: "ubuntu:latest"
  }
}
```
"#;

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#size
    assert!(
        functions
            .insert(
                "size",
                PolymorphicFunction::new(vec![
                    // This overload isn't explicitly in the spec, but it fixes an ambiguity in 1.2
                    // when passed a literal `None` value.
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .required(1)
                        .parameter(
                            "value",
                            Type::None,
                            "A file, directory, or a compound value containing files/directories, \
                             for which to determine the size."
                        )
                        .parameter(
                            "unit",
                            PrimitiveType::String,
                            "(Optional) The unit of storage; defaults to 'B'."
                        )
                        .ret(PrimitiveType::Float)
                        .definition(SIZE_DEFINITION)
                        .build(),
                    FunctionSignature::builder()
                        .required(1)
                        .parameter(
                            "value",
                            Type::from(PrimitiveType::File).optional(),
                            "A file, directory, or a compound value containing files/directories, \
                             for which to determine the size."
                        )
                        .parameter(
                            "unit",
                            PrimitiveType::String,
                            "(Optional) The unit of storage; defaults to 'B'."
                        )
                        .ret(PrimitiveType::Float)
                        .definition(SIZE_DEFINITION)
                        .build(),
                    // This overload isn't explicitly specified in the spec, but the spec
                    // allows for `String` where file/directory are accepted; an explicit
                    // `String` overload is required as `String` may coerce to either `File` or
                    // `Directory`, which is ambiguous.
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .required(1)
                        .parameter(
                            "value",
                            Type::from(PrimitiveType::String).optional(),
                            "A file, directory, or a compound value containing files/directories, \
                             for which to determine the size.",
                        )
                        .parameter(
                            "unit",
                            PrimitiveType::String,
                            "(Optional) The unit of storage; defaults to 'B'.",
                        )
                        .ret(PrimitiveType::Float)
                        .definition(SIZE_DEFINITION)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .required(1)
                        .parameter(
                            "value",
                            Type::from(PrimitiveType::Directory).optional(),
                            "A file, directory, or a compound value containing files/directories, \
                             for which to determine the size."
                        )
                        .parameter(
                            "unit",
                            PrimitiveType::String,
                            "(Optional) The unit of storage; defaults to 'B'."
                        )
                        .ret(PrimitiveType::Float)
                        .definition(SIZE_DEFINITION)
                        .build(),
                    FunctionSignature::builder()
                        .required(1)
                        .type_parameter("X", SizeableConstraint)
                        .parameter(
                            "value",
                            GenericType::Parameter("X"),
                            "A file, directory, or a compound value containing files/directories, \
                             for which to determine the size."
                        )
                        .parameter(
                            "unit",
                            PrimitiveType::String,
                            "(Optional) The unit of storage; defaults to 'B'."
                        )
                        .ret(PrimitiveType::Float)
                        .definition(SIZE_DEFINITION)
                        .build(),
                ])
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#stdout
    assert!(
        functions
            .insert(
                "stdout",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .ret(PrimitiveType::File)
                        .definition(
                            r#"
Returns the value of the executed command's standard output (stdout) as a `File`. The engine should give the file a random name and write it in a temporary directory, so as not to conflict with any other task output files.

**Parameters**: None

**Returns**: A `File` whose contents are the stdout generated by the command of the task where the function is called.

Example: echo_stdout.wdl

```wdl
version 1.2

task echo_stdout {
  command <<<
    printf "hello world"
  >>>

  output {
    File message = read_string(stdout())
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#stderr
    assert!(
        functions
            .insert(
                "stderr",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .ret(PrimitiveType::File)
                        .definition(
                            r#"
Returns the value of the executed command's standard error (stderr) as a `File`. The file should be given a random name and written in a temporary directory, so as not to conflict with any other task output files.

**Parameters**: None

**Returns**: A `File` whose contents are the stderr generated by the command of the task where the function is called.

Example: echo_stderr.wdl

```wdl
version 1.2

task echo_stderr {
  command <<<
    >&2 printf "hello world"
  >>>

  output {
    File message = read_string(stderr())
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#read_string
    assert!(
        functions
            .insert(
                "read_string",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .parameter("file", PrimitiveType::File, "Path of the file to read.")
                        .ret(PrimitiveType::String)
                        .definition(
                            r#"
Reads an entire file as a `String`, with any trailing end-of-line characters (`` and `\n`) stripped off. If the file is empty, an empty string is returned.

If the file contains any internal newline characters, they are left in tact.

**Parameters**

1. `File`: Path of the file to read.

**Returns**: A `String`.

Example: read_string_task.wdl

```wdl
version 1.2

task read_string {
  # this file will contain "this\nfile\nhas\nfive\nlines\n"
  File f = write_lines(["this", "file", "has", "five", "lines"])
  
  command <<<
  cat ~{f}
  >>>
  
  output {
    # s will contain "this\nfile\nhas\nfive\nlines"
    String s = read_string(stdout())
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#read_int
    assert!(
        functions
            .insert(
                "read_int",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .parameter("file", PrimitiveType::File, "Path of the file to read.")
                        .ret(PrimitiveType::Integer)
                        .definition(
                            r#"
Reads a file that contains a single line containing only an integer and (optional) whitespace. If the line contains a valid integer, that value is returned as an `Int`. If the file is empty or does not contain a single integer, an error is raised.

**Parameters**

1. `File`: Path of the file to read.

**Returns**: An `Int`.

Example: read_int_task.wdl

```wdl
version 1.2

task read_int {
  command <<<
  printf "  1  \n" > int_file
  >>>

  output {
    Int i = read_int("int_file")
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#read_float
    assert!(
        functions
            .insert(
                "read_float",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .parameter("file", PrimitiveType::File, "Path of the file to read.")
                        .ret(PrimitiveType::Float)
                        .definition(
                            r#"
Reads a file that contains only a numeric value and (optional) whitespace. If the line contains a valid floating point number, that value is returned as a `Float`. If the file is empty or does not contain a single float, an error is raised.

**Parameters**

1. `File`: Path of the file to read.

**Returns**: A `Float`.

Example: read_float_task.wdl

```wdl
version 1.2

task read_float {
  command <<<
  printf "  1  \n" > int_file
  printf "  2.0  \n" > float_file
  >>>

  output {
    Float f1 = read_float("int_file")
    Float f2 = read_float("float_file")
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#read_boolean
    assert!(
        functions
            .insert(
                "read_boolean",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .parameter("file", PrimitiveType::File, "Path of the file to read.")
                        .ret(PrimitiveType::Boolean)
                        .definition(
                            r#"
Reads a file that contains a single line containing only a boolean value and (optional) whitespace. If the non-whitespace content of the line is "true" or "false", that value is returned as a `Boolean`. If the file is empty or does not contain a single boolean, an error is raised. The comparison is case- and whitespace-insensitive.

**Parameters**

1. `File`: Path of the file to read.

**Returns**: A `Boolean`.

Example: read_bool_task.wdl

```wdl
version 1.2

task read_bool {
  command <<<
  printf "  true  \n" > true_file
  printf "  FALSE  \n" > false_file
  >>>

  output {
    Boolean b1 = read_boolean("true_file")
    Boolean b2 = read_boolean("false_file")
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#read_lines
    assert!(
        functions
            .insert(
                "read_lines",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .parameter("file", PrimitiveType::File, "Path of the file to read.")
                        .ret(array_string.clone())
                        .definition(
                            r#"
Reads each line of a file as a `String`, and returns all lines in the file as an `Array[String]`. Trailing end-of-line characters (`` and `\n`) are removed from each line.

The order of the lines in the returned `Array[String]` is the order in which the lines appear in the file.

If the file is empty, an empty array is returned.

**Parameters**

1. `File`: Path of the file to read.

**Returns**: An `Array[String]` representation of the lines in the file.

Example: grep_task.wdl

```wdl
version 1.2

task grep {
  input {
    String pattern
    File file
  }

  command <<<
    grep '~{pattern}' ~{file}
  >>>

  output {
    Array[String] matches = read_lines(stdout())
  }
  
  requirements {
    container: "ubuntu:latest"
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#write_lines
    assert!(
        functions
            .insert(
                "write_lines",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .parameter("array", array_string.clone(), "`Array` of strings to write.")
                        .ret(PrimitiveType::File)
                        .definition(
                            r#"
Writes a file with one line for each element in a `Array[String]`. All lines are terminated by the newline (`\n`) character (following the [POSIX standard](https://pubs.opengroup.org/onlinepubs/9699919799/basedefs/V1_chap03.html#tag_03_206)). If the `Array` is empty, an empty file is written.

**Parameters**

1. `Array[String]`: Array of strings to write.

**Returns**: A `File`.

Example: write_lines_task.wdl

```wdl
version 1.2

task write_lines {
  input {
    Array[String] array = ["first", "second", "third"]
  }

  command <<<
    paste -s -d'\t' ~{write_lines(array)}
  >>>

  output {
    String s = read_string(stdout())
  }
  
  requirements {
    container: "ubuntu:latest"
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    const READ_TSV_DEFINITION: &str = r#"
Reads a tab-separated value (TSV) file as an `Array[Array[String]]` representing a table of values. Trailing end-of-line characters (`` and `\n`) are removed from each line.

This function has three variants:

1. `Array[Array[String]] read_tsv(File, [false])`: Returns each row of the table as an `Array[String]`. There is no requirement that the rows of the table are all the same length.
2. `Array[Object] read_tsv(File, true)`: The second parameter must be `true` and specifies that the TSV file contains a header line. Each row is returned as an `Object` with its keys determined by the header (the first line in the file) and its values as `String`s. All rows in the file must be the same length and the field names in the header row must be valid `Object` field names, or an error is raised.
3. `Array[Object] read_tsv(File, Boolean, Array[String])`: The second parameter specifies whether the TSV file contains a header line, and the third parameter is an array of field names that is used to specify the field names to use for the returned `Object`s. If the second parameter is `true`, the specified field names override those in the file's header (i.e., the header line is ignored).

If the file is empty, an empty array is returned.

If the entire contents of the file can not be read for any reason, the calling task or workflow fails with an error. Examples of failure include, but are not limited to, not having access to the file, resource limitations (e.g. memory) when reading the file, and implementation-imposed file size limits.

**Parameters**

1. `File`: The TSV file to read.
2. `Boolean`: (Optional) Whether to treat the file's first line as a header.
3. `Array[String]`: (Optional) An array of field names. If specified, then the second parameter is also required.

**Returns**: An `Array` of rows in the TSV file, where each row is an `Array[String]` of fields or an `Object` with keys determined by the second and third parameters and `String` values.

Example: read_tsv_task.wdl

```wdl
version 1.2

task read_tsv {
  command <<<
    {
      printf "row1\tvalue1\n"
      printf "row2\tvalue2\n"
      printf "row3\tvalue3\n"
    } >> data.no_headers.tsv

    {
      printf "header1\theader2\n"
      printf "row1\tvalue1\n"
      printf "row2\tvalue2\n"
      printf "row3\tvalue3\n"
    } >> data.headers.tsv
  >>>

  output {
    Array[Array[String]] output_table = read_tsv("data.no_headers.tsv")
    Array[Object] output_objs1 = read_tsv("data.no_headers.tsv", false, ["name", "value"])
    Array[Object] output_objs2 = read_tsv("data.headers.tsv", true)
    Array[Object] output_objs3 = read_tsv("data.headers.tsv", true, ["name", "value"])
  }
}
```
"#;

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#read_tsv
    assert!(
        functions
            .insert(
                "read_tsv",
                PolymorphicFunction::new(vec![
                    FunctionSignature::builder()
                        .parameter("file", PrimitiveType::File, "The TSV file to read.")
                        .ret(array_array_string.clone())
                        .definition(READ_TSV_DEFINITION)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .parameter("file", PrimitiveType::File, "The TSV file to read.")
                        .parameter(
                            "header",
                            PrimitiveType::Boolean,
                            "(Optional) Whether to treat the file's first line as a header.",
                        )
                        .ret(array_object.clone())
                        .definition(READ_TSV_DEFINITION)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .parameter("file", PrimitiveType::File, "The TSV file to read.")
                        .parameter(
                            "header",
                            PrimitiveType::Boolean,
                            "(Optional) Whether to treat the file's first line as a header.",
                        )
                        .parameter(
                            "columns",
                            array_string.clone(),
                            "(Optional) An array of field names. If specified, then the second \
                             parameter is also required.",
                        )
                        .ret(array_object.clone())
                        .definition(READ_TSV_DEFINITION)
                        .build(),
                ])
                .into(),
            )
            .is_none()
    );

    const WRITE_TSV_DEFINITION: &str = r#"
Given an `Array` of elements, writes a tab-separated value (TSV) file with one line for each element.

There are three variants of this function:

1. `File write_tsv(Array[Array[String]])`: Each element is concatenated using a tab ('\t') delimiter and written as a row in the file. There is no header row.

2. `File write_tsv(Array[Array[String]], true, Array[String])`: The second argument must be `true` and the third argument provides an `Array` of column names. The column names are concatenated to create a header that is written as the first row of the file. All elements must be the same length as the header array.

3. `File write_tsv(Array[Struct], [Boolean, [Array[String]]])`: Each element is a struct whose field values are concatenated in the order the fields are defined. The optional second argument specifies whether to write a header row. If it is `true`, then the header is created from the struct field names. If the second argument is `true`, then the optional third argument may be used to specify column names to use instead of the struct field names.

Each line is terminated by the newline (`\n`) character. 

The generated file should be given a random name and written in a temporary directory, so as not to conflict with any other task output files.

If the entire contents of the file can not be written for any reason, the calling task or workflow fails with an error. Examples of failure include, but are not limited to, insufficient disk space to write the file.


**Parameters**

1. `Array[Array[String]] | Array[Struct]`: An array of rows, where each row is either an `Array` of column values or a struct whose values are the column values.
2. `Boolean`: (Optional) Whether to write a header row.
3. `Array[String]`: An array of column names. If the first argument is `Array[Array[String]]` and the second argument is `true` then it is required, otherwise it is optional. Ignored if the second argument is `false`.


**Returns**: A `File`.

Example: write_tsv_task.wdl

```wdl
version 1.2

task write_tsv {
  input {
    Array[Array[String]] array = [["one", "two", "three"], ["un", "deux", "trois"]]
    Array[Numbers] structs = [
      Numbers {
        first: "one",
        second: "two",
        third: "three"
      },
      Numbers {
        first: "un",
        second: "deux",
        third: "trois"
      }
    ]
  }

  command <<<
    cut -f 1 ~{write_tsv(array)} >> array_no_header.txt
    cut -f 1 ~{write_tsv(array, true, ["first", "second", "third"])} > array_header.txt
    cut -f 1 ~{write_tsv(structs)} >> structs_default.txt
    cut -f 2 ~{write_tsv(structs, false)} >> structs_no_header.txt
    cut -f 2 ~{write_tsv(structs, true)} >> structs_header.txt
    cut -f 3 ~{write_tsv(structs, true, ["no1", "no2", "no3"])} >> structs_user_header.txt
  >>>

  output {
    Array[String] array_no_header = read_lines("array_no_header.txt")
    Array[String] array_header = read_lines("array_header.txt")
    Array[String] structs_default = read_lines("structs_default.txt")
    Array[String] structs_no_header = read_lines("structs_no_header.txt")
    Array[String] structs_header = read_lines("structs_header.txt")
    Array[String] structs_user_header = read_lines("structs_user_header.txt")

  }
  
  requirements {
    container: "ubuntu:latest"
  }
}
```
"#;

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#write_tsv
    assert!(
        functions
            .insert(
                "write_tsv",
                PolymorphicFunction::new(vec![
                    FunctionSignature::builder()
                        .parameter(
                            "data",
                            array_array_string.clone(),
                            "An array of rows, where each row is either an `Array` of column \
                             values or a struct whose values are the column values.",
                        )
                        .ret(PrimitiveType::File)
                        .definition(WRITE_TSV_DEFINITION)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .parameter(
                            "data",
                            array_array_string.clone(),
                            "An array of rows, where each row is either an `Array` of column \
                             values or a struct whose values are the column values.",
                        )
                        .parameter(
                            "header",
                            PrimitiveType::Boolean,
                            "(Optional) Whether to write a header row.",
                        )
                        .parameter(
                            "columns",
                            array_string.clone(),
                            "An array of column names. If the first argument is \
                             `Array[Array[String]]` and the second argument is true then it is \
                             required, otherwise it is optional. Ignored if the second argument \
                             is false."
                        )
                        .ret(PrimitiveType::File)
                        .definition(WRITE_TSV_DEFINITION)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .type_parameter("S", PrimitiveStructConstraint)
                        .required(1)
                        .parameter(
                            "data",
                            GenericArrayType::new(GenericType::Parameter("S")),
                            "An array of rows, where each row is either an `Array` of column \
                             values or a struct whose values are the column values.",
                        )
                        .parameter(
                            "header",
                            PrimitiveType::Boolean,
                            "(Optional) Whether to write a header row.",
                        )
                        .parameter(
                            "columns",
                            array_string.clone(),
                            "An array of column names. If the first argument is \
                             `Array[Array[String]]` and the second argument is true then it is \
                             required, otherwise it is optional. Ignored if the second argument \
                             is false."
                        )
                        .ret(PrimitiveType::File)
                        .definition(WRITE_TSV_DEFINITION)
                        .build(),
                ])
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#read_map
    assert!(
        functions
            .insert(
                "read_map",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .parameter(
                            "file",
                            PrimitiveType::File,
                            "Path of the two-column TSV file to read.",
                        )
                        .ret(map_string_string.clone())
                        .definition(
                            r#"
Reads a tab-separated value (TSV) file representing a set of pairs. Each row must have exactly two columns, e.g., `col1\tcol2`. Trailing end-of-line characters (`` and `\n`) are removed from each line.

Each pair is added to a `Map[String, String]` in order. The values in the first column must be unique; if there are any duplicate keys, an error is raised.

If the file is empty, an empty map is returned.

**Parameters**

1. `File`: Path of the two-column TSV file to read.

**Returns**: A `Map[String, String]`, with one element for each row in the TSV file.

Example: read_map_task.wdl

```wdl
version 1.2

task read_map {
  command <<<
    printf "key1\tvalue1\n" >> map_file
    printf "key2\tvalue2\n" >> map_file
  >>>
  
  output {
    Map[String, String] mapping = read_map(stdout())
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#write_map
    assert!(
        functions
            .insert(
                "write_map",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .parameter(
                            "map",
                            map_string_string.clone(),
                            "A `Map`, where each element will be a row in the generated file.",
                        )
                        .ret(PrimitiveType::File)
                        .definition(
                            r#"
Writes a tab-separated value (TSV) file with one line for each element in a `Map[String, String]`. Each element is concatenated into a single tab-delimited string of the format `~{key}\t~{value}`. Each line is terminated by the newline (`\n`) character. If the `Map` is empty, an empty file is written.

Since `Map`s are ordered, the order of the lines in the file is guaranteed to be the same order that the elements were added to the `Map`.

**Parameters**

1. `Map[String, String]`: A `Map`, where each element will be a row in the generated file.

**Returns**: A `File`.

Example: write_map_task.wdl

```wdl
version 1.2

task write_map {
  input {
    Map[String, String] map = {"key1": "value1", "key2": "value2"}
  }

  command <<<
    cut -f 1 ~{write_map(map)}
  >>>
  
  output {
    Array[String] keys = read_lines(stdout())
  }

  requirements {
    container: "ubuntu:latest"
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#read_json
    assert!(
        functions
            .insert(
                "read_json",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .parameter("file", PrimitiveType::File, "Path of the JSON file to read.")
                        .ret(Type::Union)
                        .definition(
                            r#"
Reads a JSON file into a WDL value whose type depends on the file's contents. The mapping of JSON type to WDL type is:

| JSON Type | WDL Type         |
| --------- | ---------------- |
| object    | `Object`         |
| array     | `Array[X]`       |
| number    | `Int` or `Float` |
| string    | `String`         |
| boolean   | `Boolean`        |
| null      | `None`           |

The return value is of type [`Union`](#union-hidden-type) and must be used in a context where it can be coerced to the expected type, or an error is raised. For example, if the JSON file contains `null`, then the return value will be `None`, meaning the value can only be used in a context where an optional type is expected.

If the JSON file contains an array, then all the elements of the array must be coercible to the same type, or an error is raised.

The `read_json` function does not have access to any WDL type information, so it cannot return an instance of a specific `Struct` type. Instead, it returns a generic `Object` value that must be coerced to the desired `Struct` type.

Note that an empty file is not valid according to the JSON specification, and so calling `read_json` on an empty file raises an error.

**Parameters**

1. `File`: Path of the JSON file to read.

**Returns**: A value whose type is dependent on the contents of the JSON file.

Example: read_person.wdl

```wdl
version 1.2

struct Person {
  String name
  Int age
}

workflow read_person {
  input {
    File json_file
  }

  output {
    Person p = read_json(json_file)
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#write_json
    assert!(
        functions
            .insert(
                "write_json",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .type_parameter("X", JsonSerializableConstraint)
                        .parameter(
                            "value",
                            GenericType::Parameter("X"),
                            "A WDL value of a supported type.",
                        )
                        .ret(PrimitiveType::File)
                        .definition(
                            r#"
Writes a JSON file with the serialized form of a WDL value. The following WDL types can be serialized:

| WDL Type         | JSON Type |
| ---------------- | --------- |
| `Struct`         | object    |
| `Object`         | object    |
| `Map[String, X]` | object    |
| `Array[X]`       | array     |
| `Int`            | number    |
| `Float`          | number    |
| `String`         | string    |
| `File`           | string    |
| `Boolean`        | boolean   |
| `None`           | null      |

When serializing compound types, all nested types must be serializable or an error is raised.

**Parameters**

1. `X`: A WDL value of a supported type.

**Returns**: A `File`.

Example: write_json_fail.wdl

```wdl
version 1.2

workflow write_json_fail {
  Pair[Int, Map[Int, String]] x = (1, {2: "hello"})
  # this fails with an error - Map with Int keys is not serializable
  File f = write_json(x)
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#read_object
    assert!(
        functions
            .insert(
                "read_object",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .parameter(
                            "file",
                            PrimitiveType::File,
                            "Path of the two-row TSV file to read.",
                        )
                        .ret(Type::Object)
                        .definition(
                            r#"
Reads a tab-separated value (TSV) file representing the names and values of the members of an `Object`. There must be exactly two rows, and each row must have the same number of elements, otherwise an error is raised. Trailing end-of-line characters (`` and `\n`) are removed from each line.

The first row specifies the object member names. The names in the first row must be unique; if there are any duplicate names, an error is raised.

The second row specifies the object member values corresponding to the names in the first row. All of the `Object`'s values are of type `String`.

**Parameters**

1. `File`: Path of the two-row TSV file to read.

**Returns**: An `Object`, with as many members as there are unique names in the TSV.

Example: read_object_task.wdl

```wdl
version 1.2

task read_object {
  command <<<
    python <<CODE
    print('\t'.join(["key_{}".format(i) for i in range(3)]))
    print('\t'.join(["value_{}".format(i) for i in range(3)]))
    CODE
  >>>

  output {
    Object my_obj = read_object(stdout())
  }

  requirements {
    container: "python:latest"
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#read_objects
    assert!(
        functions
            .insert(
                "read_objects",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .parameter("file", PrimitiveType::File, "The file to read.")
                        .ret(array_object.clone())
                        .definition(
                            r#"
Reads a tab-separated value (TSV) file representing the names and values of the members of any number of `Object`s. Trailing end-of-line characters (`` and `\n`) are removed from each line.

The first line of the file must be a header row with the names of the object members. The names in the first row must be unique; if there are any duplicate names, an error is raised.

There are any number of additional rows, where each additional row contains the values of an object corresponding to the member names. Each row in the file must have the same number of fields as the header row. All of the `Object`'s values are of type `String`.

If the file is empty or contains only a header line, an empty array is returned.

**Parameters**

1. `File`: Path of the TSV file to read.

**Returns**: An `Array[Object]`, with `N-1` elements, where `N` is the number of rows in the file.

Example: read_objects_task.wdl

```wdl
version 1.2

task read_objects {
  command <<<
    python <<CODE
    print('\t'.join(["key_{}".format(i) for i in range(3)]))
    print('\t'.join(["value_A{}".format(i) for i in range(3)]))
    print('\t'.join(["value_B{}".format(i) for i in range(3)]))
    print('\t'.join(["value_C{}".format(i) for i in range(3)]))
    CODE
  >>>

  output {
    Array[Object] my_obj = read_objects(stdout())
  }
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    const WRITE_OBJECT_DEFINITION: &str = r#"
Writes a tab-separated value (TSV) file representing the names and values of the members of an `Object`. The file will contain exactly two rows. The first row specifies the object member names. The second row specifies the object member values corresponding to the names in the first row.

Each line is terminated by the newline (`\n`) character. 

The generated file should be given a random name and written in a temporary directory, so as not to conflict with any other task output files.

If the entire contents of the file can not be written for any reason, the calling task or workflow fails with an error. Examples of failure include, but are not limited to, insufficient disk space to write the file.

**Parameters**

1. `Object`: An `Object` whose members will be written to the file.

**Returns**: A `File`.

Example: write_object_task.wdl

```wdl
version 1.2

task write_object {
  input {
    Object my_obj = {"key_0": "value_A0", "key_1": "value_A1", "key_2": "value_A2"}
  }

  command <<<
    cat ~{write_object(my_obj)}
  >>>

  output {
    Object new_obj = read_object(stdout())
  }
}
```
"#;

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#write_object
    assert!(
        functions
            .insert(
                "write_object",
                PolymorphicFunction::new(vec![
                    FunctionSignature::builder()
                        .parameter("object", Type::Object, "An object to write.")
                        .ret(PrimitiveType::File)
                        .definition(WRITE_OBJECT_DEFINITION)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .type_parameter("S", PrimitiveStructConstraint)
                        .parameter("object", GenericType::Parameter("S"), "An object to write.")
                        .ret(PrimitiveType::File)
                        .definition(WRITE_OBJECT_DEFINITION)
                        .build(),
                ])
                .into(),
            )
            .is_none()
    );

    const WRITE_OBJECTS_DEFINITION: &str = r#"
Writes a tab-separated value (TSV) file representing the names and values of the members of any number of `Object`s. The first line of the file will be a header row with the names of the object members. There will be one additional row for each element in the input array, where each additional row contains the values of an object corresponding to the member names.

Each line is terminated by the newline (`\n`) character. 

The generated file should be given a random name and written in a temporary directory, so as not to conflict with any other task output files.

If the entire contents of the file can not be written for any reason, the calling task or workflow fails with an error. Examples of failure include, but are not limited to, insufficient disk space to write the file.

**Parameters**

1. `Array[Object]`: An `Array[Object]` whose elements will be written to the file.

**Returns**: A `File`.

Example: write_objects_task.wdl

```wdl
version 1.2

task write_objects {
  input {
    Array[Object] my_objs = [
      {"key_0": "value_A0", "key_1": "value_A1", "key_2": "value_A2"},
      {"key_0": "value_B0", "key_1": "value_B1", "key_2": "value_B2"},
      {"key_0": "value_C0", "key_1": "value_C1", "key_2": "value_C2"}
    ]
  }

  command <<<
    cat ~{write_objects(my_objs)}
  >>>

  output {
    Array[Object] new_objs = read_objects(stdout())
  }
}
```
"#;

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#write_objects
    assert!(
        functions
            .insert(
                "write_objects",
                PolymorphicFunction::new(vec![
                    FunctionSignature::builder()
                        .parameter("objects", array_object.clone(), "The objects to write.")
                        .ret(PrimitiveType::File)
                        .definition(WRITE_OBJECTS_DEFINITION)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .type_parameter("S", PrimitiveStructConstraint)
                        .parameter(
                            "objects",
                            GenericArrayType::new(GenericType::Parameter("S")),
                            "The objects to write."
                        )
                        .ret(PrimitiveType::File)
                        .definition(WRITE_OBJECTS_DEFINITION)
                        .build(),
                ])
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#prefix
    assert!(
        functions
            .insert(
                "prefix",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .type_parameter("P", PrimitiveTypeConstraint)
                        .parameter(
                            "prefix",
                            PrimitiveType::String,
                            "The prefix to prepend to each element in the array.",
                        )
                        .parameter(
                            "array",
                            GenericArrayType::new(GenericType::Parameter("P")),
                            "Array with a primitive element type.",
                        )
                        .ret(array_string.clone())
                        .definition(
                            r#"
Given a `String` `prefix` and an `Array[X]` `a`, returns a new `Array[String]` where each element `x` of `a` is prepended with `prefix`. The elements of `a` are converted to `String`s before being prepended. If `a` is empty, an empty array is returned.

**Parameters**

1. `String`: The string to prepend.
2. `Array[X]`: The array whose elements will be prepended.

**Returns**: A new `Array[String]` with the prepended elements.

Example: prefix_task.wdl

```wdl
version 1.2

task prefix {
  input {
    Array[Int] ints = [1, 2, 3]
  }

  output {
    Array[String] prefixed_ints = prefix("file_", ints) # ["file_1", "file_2", "file_3"]
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#suffix
    assert!(
        functions
            .insert(
                "suffix",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .type_parameter("P", PrimitiveTypeConstraint)
                        .parameter(
                            "suffix",
                            PrimitiveType::String,
                            "The suffix to append to each element in the array.",
                        )
                        .parameter(
                            "array",
                            GenericArrayType::new(GenericType::Parameter("P")),
                            "Array with a primitive element type.",
                        )
                        .ret(array_string.clone())
                        .definition(
                            r#"
Given a `String` `suffix` and an `Array[X]` `a`, returns a new `Array[String]` where each element `x` of `a` is appended with `suffix`. The elements of `a` are converted to `String`s before being appended. If `a` is empty, an empty array is returned.

**Parameters**

1. `String`: The string to append.
2. `Array[X]`: The array whose elements will be appended.

**Returns**: A new `Array[String]` with the appended elements.

Example: suffix_task.wdl

```wdl
version 1.2

task suffix {
  input {
    Array[Int] ints = [1, 2, 3]
  }

  output {
    Array[String] suffixed_ints = suffix(".txt", ints) # ["1.txt", "2.txt", "3.txt"]
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#quote
    assert!(
        functions
            .insert(
                "quote",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .type_parameter("P", PrimitiveTypeConstraint)
                        .parameter(
                            "array",
                            GenericArrayType::new(GenericType::Parameter("P")),
                            "Array with a primitive element type.",
                        )
                        .ret(array_string.clone())
                        .definition(
                            r#"
Given an `Array[X]` `a`, returns a new `Array[String]` where each element `x` of `a` is converted to a `String` and then surrounded by double quotes (`"`). If `a` is empty, an empty array is returned.

**Parameters**

1. `Array[X]`: The array whose elements will be quoted.

**Returns**: A new `Array[String]` with the quoted elements.

Example: quote_task.wdl

```wdl
version 1.2

task quote {
  input {
    Array[String] strings = ["hello", "world"]
  }

  output {
    Array[String] quoted_strings = quote(strings) # ["\"hello\"", "\"world\""]
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#squote
    assert!(
        functions
            .insert(
                "squote",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .type_parameter("P", PrimitiveTypeConstraint)
                        .parameter("array", GenericArrayType::new(GenericType::Parameter("P")), "The array of values.")                        .ret(array_string.clone())
                        .definition(
                            r#"
Given an `Array[X]` `a`, returns a new `Array[String]` where each element `x` of `a` is converted to a `String` and then surrounded by single quotes (`'`). If `a` is empty, an empty array is returned.

**Parameters**

1. `Array[X]`: The array whose elements will be single-quoted.

**Returns**: A new `Array[String]` with the single-quoted elements.

Example: squote_task.wdl

```wdl
version 1.2

task squote {
  input {
    Array[String] strings = ["hello", "world"]
  }

  output {
    Array[String] squoted_strings = squote(strings) # ["'hello'", "'world'"]
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#sep
    assert!(
        functions
            .insert(
                "sep",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .type_parameter("P", PrimitiveTypeConstraint)
                        .parameter("separator", PrimitiveType::String, "Separator string.")
                        .parameter(
                            "array",
                            GenericArrayType::new(GenericType::Parameter("P")),
                            "`Array` of strings to concatenate.",
                        )
                        .ret(PrimitiveType::String)
                        .definition(
                            r#"
Given a `String` `separator` and an `Array[X]` `a`, returns a new `String` where each element `x` of `a` is converted to a `String` and then joined by `separator`. If `a` is empty, an empty string is returned.

**Parameters**

1. `String`: The string to use as a separator.
2. `Array[X]`: The array whose elements will be joined.

**Returns**: A new `String` with the joined elements.

Example: sep_task.wdl

```wdl
version 1.2

task sep {
  input {
    Array[Int] ints = [1, 2, 3]
  }

  output {
    String joined_ints = sep(",", ints) # "1,2,3"
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#range
    assert!(
        functions
            .insert(
                "range",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .parameter("n", PrimitiveType::Integer, "The length of array to create.")
                        .ret(array_int.clone())
                        .definition(
                            r#"
Returns an `Array[Int]` of integers from `0` up to (but not including) the given `Int` `n`. If `n` is less than or equal to `0`, an empty array is returned.

**Parameters**

1. `Int`: The upper bound (exclusive) of the range.

**Returns**: An `Array[Int]` of integers.

Example: range_task.wdl

```wdl
version 1.2

task range {
  input {
    Int n = 5
  }

  output {
    Array[Int] r = range(n) # [0, 1, 2, 3, 4]
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#transpose
    assert!(
        functions
            .insert(
                "transpose",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .any_type_parameter("X")
                        .parameter(
                            "array",
                            GenericArrayType::new(GenericArrayType::new(
                                GenericType::Parameter("X"),
                            )),
                            "A M*N two-dimensional array.",
                        )
                        .ret(GenericArrayType::new(GenericArrayType::new(
                            GenericType::Parameter("X"),
                        )))
                        .definition(
                            r#"
Given an `Array[Array[X]]` `a`, returns a new `Array[Array[X]]` where the rows and columns of `a` are swapped. If `a` is empty, an empty array is returned.

If the inner arrays are not all the same length, an error is raised.

**Parameters**

1. `Array[Array[X]]`: The array to transpose.

**Returns**: A new `Array[Array[X]]` with the rows and columns swapped.

Example: transpose_task.wdl

```wdl
version 1.2

task transpose {
  input {
    Array[Array[Int]] matrix = [[1, 2, 3], [4, 5, 6]]
  }

  output {
    Array[Array[Int]] transposed_matrix = transpose(matrix) # [[1, 4], [2, 5], [3, 6]]
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#cross
    assert!(
        functions
            .insert(
                "cross",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .any_type_parameter("X")
                        .any_type_parameter("Y")
                        .parameter("a", GenericArrayType::new(GenericType::Parameter("X")), "The first array of length M.")
                        .parameter("b", GenericArrayType::new(GenericType::Parameter("Y")), "The second array of length N.")
                        .ret(GenericArrayType::new(GenericPairType::new(
                            GenericType::Parameter("X"),
                            GenericType::Parameter("Y"),
                        )))
                        .definition(
                            r#"
Given two `Array`s `a` and `b`, returns a new `Array[Pair[X, Y]]` where each element is a `Pair` of an element from `a` and an element from `b`. The order of the elements in the returned array is such that all elements from `b` are paired with the first element of `a`, then all elements from `b` are paired with the second element of `a`, and so on.

If either `a` or `b` is empty, an empty array is returned.

**Parameters**

1. `Array[X]`: The first array.
2. `Array[Y]`: The second array.

**Returns**: A new `Array[Pair[X, Y]]` with the cross product of the two arrays.

Example: cross_task.wdl

```wdl
version 1.2

task cross {
  input {
    Array[Int] ints = [1, 2]
    Array[String] strings = ["a", "b"]
  }

  output {
    Array[Pair[Int, String]] crossed = cross(ints, strings) # [(1, "a"), (1, "b"), (2, "a"), (2, "b")]
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#zip
    assert!(
        functions
            .insert(
                "zip",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .any_type_parameter("X")
                        .any_type_parameter("Y")
                        .parameter("a", GenericArrayType::new(GenericType::Parameter("X")), "The first array of length M.")
                        .parameter("b", GenericArrayType::new(GenericType::Parameter("Y")), "The second array of length N.")
                        .ret(GenericArrayType::new(GenericPairType::new(
                            GenericType::Parameter("X"),
                            GenericType::Parameter("Y"),
                        )))
                        .definition(
                            r#"
Given two `Array`s `a` and `b`, returns a new `Array[Pair[X, Y]]` where each element is a `Pair` of an element from `a` and an element from `b` at the same index. The length of the returned array is the minimum of the lengths of `a` and `b`.

If either `a` or `b` is empty, an empty array is returned.

**Parameters**

1. `Array[X]`: The first array.
2. `Array[Y]`: The second array.

**Returns**: A new `Array[Pair[X, Y]]` with the zipped elements.

Example: zip_task.wdl

```wdl
version 1.2

task zip {
  input {
    Array[Int] ints = [1, 2, 3]
    Array[String] strings = ["a", "b"]
  }

  output {
    Array[Pair[Int, String]] zipped = zip(ints, strings) # [(1, "a"), (2, "b")]
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#unzip
    assert!(
        functions
            .insert(
                "unzip",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .any_type_parameter("X")
                        .any_type_parameter("Y")
                        .parameter(
                            "array",
                            GenericArrayType::new(GenericPairType::new(
                                GenericType::Parameter("X"),
                                GenericType::Parameter("Y"),
                            )),
                            "The `Array` of `Pairs` of length N to unzip.",
                        )
                        .ret(GenericPairType::new(
                            GenericArrayType::new(GenericType::Parameter("X")),
                            GenericArrayType::new(GenericType::Parameter("Y")),
                        ))
                        .definition(
                            r#"
Given an `Array[Pair[X, Y]]` `a`, returns a new `Pair[Array[X], Array[Y]]` where the first element of the `Pair` is an `Array` of all the first elements of the `Pair`s in `a`, and the second element of the `Pair` is an `Array` of all the second elements of the `Pair`s in `a`.

If `a` is empty, a `Pair` of two empty arrays is returned.

**Parameters**

1. `Array[Pair[X, Y]]`: The array of pairs to unzip.

**Returns**: A new `Pair[Array[X], Array[Y]]` with the unzipped elements.

Example: unzip_task.wdl

```wdl
version 1.2

task unzip {
  input {
    Array[Pair[Int, String]] zipped = [(1, "a"), (2, "b")]
  }

  output {
    Pair[Array[Int], Array[String]] unzipped = unzip(zipped) # ([1, 2], ["a", "b"])
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#-contains
    assert!(
        functions
            .insert(
                "contains",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .type_parameter("P", PrimitiveTypeConstraint)
                        .parameter(
                            "array",
                            GenericArrayType::new(GenericType::Parameter("P")),
                            "An array of any primitive type.",
                        )
                        .parameter(
                            "value",
                            GenericType::Parameter("P"),
                            "A primitive value of the same type as the array. If the array's \
                             type is optional, then the value may also be optional.",
                        )
                        .ret(PrimitiveType::Boolean)
                        .definition(
                            r#"
Given an `Array[X]` `a` and a value `v` of type `X`, returns `true` if `v` is present in `a`, otherwise `false`.

**Parameters**

1. `Array[X]`: The array to search.
2. `X`: The value to search for.

**Returns**: `true` if `v` is present in `a`, otherwise `false`.

Example: contains_task.wdl

```wdl
version 1.2

task contains {
  input {
    Array[Int] ints = [1, 2, 3]
  }

  output {
    Boolean contains_2 = contains(ints, 2) # true
    Boolean contains_4 = contains(ints, 4) # false
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#-chunk
    assert!(
        functions
            .insert(
                "chunk",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .any_type_parameter("X")
                        .parameter(
                            "array",
                            GenericArrayType::new(GenericType::Parameter("X")),
                            "The array to split. May be empty.",
                        )
                        .parameter("size", PrimitiveType::Integer, "The desired length of the sub-arrays. Must be > 0.")
                        .ret(GenericArrayType::new(GenericArrayType::new(
                            GenericType::Parameter("X"),
                        )))
                        .definition(
                            r#"
Given an `Array[X]` `a` and an `Int` `size`, returns a new `Array[Array[X]]` where each inner array has at most `size` elements. The last inner array may have fewer than `size` elements. If `a` is empty, an empty array is returned.

If `size` is less than or equal to `0`, an error is raised.

**Parameters**

1. `Array[X]`: The array to chunk.
2. `Int`: The maximum size of each chunk.

**Returns**: A new `Array[Array[X]]` with the chunked elements.

Example: chunk_task.wdl

```wdl
version 1.2

task chunk {
  input {
    Array[Int] ints = [1, 2, 3, 4, 5]
  }

  output {
    Array[Array[Int]] chunked = chunk(ints, 2) # [[1, 2], [3, 4], [5]]
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#flatten
    assert!(
        functions
            .insert(
                "flatten",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .any_type_parameter("X")
                        .parameter(
                            "array",
                            GenericArrayType::new(GenericArrayType::new(
                                GenericType::Parameter("X"),
                            )),
                            "A nested array to flatten.",
                        )
                        .ret(GenericArrayType::new(GenericType::Parameter("X")))
                        .definition(
                            r#"
Given an `Array[Array[X]]` `a`, returns a new `Array[X]` where all the elements of the inner arrays are concatenated into a single array. If `a` is empty, an empty array is returned.

**Parameters**

1. `Array[Array[X]]`: The array to flatten.

**Returns**: A new `Array[X]` with the flattened elements.

Example: flatten_task.wdl

```wdl
version 1.2

task flatten {
  input {
    Array[Array[Int]] nested_ints = [[1, 2], [3, 4], [5]]
  }

  output {
    Array[Int] flattened_ints = flatten(nested_ints) # [1, 2, 3, 4, 5]
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#select_first
    assert!(
        functions
            .insert(
                "select_first",
                // This differs from the definition of `select_first` in that we can have a single
                // signature of `X select_first(Array[X?], [X])`.
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .any_type_parameter("X")
                        .required(1)
                        .parameter(
                            "array",
                            GenericArrayType::new(GenericType::Parameter("X")),
                            "Non-empty `Array` of optional values.",
                        )
                        .parameter("default", GenericType::UnqualifiedParameter("X"), "(Optional) The default value.")
                        .ret(GenericType::UnqualifiedParameter("X"))
                        .definition(
                            r#"
Given an `Array[X?]` `a`, returns the first non-`None` element in `a`. If all elements are `None`, an error is raised.

**Parameters**

1. `Array[X?]`: The array to search.

**Returns**: The first non-`None` element.

Example: select_first_task.wdl

```wdl
version 1.2

task select_first {
  input {
    Array[Int?] ints = [None, 1, None, 2]
  }

  output {
    Int first_int = select_first(ints) # 1
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#select_all
    assert!(
        functions
            .insert(
                "select_all",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .any_type_parameter("X")
                        .parameter(
                            "array",
                            GenericArrayType::new(GenericType::Parameter("X")),
                            "`Array` of optional values.",
                        )
                        .ret(GenericArrayType::new(GenericType::UnqualifiedParameter(
                            "X"
                        )))
                        .definition(
                            r#"
Given an `Array[X?]` `a`, returns a new `Array[X]` containing all the non-`None` elements in `a`. If all elements are `None`, an empty array is returned.

**Parameters**

1. `Array[X?]`: The array to filter.

**Returns**: A new `Array[X]` with all the non-`None` elements.

Example: select_all_task.wdl

```wdl
version 1.2

task select_all {
  input {
    Array[Int?] ints = [None, 1, None, 2]
  }

  output {
    Array[Int] all_ints = select_all(ints) # [1, 2]
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#as_pairs
    assert!(
        functions
            .insert(
                "as_pairs",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .type_parameter("K", PrimitiveTypeConstraint)
                        .any_type_parameter("V")
                        .parameter(
                            "map",
                            GenericMapType::new(
                                GenericType::Parameter("K"),
                                GenericType::Parameter("V"),
                            ),
                            "`Map` to convert to `Pairs`.",
                        )
                        .ret(GenericArrayType::new(GenericPairType::new(
                            GenericType::Parameter("K"),
                            GenericType::Parameter("V")
                        )))
                        .definition(
                            r#"
Given a `Map[K, V]` `m`, returns a new `Array[Pair[K, V]]` where each element is a `Pair` of a key and its corresponding value from `m`. The order of the elements in the returned array is the same as the order in which the elements were added to the `Map`.

If `m` is empty, an empty array is returned.

**Parameters**

1. `Map[K, V]`: The map to convert.

**Returns**: A new `Array[Pair[K, V]]` with the key-value pairs.

Example: as_pairs_task.wdl

```wdl
version 1.2

task as_pairs {
  input {
    Map[String, Int] map = {"a": 1, "b": 2}
  }

  output {
    Array[Pair[String, Int]] pairs = as_pairs(map) # [("a", 1), ("b", 2)]
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#as_map
    assert!(
        functions
            .insert(
                "as_map",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .type_parameter("K", PrimitiveTypeConstraint)
                        .any_type_parameter("V")
                        .parameter(
                            "pairs",
                            GenericArrayType::new(GenericPairType::new(
                                GenericType::Parameter("K"),
                                GenericType::Parameter("V"),
                            )),
                            "`Array` of `Pairs` to convert to a `Map`.",
                        )
                        .ret(GenericMapType::new(
                            GenericType::Parameter("K"),
                            GenericType::Parameter("V")
                        ))
                        .definition(
                            r#"
Given an `Array[Pair[K, V]]` `a`, returns a new `Map[K, V]` where each `Pair` is converted to a key-value pair in the `Map`. If `a` is empty, an empty map is returned.

If there are any duplicate keys in `a`, an error is raised.

**Parameters**

1. `Array[Pair[K, V]]`: The array of pairs to convert.

**Returns**: A new `Map[K, V]` with the key-value pairs.

Example: as_map_task.wdl

```wdl
version 1.2

task as_map {
  input {
    Array[Pair[String, Int]] pairs = [("a", 1), ("b", 2)]
  }

  output {
    Map[String, Int] map = as_map(pairs) # {"a": 1, "b": 2}
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    const KEYS_DEFINITION: &str = r#"
Given a `Map[K, V]` `m`, returns a new `Array[K]` containing all the keys in `m`. The order of the keys in the returned array is the same as the order in which the elements were added to the `Map`.

If `m` is empty, an empty array is returned.

**Parameters**

1. `Map[K, V]`: The map to get the keys from.

**Returns**: A new `Array[K]` with the keys.

Example: keys_map_task.wdl

```wdl
version 1.2

task keys_map {
  input {
    Map[String, Int] map = {"a": 1, "b": 2}
  }

  output {
    Array[String] keys = keys(map) # ["a", "b"]
  }
}
```
"#;

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#keys
    assert!(
        functions
            .insert(
                "keys",
                PolymorphicFunction::new(vec![
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .type_parameter("K", PrimitiveTypeConstraint)
                        .any_type_parameter("V")
                        .parameter(
                            "map",
                            GenericMapType::new(
                                GenericType::Parameter("K"),
                                GenericType::Parameter("V"),
                            ),
                            "Collection from which to extract keys.",
                        )
                        .ret(GenericArrayType::new(GenericType::Parameter("K")))
                        .definition(KEYS_DEFINITION)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .type_parameter("S", StructConstraint)
                        .parameter(
                            "struct",
                            GenericType::Parameter("S"),
                            "Collection from which to extract keys.",
                        )
                        .ret(array_string.clone())
                        .definition(KEYS_DEFINITION)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .parameter(
                            "object",
                            Type::Object,
                            "Collection from which to extract keys.",
                        )
                        .ret(array_string.clone())
                        .definition(KEYS_DEFINITION)
                        .build(),
                ])
                .into(),
            )
            .is_none()
    );

    const CONTAINS_KEY_DEFINITION: &str = r#"
Given a `Map[K, V]` `m` and a key `k` of type `K`, returns `true` if `k` is present in `m`, otherwise `false`.

**Parameters**

1. `Map[K, V]`: The map to search.
2. `K`: The key to search for.

**Returns**: `true` if `k` is present in `m`, otherwise `false`.

Example: contains_key_map_task.wdl

```wdl
version 1.2

task contains_key_map {
  input {
    Map[String, Int] map = {"a": 1, "b": 2}
  }

  output {
    Boolean contains_a = contains_key(map, "a") # true
    Boolean contains_c = contains_key(map, "c") # false
  }
}
```
"#;

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#contains_key
    assert!(
        functions
            .insert(
                "contains_key",
                PolymorphicFunction::new(vec![
                        FunctionSignature::builder()
                            .min_version(SupportedVersion::V1(V1::Two))
                            .type_parameter("K", PrimitiveTypeConstraint)
                            .any_type_parameter("V")
                            .parameter(
                                "map",
                                GenericMapType::new(
                                    GenericType::Parameter("K"),
                                    GenericType::Parameter("V"),
                                ),
                                "Collection to search for the key.",
                            )
                            .parameter(
                                "key",
                                GenericType::Parameter("K"),
                                "The key to search for. If the first argument is a `Map`, then \
                                 the key must be of the same type as the `Map`'s key type. If the \
                                 `Map`'s key type is optional then the key may also be optional. \
                                 If the first argument is a `Map[String, Y]`, `Struct`, or \
                                 `Object`, then the key may be either a `String` or \
                                 `Array[String]`."
                            )
                            .ret(PrimitiveType::Boolean)
                            .definition(CONTAINS_KEY_DEFINITION)
                            .build(),
                        FunctionSignature::builder()
                            .min_version(SupportedVersion::V1(V1::Two))
                            .parameter("object", Type::Object, "Collection to search for the key.")
                            .parameter(
                                "key",
                                PrimitiveType::String,
                                "The key to search for. If the first argument is a `Map`, then \
                                 the key must be of the same type as the `Map`'s key type. If the \
                                 `Map`'s key type is optional then the key may also be optional. \
                                 If the first argument is a `Map[String, Y]`, `Struct`, or \
                                 `Object`, then the key may be either a `String` or \
                                 `Array[String]`."
                            )
                            .ret(PrimitiveType::Boolean)
                            .definition(CONTAINS_KEY_DEFINITION)
                            .build(),
                        FunctionSignature::builder()
                            .min_version(SupportedVersion::V1(V1::Two))
                            .any_type_parameter("V")
                            .parameter(
                                "map",
                                GenericMapType::new(
                                    PrimitiveType::String,
                                    GenericType::Parameter("V"),
                                ),
                                "Collection to search for the key.",
                            )
                            .parameter(
                                "keys",
                                array_string.clone(),
                                "The key to search for. If the first argument is a `Map`, then \
                                 the key must be of the same type as the `Map`'s key type. If the \
                                 `Map`'s key type is optional then the key may also be optional. \
                                 If the first argument is a `Map[String, Y]`, `Struct`, or \
                                 `Object`, then the key may be either a `String` or \
                                 `Array[String]`."
                            )
                            .ret(PrimitiveType::Boolean)
                            .definition(CONTAINS_KEY_DEFINITION)
                            .build(),
                        FunctionSignature::builder()
                            .min_version(SupportedVersion::V1(V1::Two))
                            .type_parameter("S", StructConstraint)
                            .parameter(
                                "struct",
                                GenericType::Parameter("S"),
                                "Collection to search for the key.",
                            )
                            .parameter(
                                "keys",
                                array_string.clone(),
                                "The key to search for. If the first argument is a `Map`, then \
                                 the key must be of the same type as the `Map`'s key type. If the \
                                 `Map`'s key type is optional then the key may also be optional. \
                                 If the first argument is a `Map[String, Y]`, `Struct`, or \
                                 `Object`, then the key may be either a `String` or \
                                 `Array[String]`."
                            )
                            .ret(PrimitiveType::Boolean)
                            .definition(CONTAINS_KEY_DEFINITION)
                            .build(),
                        FunctionSignature::builder()
                            .min_version(SupportedVersion::V1(V1::Two))
                            .parameter("object", Type::Object, "Collection to search for the key.")
                            .parameter(
                                "keys",
                                array_string.clone(),
                                "The key to search for. If the first argument is a `Map`, then \
                                 the key must be of the same type as the `Map`'s key type. If the \
                                 `Map`'s key type is optional then the key may also be optional. \
                                 If the first argument is a `Map[String, Y]`, `Struct`, or \
                                 `Object`, then the key may be either a `String` or \
                                 `Array[String]`."
                            )
                            .ret(PrimitiveType::Boolean)
                            .definition(CONTAINS_KEY_DEFINITION)
                            .build(),
                    ])
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#-values
    assert!(
        functions
            .insert(
                "values",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .type_parameter("K", PrimitiveTypeConstraint)
                        .any_type_parameter("V")
                        .parameter(
                            "map",
                            GenericMapType::new(
                                GenericType::Parameter("K"),
                                GenericType::Parameter("V"),
                            ),
                            "`Map` from which to extract values.",
                        )
                        .ret(GenericArrayType::new(GenericType::Parameter("V")))
                        .definition(
                            r#"
Given a `Map[K, V]` `m`, returns a new `Array[V]` containing all the values in `m`. The order of the values in the returned array is the same as the order in which the elements were added to the `Map`.

If `m` is empty, an empty array is returned.

**Parameters**

1. `Map[K, V]`: The map to get the values from.

**Returns**: A new `Array[V]` with the values.

Example: values_map_task.wdl

```wdl
version 1.2

task values_map {
  input {
    Map[String, Int] map = {"a": 1, "b": 2}
  }

  output {
    Array[Int] values = values(map) # [1, 2]
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#collect_by_key
    assert!(
        functions
            .insert(
                "collect_by_key",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .type_parameter("K", PrimitiveTypeConstraint)
                        .any_type_parameter("V")
                        .parameter(
                            "pairs",
                            GenericArrayType::new(GenericPairType::new(
                                GenericType::Parameter("K"),
                                GenericType::Parameter("V"),
                            )),
                            "`Array` of `Pairs` to group.",
                        )
                        .ret(GenericMapType::new(
                            GenericType::Parameter("K"),
                            GenericArrayType::new(GenericType::Parameter("V"))
                        ))
                        .definition(
                            r#"
Given an `Array[Pair[K, V]]` `a`, returns a new `Map[K, Array[V]]` where each key `K` maps to an `Array` of all the values `V` that were paired with `K` in `a`. The order of the values in the inner arrays is the same as the order in which they appeared in `a`.

If `a` is empty, an empty map is returned.

**Parameters**

1. `Array[Pair[K, V]]`: The array of pairs to collect.

**Returns**: A new `Map[K, Array[V]]` with the collected values.

Example: collect_by_key_task.wdl

```wdl
version 1.2

task collect_by_key {
  input {
    Array[Pair[String, Int]] pairs = [("a", 1), ("b", 2), ("a", 3)]
  }

  output {
    Map[String, Array[Int]] collected = collect_by_key(pairs) # {"a": [1, 3], "b": 2}
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // Enum functions (WDL 1.3)
    assert!(
        functions
            .insert(
                "value",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Three))
                        .type_parameter("V", EnumVariantConstraint)
                        .parameter(
                            "variant",
                            GenericType::Parameter("V"),
                            "An enum variant of any enum type.",
                        )
                        .ret(GenericEnumValueType::new("V"))
                        .definition(
                            r##"
Returns the underlying value associated with an enum variant.

**Parameters**

1. `Enum`: an enum variant of any enum type.

**Returns**: The variant's associated value.

Example: test_enum_value.wdl

```wdl
version 1.3

enum Color {
  Red = "#FF0000",
  Green = "#00FF00",
  Blue = "#0000FF"
}

workflow test_enum_value {
  input {
    Color color = Color.Red
  }

  output {
    String variant_value = value(color)   # "#FF0000"
    String implicit = "~{color}"          # "Red" (default to name)
  }
}
```
"##
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#defined
    assert!(
        functions
            .insert(
                "defined",
                MonomorphicFunction::new(
                    FunctionSignature::builder()
                        .any_type_parameter("X")
                        .parameter(
                            "value",
                            GenericType::Parameter("X"),
                            "Optional value of any type."
                        )
                        .ret(PrimitiveType::Boolean)
                        .definition(
                            r#"
Given an optional value `x`, returns `true` if `x` is defined (i.e., not `None`), otherwise `false`.

**Parameters**

1. `X?`: The optional value to check.

**Returns**: `true` if `x` is defined, otherwise `false`.

Example: defined_task.wdl

```wdl
version 1.2

task defined {
  input {
    Int? x = 1
    Int? y = None
  }

  output {
    Boolean x_defined = defined(x) # true
    Boolean y_defined = defined(y) # false
  }
}
```
"#
                        )
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    const LENGTH_DEFINITION: &str = r#"
Given an `Array[X]` `a`, returns the number of elements in `a`. If `a` is empty, `0` is returned.

**Parameters**

1. `Array[X]`: The array to get the length from.

**Returns**: The number of elements in the array as an `Int`.

Example: length_array_task.wdl

```wdl
version 1.2

task length_array {
  input {
    Array[Int] ints = [1, 2, 3]
  }

  output {
    Int len = length(ints) # 3
  }
}
```
"#;

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#length
    assert!(
        functions
            .insert(
                "length",
                PolymorphicFunction::new(vec![
                    FunctionSignature::builder()
                        .any_type_parameter("X")
                        .parameter(
                            "array",
                            GenericArrayType::new(GenericType::Parameter("X")),
                            "A collection or string whose elements are to be counted.",
                        )
                        .ret(PrimitiveType::Integer)
                        .definition(LENGTH_DEFINITION)
                        .build(),
                    FunctionSignature::builder()
                        .any_type_parameter("K")
                        .any_type_parameter("V")
                        .parameter(
                            "map",
                            GenericMapType::new(
                                GenericType::Parameter("K"),
                                GenericType::Parameter("V"),
                            ),
                            "A collection or string whose elements are to be counted.",
                        )
                        .ret(PrimitiveType::Integer)
                        .definition(LENGTH_DEFINITION)
                        .build(),
                    FunctionSignature::builder()
                        .parameter(
                            "object",
                            Type::Object,
                            "A collection or string whose elements are to be counted.",
                        )
                        .ret(PrimitiveType::Integer)
                        .definition(LENGTH_DEFINITION)
                        .build(),
                    FunctionSignature::builder()
                        .parameter(
                            "string",
                            PrimitiveType::String,
                            "A collection or string whose elements are to be counted.",
                        )
                        .ret(PrimitiveType::Integer)
                        .definition(LENGTH_DEFINITION)
                        .build(),
                ])
                .into(),
            )
            .is_none()
    );

    StandardLibrary {
        functions,
        array_int,
        array_string,
        array_file,
        array_object,
        array_string_non_empty,
        array_array_string,
        map_string_string,
        map_string_int,
    }
});

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn verify_stdlib_signatures() {
        let mut signatures = Vec::new();
        for (name, f) in STDLIB.functions() {
            match f {
                Function::Monomorphic(f) => {
                    let params = TypeParameters::new(&f.signature.type_parameters);
                    signatures.push(format!("{name}{sig}", sig = f.signature.display(&params)));
                }
                Function::Polymorphic(f) => {
                    for signature in &f.signatures {
                        let params = TypeParameters::new(&signature.type_parameters);
                        signatures.push(format!("{name}{sig}", sig = signature.display(&params)));
                    }
                }
            }
        }

        assert_eq!(
            signatures,
            [
                "floor(value: Float) -> Int",
                "ceil(value: Float) -> Int",
                "round(value: Float) -> Int",
                "min(a: Int, b: Int) -> Int",
                "min(a: Int, b: Float) -> Float",
                "min(a: Float, b: Int) -> Float",
                "min(a: Float, b: Float) -> Float",
                "max(a: Int, b: Int) -> Int",
                "max(a: Int, b: Float) -> Float",
                "max(a: Float, b: Int) -> Float",
                "max(a: Float, b: Float) -> Float",
                "find(input: String, pattern: String) -> String?",
                "matches(input: String, pattern: String) -> Boolean",
                "sub(input: String, pattern: String, replace: String) -> String",
                "split(input: String, delimiter: String) -> Array[String]",
                "basename(path: File, <suffix: String>) -> String",
                "basename(path: String, <suffix: String>) -> String",
                "basename(path: Directory, <suffix: String>) -> String",
                "join_paths(base: File, relative: String) -> File",
                "join_paths(base: File, relative: Array[String]+) -> File",
                "join_paths(paths: Array[String]+) -> File",
                "glob(pattern: String) -> Array[File]",
                "size(value: None, <unit: String>) -> Float",
                "size(value: File?, <unit: String>) -> Float",
                "size(value: String?, <unit: String>) -> Float",
                "size(value: Directory?, <unit: String>) -> Float",
                "size(value: X, <unit: String>) -> Float where `X`: any compound type that \
                 recursively contains a `File` or `Directory`",
                "stdout() -> File",
                "stderr() -> File",
                "read_string(file: File) -> String",
                "read_int(file: File) -> Int",
                "read_float(file: File) -> Float",
                "read_boolean(file: File) -> Boolean",
                "read_lines(file: File) -> Array[String]",
                "write_lines(array: Array[String]) -> File",
                "read_tsv(file: File) -> Array[Array[String]]",
                "read_tsv(file: File, header: Boolean) -> Array[Object]",
                "read_tsv(file: File, header: Boolean, columns: Array[String]) -> Array[Object]",
                "write_tsv(data: Array[Array[String]]) -> File",
                "write_tsv(data: Array[Array[String]], header: Boolean, columns: Array[String]) \
                 -> File",
                "write_tsv(data: Array[S], <header: Boolean>, <columns: Array[String]>) -> File \
                 where `S`: any structure containing only primitive types",
                "read_map(file: File) -> Map[String, String]",
                "write_map(map: Map[String, String]) -> File",
                "read_json(file: File) -> Union",
                "write_json(value: X) -> File where `X`: any JSON-serializable type",
                "read_object(file: File) -> Object",
                "read_objects(file: File) -> Array[Object]",
                "write_object(object: Object) -> File",
                "write_object(object: S) -> File where `S`: any structure containing only \
                 primitive types",
                "write_objects(objects: Array[Object]) -> File",
                "write_objects(objects: Array[S]) -> File where `S`: any structure containing \
                 only primitive types",
                "prefix(prefix: String, array: Array[P]) -> Array[String] where `P`: any \
                 primitive type",
                "suffix(suffix: String, array: Array[P]) -> Array[String] where `P`: any \
                 primitive type",
                "quote(array: Array[P]) -> Array[String] where `P`: any primitive type",
                "squote(array: Array[P]) -> Array[String] where `P`: any primitive type",
                "sep(separator: String, array: Array[P]) -> String where `P`: any primitive type",
                "range(n: Int) -> Array[Int]",
                "transpose(array: Array[Array[X]]) -> Array[Array[X]]",
                "cross(a: Array[X], b: Array[Y]) -> Array[Pair[X, Y]]",
                "zip(a: Array[X], b: Array[Y]) -> Array[Pair[X, Y]]",
                "unzip(array: Array[Pair[X, Y]]) -> Pair[Array[X], Array[Y]]",
                "contains(array: Array[P], value: P) -> Boolean where `P`: any primitive type",
                "chunk(array: Array[X], size: Int) -> Array[Array[X]]",
                "flatten(array: Array[Array[X]]) -> Array[X]",
                "select_first(array: Array[X], <default: X>) -> X",
                "select_all(array: Array[X]) -> Array[X]",
                "as_pairs(map: Map[K, V]) -> Array[Pair[K, V]] where `K`: any primitive type",
                "as_map(pairs: Array[Pair[K, V]]) -> Map[K, V] where `K`: any primitive type",
                "keys(map: Map[K, V]) -> Array[K] where `K`: any primitive type",
                "keys(struct: S) -> Array[String] where `S`: any structure",
                "keys(object: Object) -> Array[String]",
                "contains_key(map: Map[K, V], key: K) -> Boolean where `K`: any primitive type",
                "contains_key(object: Object, key: String) -> Boolean",
                "contains_key(map: Map[String, V], keys: Array[String]) -> Boolean",
                "contains_key(struct: S, keys: Array[String]) -> Boolean where `S`: any structure",
                "contains_key(object: Object, keys: Array[String]) -> Boolean",
                "values(map: Map[K, V]) -> Array[V] where `K`: any primitive type",
                "collect_by_key(pairs: Array[Pair[K, V]]) -> Map[K, Array[V]] where `K`: any \
                 primitive type",
                "value(variant: V) -> T where `V`: any enumeration variant",
                "defined(value: X) -> Boolean",
                "length(array: Array[X]) -> Int",
                "length(map: Map[K, V]) -> Int",
                "length(object: Object) -> Int",
                "length(string: String) -> Int",
            ]
        );
    }

    #[test]
    fn it_binds_a_simple_function() {
        let f = STDLIB.function("floor").expect("should have function");
        assert_eq!(f.minimum_version(), SupportedVersion::V1(V1::Zero));

        let e = f
            .bind(SupportedVersion::V1(V1::Zero), &[])
            .expect_err("bind should fail");
        assert_eq!(e, FunctionBindError::TooFewArguments(1));

        let e = f
            .bind(
                SupportedVersion::V1(V1::One),
                &[PrimitiveType::String.into(), PrimitiveType::Boolean.into()],
            )
            .expect_err("bind should fail");
        assert_eq!(e, FunctionBindError::TooManyArguments(1));

        // Check for a string argument (should be a type mismatch)
        let e = f
            .bind(
                SupportedVersion::V1(V1::Two),
                &[PrimitiveType::String.into()],
            )
            .expect_err("bind should fail");
        assert_eq!(
            e,
            FunctionBindError::ArgumentTypeMismatch {
                index: 0,
                expected: "`Float`".into()
            }
        );

        // Check for Union (i.e. indeterminate)
        let binding = f
            .bind(SupportedVersion::V1(V1::Zero), &[Type::Union])
            .expect("bind should succeed");
        assert_eq!(binding.index(), 0);
        assert_eq!(binding.return_type().to_string(), "Int");

        // Check for a float argument
        let binding = f
            .bind(
                SupportedVersion::V1(V1::One),
                &[PrimitiveType::Float.into()],
            )
            .expect("bind should succeed");
        assert_eq!(binding.index(), 0);
        assert_eq!(binding.return_type().to_string(), "Int");

        // Check for an integer argument (should coerce)
        let binding = f
            .bind(
                SupportedVersion::V1(V1::Two),
                &[PrimitiveType::Integer.into()],
            )
            .expect("bind should succeed");
        assert_eq!(binding.index(), 0);
        assert_eq!(binding.return_type().to_string(), "Int");
    }

    #[test]
    fn it_binds_a_generic_function() {
        let f = STDLIB.function("values").expect("should have function");
        assert_eq!(f.minimum_version(), SupportedVersion::V1(V1::Two));

        let e = f
            .bind(SupportedVersion::V1(V1::Zero), &[])
            .expect_err("bind should fail");
        assert_eq!(
            e,
            FunctionBindError::RequiresVersion(SupportedVersion::V1(V1::Two))
        );

        let e = f
            .bind(SupportedVersion::V1(V1::Two), &[])
            .expect_err("bind should fail");
        assert_eq!(e, FunctionBindError::TooFewArguments(1));

        let e = f
            .bind(
                SupportedVersion::V1(V1::Two),
                &[PrimitiveType::String.into(), PrimitiveType::Boolean.into()],
            )
            .expect_err("bind should fail");
        assert_eq!(e, FunctionBindError::TooManyArguments(1));

        // Check for a string argument (should be a type mismatch)
        let e = f
            .bind(
                SupportedVersion::V1(V1::Two),
                &[PrimitiveType::String.into()],
            )
            .expect_err("bind should fail");
        assert_eq!(
            e,
            FunctionBindError::ArgumentTypeMismatch {
                index: 0,
                expected: "`Map[K, V]` where `K`: any primitive type".into()
            }
        );

        // Check for Union (i.e. indeterminate)
        let binding = f
            .bind(SupportedVersion::V1(V1::Two), &[Type::Union])
            .expect("bind should succeed");
        assert_eq!(binding.index(), 0);
        assert_eq!(binding.return_type().to_string(), "Array[Union]");

        // Check for a Map[String, String]
        let ty: Type = MapType::new(PrimitiveType::String, PrimitiveType::String).into();
        let binding = f
            .bind(SupportedVersion::V1(V1::Two), &[ty])
            .expect("bind should succeed");
        assert_eq!(binding.index(), 0);
        assert_eq!(binding.return_type().to_string(), "Array[String]");

        // Check for a Map[String, Object]
        let ty: Type = MapType::new(PrimitiveType::String, Type::Object).into();
        let binding = f
            .bind(SupportedVersion::V1(V1::Two), &[ty])
            .expect("bind should succeed");
        assert_eq!(binding.index(), 0);
        assert_eq!(binding.return_type().to_string(), "Array[Object]");

        // Check for a map with an optional primitive type
        let ty: Type = MapType::new(
            Type::from(PrimitiveType::String).optional(),
            PrimitiveType::Boolean,
        )
        .into();
        let binding = f
            .bind(SupportedVersion::V1(V1::Two), &[ty])
            .expect("bind should succeed");
        assert_eq!(binding.index(), 0);
        assert_eq!(binding.return_type().to_string(), "Array[Boolean]");
    }

    #[test]
    fn it_removes_qualifiers() {
        let f = STDLIB.function("select_all").expect("should have function");
        assert_eq!(f.minimum_version(), SupportedVersion::V1(V1::Zero));

        // Check for a Array[String]
        let array_string: Type = ArrayType::new(PrimitiveType::String).into();
        let binding = f
            .bind(SupportedVersion::V1(V1::One), &[array_string])
            .expect("bind should succeed");
        assert_eq!(binding.index(), 0);
        assert_eq!(binding.return_type().to_string(), "Array[String]");

        // Check for a Array[String?] -> Array[String]
        let array_optional_string: Type =
            ArrayType::new(Type::from(PrimitiveType::String).optional()).into();
        let binding = f
            .bind(SupportedVersion::V1(V1::One), &[array_optional_string])
            .expect("bind should succeed");
        assert_eq!(binding.index(), 0);
        assert_eq!(binding.return_type().to_string(), "Array[String]");

        // Check for Union (i.e. indeterminate)
        let binding = f
            .bind(SupportedVersion::V1(V1::Two), &[Type::Union])
            .expect("bind should succeed");
        assert_eq!(binding.index(), 0);
        assert_eq!(binding.return_type().to_string(), "Array[Union]");

        // Check for a Array[Array[String]?] -> Array[Array[String]]
        let array_string = Type::from(ArrayType::new(PrimitiveType::String)).optional();
        let array_array_string = ArrayType::new(array_string).into();
        let binding = f
            .bind(SupportedVersion::V1(V1::Zero), &[array_array_string])
            .expect("bind should succeed");
        assert_eq!(binding.index(), 0);
        assert_eq!(binding.return_type().to_string(), "Array[Array[String]]");
    }

    #[test]
    fn it_binds_concrete_overloads() {
        let f = STDLIB.function("max").expect("should have function");
        assert_eq!(f.minimum_version(), SupportedVersion::V1(V1::One));

        let e = f
            .bind(SupportedVersion::V1(V1::One), &[])
            .expect_err("bind should fail");
        assert_eq!(e, FunctionBindError::TooFewArguments(2));

        let e = f
            .bind(
                SupportedVersion::V1(V1::Two),
                &[
                    PrimitiveType::String.into(),
                    PrimitiveType::Boolean.into(),
                    PrimitiveType::File.into(),
                ],
            )
            .expect_err("bind should fail");
        assert_eq!(e, FunctionBindError::TooManyArguments(2));

        // Check for `(Int, Int)`
        let binding = f
            .bind(
                SupportedVersion::V1(V1::One),
                &[PrimitiveType::Integer.into(), PrimitiveType::Integer.into()],
            )
            .expect("binding should succeed");
        assert_eq!(binding.index(), 0);
        assert_eq!(binding.return_type().to_string(), "Int");

        // Check for `(Int, Float)`
        let binding = f
            .bind(
                SupportedVersion::V1(V1::Two),
                &[PrimitiveType::Integer.into(), PrimitiveType::Float.into()],
            )
            .expect("binding should succeed");
        assert_eq!(binding.index(), 1);
        assert_eq!(binding.return_type().to_string(), "Float");

        // Check for `(Float, Int)`
        let binding = f
            .bind(
                SupportedVersion::V1(V1::One),
                &[PrimitiveType::Float.into(), PrimitiveType::Integer.into()],
            )
            .expect("binding should succeed");
        assert_eq!(binding.index(), 2);
        assert_eq!(binding.return_type().to_string(), "Float");

        // Check for `(Float, Float)`
        let binding = f
            .bind(
                SupportedVersion::V1(V1::Two),
                &[PrimitiveType::Float.into(), PrimitiveType::Float.into()],
            )
            .expect("binding should succeed");
        assert_eq!(binding.index(), 3);
        assert_eq!(binding.return_type().to_string(), "Float");

        // Check for `(String, Int)`
        let e = f
            .bind(
                SupportedVersion::V1(V1::One),
                &[PrimitiveType::String.into(), PrimitiveType::Integer.into()],
            )
            .expect_err("binding should fail");
        assert_eq!(
            e,
            FunctionBindError::ArgumentTypeMismatch {
                index: 0,
                expected: "`Int` or `Float`".into()
            }
        );

        // Check for `(Int, String)`
        let e = f
            .bind(
                SupportedVersion::V1(V1::Two),
                &[PrimitiveType::Integer.into(), PrimitiveType::String.into()],
            )
            .expect_err("binding should fail");
        assert_eq!(
            e,
            FunctionBindError::ArgumentTypeMismatch {
                index: 1,
                expected: "`Int` or `Float`".into()
            }
        );

        // Check for `(String, Float)`
        let e = f
            .bind(
                SupportedVersion::V1(V1::One),
                &[PrimitiveType::String.into(), PrimitiveType::Float.into()],
            )
            .expect_err("binding should fail");
        assert_eq!(
            e,
            FunctionBindError::ArgumentTypeMismatch {
                index: 0,
                expected: "`Int` or `Float`".into()
            }
        );

        // Check for `(Float, String)`
        let e = f
            .bind(
                SupportedVersion::V1(V1::Two),
                &[PrimitiveType::Float.into(), PrimitiveType::String.into()],
            )
            .expect_err("binding should fail");
        assert_eq!(
            e,
            FunctionBindError::ArgumentTypeMismatch {
                index: 1,
                expected: "`Int` or `Float`".into()
            }
        );
    }

    #[test]
    fn it_binds_generic_overloads() {
        let f = STDLIB
            .function("select_first")
            .expect("should have function");
        assert_eq!(f.minimum_version(), SupportedVersion::V1(V1::Zero));

        let e = f
            .bind(SupportedVersion::V1(V1::Zero), &[])
            .expect_err("bind should fail");
        assert_eq!(e, FunctionBindError::TooFewArguments(1));

        let e = f
            .bind(
                SupportedVersion::V1(V1::One),
                &[
                    PrimitiveType::String.into(),
                    PrimitiveType::Boolean.into(),
                    PrimitiveType::File.into(),
                ],
            )
            .expect_err("bind should fail");
        assert_eq!(e, FunctionBindError::TooManyArguments(2));

        // Check `Int`
        let e = f
            .bind(
                SupportedVersion::V1(V1::Two),
                &[PrimitiveType::Integer.into()],
            )
            .expect_err("binding should fail");
        assert_eq!(
            e,
            FunctionBindError::ArgumentTypeMismatch {
                index: 0,
                expected: "`Array[X]`".into()
            }
        );

        // Check `Array[String?]+`
        let array: Type = ArrayType::non_empty(Type::from(PrimitiveType::String).optional()).into();
        let binding = f
            .bind(SupportedVersion::V1(V1::Zero), std::slice::from_ref(&array))
            .expect("binding should succeed");
        assert_eq!(binding.index(), 0);
        assert_eq!(binding.return_type().to_string(), "String");

        // Check (`Array[String?]+`, `String`)
        let binding = f
            .bind(
                SupportedVersion::V1(V1::One),
                &[array.clone(), PrimitiveType::String.into()],
            )
            .expect("binding should succeed");
        assert_eq!(binding.index(), 0);
        assert_eq!(binding.return_type().to_string(), "String");

        // Check (`Array[String?]+`, `Int`)
        let e = f
            .bind(
                SupportedVersion::V1(V1::Two),
                &[array.clone(), PrimitiveType::Integer.into()],
            )
            .expect_err("binding should fail");
        assert_eq!(
            e,
            FunctionBindError::ArgumentTypeMismatch {
                index: 1,
                expected: "`String`".into()
            }
        );

        // Check `Array[String?]`
        let array: Type = ArrayType::new(Type::from(PrimitiveType::String).optional()).into();
        let binding = f
            .bind(SupportedVersion::V1(V1::Zero), std::slice::from_ref(&array))
            .expect("binding should succeed");
        assert_eq!(binding.index(), 0);
        assert_eq!(binding.return_type().to_string(), "String");

        // Check (`Array[String?]`, `String`)
        let binding = f
            .bind(
                SupportedVersion::V1(V1::One),
                &[array.clone(), PrimitiveType::String.into()],
            )
            .expect("binding should succeed");
        assert_eq!(binding.index(), 0);
        assert_eq!(binding.return_type().to_string(), "String");

        // Check (`Array[String?]`, `Int`)
        let e = f
            .bind(
                SupportedVersion::V1(V1::Two),
                &[array, PrimitiveType::Integer.into()],
            )
            .expect_err("binding should fail");
        assert_eq!(
            e,
            FunctionBindError::ArgumentTypeMismatch {
                index: 1,
                expected: "`String`".into()
            }
        );
    }
}
