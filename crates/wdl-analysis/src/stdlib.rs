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

                if !ignore_constraints {
                    if let Some(constraint) = param.constraint() {
                        if !constraint.satisfied(ty) {
                            return;
                        }
                    }
                }

                params.set_inferred_type(name, ty.clone());
            }
            Self::Array(array) => array.infer_type_parameters(ty, params, ignore_constraints),
            Self::Pair(pair) => pair.infer_type_parameters(ty, params, ignore_constraints),
            Self::Map(map) => map.infer_type_parameters(ty, params, ignore_constraints),
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
            parameters.len() < MAX_TYPE_PARAMETERS,
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

/// Represents a WDL function signature.
#[derive(Debug)]
pub struct FunctionSignature {
    /// The minimum required version for the function signature.
    minimum_version: Option<SupportedVersion>,
    /// The generic type parameters of the function.
    type_parameters: Vec<TypeParameter>,
    /// The number of required parameters of the function.
    required: Option<usize>,
    /// The parameter types of the function.
    parameters: Vec<FunctionalType>,
    /// The return type of the function.
    ret: FunctionalType,
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

    /// Gets the types of the function's parameters.
    pub fn parameters(&self) -> &[FunctionalType] {
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

    /// Determines if the function signature is generic.
    pub fn is_generic(&self) -> bool {
        self.generic_parameter_count() > 0 || self.ret.is_generic()
    }

    /// Gets the count of generic parameters for the function.
    pub fn generic_parameter_count(&self) -> usize {
        self.parameters.iter().filter(|p| p.is_generic()).count()
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

                    write!(f, "{param}", param = parameter.display(self.params))?;

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
            parameter.infer_type_parameters(argument, &mut parameters, ignore_constraints);
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
            match parameter.realize(&type_parameters) {
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
                        param = parameter.display(&type_parameters)
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
    pub fn parameter(mut self, ty: impl Into<FunctionalType>) -> Self {
        self.0.parameters.push(ty.into());
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

    /// Consumes the builder and produces the function signature.
    ///
    /// # Panics
    ///
    /// This method panics if the function signature is invalid.
    pub fn build(self) -> FunctionSignature {
        let sig = self.0;

        // Ensure the number of required parameters doesn't exceed the number of
        // parameters
        if let Some(required) = sig.required {
            if required > sig.parameters.len() {
                panic!("number of required parameters exceeds the number of parameters");
            }
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
        for param in sig.parameters.iter() {
            param.assert_type_parameters(&sig.type_parameters)
        }

        sig.ret().assert_type_parameters(&sig.type_parameters);

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
                        .parameter(PrimitiveType::Float)
                        .ret(PrimitiveType::Integer)
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
                        .parameter(PrimitiveType::Float)
                        .ret(PrimitiveType::Integer)
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
                        .parameter(PrimitiveType::Float)
                        .ret(PrimitiveType::Integer)
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#min
    assert!(
        functions
            .insert(
                "min",
                PolymorphicFunction::new(vec![
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .parameter(PrimitiveType::Integer)
                        .parameter(PrimitiveType::Integer)
                        .ret(PrimitiveType::Integer)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .parameter(PrimitiveType::Integer)
                        .parameter(PrimitiveType::Float)
                        .ret(PrimitiveType::Float)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .parameter(PrimitiveType::Float)
                        .parameter(PrimitiveType::Integer)
                        .ret(PrimitiveType::Float)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .parameter(PrimitiveType::Float)
                        .parameter(PrimitiveType::Float)
                        .ret(PrimitiveType::Float)
                        .build(),
                ],)
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#max
    assert!(
        functions
            .insert(
                "max",
                PolymorphicFunction::new(vec![
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .parameter(PrimitiveType::Integer)
                        .parameter(PrimitiveType::Integer)
                        .ret(PrimitiveType::Integer)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .parameter(PrimitiveType::Integer)
                        .parameter(PrimitiveType::Float)
                        .ret(PrimitiveType::Float)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .parameter(PrimitiveType::Float)
                        .parameter(PrimitiveType::Integer)
                        .ret(PrimitiveType::Float)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .parameter(PrimitiveType::Float)
                        .parameter(PrimitiveType::Float)
                        .ret(PrimitiveType::Float)
                        .build(),
                ],)
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
                        .parameter(PrimitiveType::String)
                        .parameter(PrimitiveType::String)
                        .ret(Type::from(PrimitiveType::String).optional())
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
                        .parameter(PrimitiveType::String)
                        .parameter(PrimitiveType::String)
                        .ret(PrimitiveType::Boolean)
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
                        .parameter(PrimitiveType::String)
                        .parameter(PrimitiveType::String)
                        .parameter(PrimitiveType::String)
                        .ret(PrimitiveType::String)
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#basename
    assert!(
        functions
            .insert(
                "basename",
                PolymorphicFunction::new(vec![
                    FunctionSignature::builder()
                        .required(1)
                        .parameter(PrimitiveType::File)
                        .parameter(PrimitiveType::String)
                        .ret(PrimitiveType::String)
                        .build(),
                    // This overload isn't explicitly specified in the spec, but the spec
                    // allows for `String` where file/directory are accepted; an explicit
                    // `String` overload is required as `String` may coerce to either `File` or
                    // `Directory`, which is ambiguous.
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .required(1)
                        .parameter(PrimitiveType::String)
                        .parameter(PrimitiveType::String)
                        .ret(PrimitiveType::String)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .required(1)
                        .parameter(PrimitiveType::Directory)
                        .parameter(PrimitiveType::String)
                        .ret(PrimitiveType::String)
                        .build(),
                ],)
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#-join_paths
    assert!(
        functions
            .insert(
                "join_paths",
                PolymorphicFunction::new(vec![
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .parameter(PrimitiveType::File)
                        .parameter(PrimitiveType::String)
                        .ret(PrimitiveType::File)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .parameter(PrimitiveType::File)
                        .parameter(array_string_non_empty.clone())
                        .ret(PrimitiveType::File)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .parameter(array_string_non_empty.clone())
                        .ret(PrimitiveType::File)
                        .build(),
                ],)
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
                        .parameter(PrimitiveType::String)
                        .ret(array_file.clone())
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

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
                        .parameter(Type::None)
                        .parameter(PrimitiveType::String)
                        .ret(PrimitiveType::Float)
                        .build(),
                    FunctionSignature::builder()
                        .required(1)
                        .parameter(Type::from(PrimitiveType::File).optional())
                        .parameter(PrimitiveType::String)
                        .ret(PrimitiveType::Float)
                        .build(),
                    // This overload isn't explicitly specified in the spec, but the spec
                    // allows for `String` where file/directory are accepted; an explicit
                    // `String` overload is required as `String` may coerce to either `File` or
                    // `Directory`, which is ambiguous.
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .required(1)
                        .parameter(Type::from(PrimitiveType::String).optional())
                        .parameter(PrimitiveType::String)
                        .ret(PrimitiveType::Float)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .required(1)
                        .parameter(Type::from(PrimitiveType::Directory).optional())
                        .parameter(PrimitiveType::String)
                        .ret(PrimitiveType::Float)
                        .build(),
                    FunctionSignature::builder()
                        .required(1)
                        .type_parameter("X", SizeableConstraint)
                        .parameter(GenericType::Parameter("X"))
                        .parameter(PrimitiveType::String)
                        .ret(PrimitiveType::Float)
                        .build(),
                ],)
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
                        .parameter(PrimitiveType::File)
                        .ret(PrimitiveType::String)
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
                        .parameter(PrimitiveType::File)
                        .ret(PrimitiveType::Integer)
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
                        .parameter(PrimitiveType::File)
                        .ret(PrimitiveType::Float)
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
                        .parameter(PrimitiveType::File)
                        .ret(PrimitiveType::Boolean)
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
                        .parameter(PrimitiveType::File)
                        .ret(array_string.clone())
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
                        .parameter(array_string.clone())
                        .ret(PrimitiveType::File)
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#read_tsv
    assert!(
        functions
            .insert(
                "read_tsv",
                PolymorphicFunction::new(vec![
                    FunctionSignature::builder()
                        .parameter(PrimitiveType::File)
                        .ret(array_array_string.clone())
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .parameter(PrimitiveType::File)
                        .parameter(PrimitiveType::Boolean)
                        .ret(array_object.clone())
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .parameter(PrimitiveType::File)
                        .parameter(PrimitiveType::Boolean)
                        .parameter(array_string.clone())
                        .ret(array_object.clone())
                        .build(),
                ],)
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#write_tsv
    assert!(
        functions
            .insert(
                "write_tsv",
                PolymorphicFunction::new(vec![
                    FunctionSignature::builder()
                        .parameter(array_array_string.clone())
                        .ret(PrimitiveType::File)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .parameter(array_array_string.clone())
                        .parameter(PrimitiveType::Boolean)
                        .parameter(array_string.clone())
                        .ret(PrimitiveType::File)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .type_parameter("S", PrimitiveStructConstraint)
                        .required(1)
                        .parameter(GenericArrayType::new(GenericType::Parameter("S")))
                        .parameter(PrimitiveType::Boolean)
                        .parameter(array_string.clone())
                        .ret(PrimitiveType::File)
                        .build(),
                ],)
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
                        .parameter(PrimitiveType::File)
                        .ret(map_string_string.clone())
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
                        .parameter(map_string_string.clone())
                        .ret(PrimitiveType::File)
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
                        .parameter(PrimitiveType::File)
                        .ret(Type::Union)
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
                        .parameter(GenericType::Parameter("X"))
                        .ret(PrimitiveType::File)
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
                        .parameter(PrimitiveType::File)
                        .ret(Type::Object)
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
                        .parameter(PrimitiveType::File)
                        .ret(array_object.clone())
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#write_object
    assert!(
        functions
            .insert(
                "write_object",
                PolymorphicFunction::new(vec![
                    FunctionSignature::builder()
                        .parameter(Type::Object)
                        .ret(PrimitiveType::File)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .type_parameter("S", PrimitiveStructConstraint)
                        .parameter(GenericType::Parameter("S"))
                        .ret(PrimitiveType::File)
                        .build(),
                ],)
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#write_objects
    assert!(
        functions
            .insert(
                "write_objects",
                PolymorphicFunction::new(vec![
                    FunctionSignature::builder()
                        .parameter(array_object.clone())
                        .ret(PrimitiveType::File)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::One))
                        .type_parameter("S", PrimitiveStructConstraint)
                        .parameter(GenericArrayType::new(GenericType::Parameter("S")))
                        .ret(PrimitiveType::File)
                        .build(),
                ],)
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
                        .parameter(PrimitiveType::String)
                        .parameter(GenericArrayType::new(GenericType::Parameter("P")))
                        .ret(array_string.clone())
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
                        .parameter(PrimitiveType::String)
                        .parameter(GenericArrayType::new(GenericType::Parameter("P")))
                        .ret(array_string.clone())
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
                        .parameter(GenericArrayType::new(GenericType::Parameter("P")))
                        .ret(array_string.clone())
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
                        .parameter(GenericArrayType::new(GenericType::Parameter("P")))
                        .ret(array_string.clone())
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
                        .parameter(PrimitiveType::String)
                        .parameter(GenericArrayType::new(GenericType::Parameter("P")))
                        .ret(PrimitiveType::String)
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
                        .parameter(PrimitiveType::Integer)
                        .ret(array_int.clone())
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
                        .parameter(GenericArrayType::new(GenericArrayType::new(
                            GenericType::Parameter("X"),
                        )))
                        .ret(GenericArrayType::new(GenericArrayType::new(
                            GenericType::Parameter("X"),
                        )))
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
                        .parameter(GenericArrayType::new(GenericType::Parameter("X")))
                        .parameter(GenericArrayType::new(GenericType::Parameter("Y")))
                        .ret(GenericArrayType::new(GenericPairType::new(
                            GenericType::Parameter("X"),
                            GenericType::Parameter("Y"),
                        )))
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
                        .parameter(GenericArrayType::new(GenericType::Parameter("X")))
                        .parameter(GenericArrayType::new(GenericType::Parameter("Y")))
                        .ret(GenericArrayType::new(GenericPairType::new(
                            GenericType::Parameter("X"),
                            GenericType::Parameter("Y"),
                        )))
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
                        .parameter(GenericArrayType::new(GenericPairType::new(
                            GenericType::Parameter("X"),
                            GenericType::Parameter("Y"),
                        )))
                        .ret(GenericPairType::new(
                            GenericArrayType::new(GenericType::Parameter("X")),
                            GenericArrayType::new(GenericType::Parameter("Y")),
                        ))
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
                        .parameter(GenericArrayType::new(GenericType::Parameter("P")))
                        .parameter(GenericType::Parameter("P"))
                        .ret(PrimitiveType::Boolean)
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
                        .parameter(GenericArrayType::new(GenericType::Parameter("X")))
                        .parameter(PrimitiveType::Integer)
                        .ret(GenericArrayType::new(GenericArrayType::new(
                            GenericType::Parameter("X"),
                        )))
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
                        .parameter(GenericArrayType::new(GenericArrayType::new(
                            GenericType::Parameter("X")
                        )))
                        .ret(GenericArrayType::new(GenericType::Parameter("X")))
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
                        .parameter(GenericArrayType::new(GenericType::Parameter("X")))
                        .parameter(GenericType::UnqualifiedParameter("X"))
                        .ret(GenericType::UnqualifiedParameter("X"))
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
                        .parameter(GenericArrayType::new(GenericType::Parameter("X")))
                        .ret(GenericArrayType::new(GenericType::UnqualifiedParameter(
                            "X"
                        )))
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
                        .parameter(GenericMapType::new(
                            GenericType::Parameter("K"),
                            GenericType::Parameter("V")
                        ))
                        .ret(GenericArrayType::new(GenericPairType::new(
                            GenericType::Parameter("K"),
                            GenericType::Parameter("V")
                        )))
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
                        .parameter(GenericArrayType::new(GenericPairType::new(
                            GenericType::Parameter("K"),
                            GenericType::Parameter("V")
                        )))
                        .ret(GenericMapType::new(
                            GenericType::Parameter("K"),
                            GenericType::Parameter("V")
                        ))
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

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
                        .parameter(GenericMapType::new(
                            GenericType::Parameter("K"),
                            GenericType::Parameter("V")
                        ))
                        .ret(GenericArrayType::new(GenericType::Parameter("K")))
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .type_parameter("S", StructConstraint)
                        .parameter(GenericType::Parameter("S"))
                        .ret(array_string.clone())
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .parameter(Type::Object)
                        .ret(array_string.clone())
                        .build(),
                ])
                .into(),
            )
            .is_none()
    );

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
                        .parameter(GenericMapType::new(
                            GenericType::Parameter("K"),
                            GenericType::Parameter("V")
                        ))
                        .parameter(GenericType::Parameter("K"))
                        .ret(PrimitiveType::Boolean)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .parameter(Type::Object)
                        .parameter(PrimitiveType::String)
                        .ret(PrimitiveType::Boolean)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .any_type_parameter("V")
                        .parameter(GenericMapType::new(
                            PrimitiveType::String,
                            GenericType::Parameter("V")
                        ))
                        .parameter(array_string.clone())
                        .ret(PrimitiveType::Boolean)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .type_parameter("S", StructConstraint)
                        .parameter(GenericType::Parameter("S"))
                        .parameter(array_string.clone())
                        .ret(PrimitiveType::Boolean)
                        .build(),
                    FunctionSignature::builder()
                        .min_version(SupportedVersion::V1(V1::Two))
                        .parameter(Type::Object)
                        .parameter(array_string.clone())
                        .ret(PrimitiveType::Boolean)
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
                        .parameter(GenericMapType::new(
                            GenericType::Parameter("K"),
                            GenericType::Parameter("V")
                        ))
                        .ret(GenericArrayType::new(GenericType::Parameter("V")))
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
                        .parameter(GenericArrayType::new(GenericPairType::new(
                            GenericType::Parameter("K"),
                            GenericType::Parameter("V")
                        )))
                        .ret(GenericMapType::new(
                            GenericType::Parameter("K"),
                            GenericArrayType::new(GenericType::Parameter("V"))
                        ))
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
                        .parameter(GenericType::Parameter("X"))
                        .ret(PrimitiveType::Boolean)
                        .build(),
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#length
    assert!(
        functions
            .insert(
                "length",
                PolymorphicFunction::new(vec![
                    FunctionSignature::builder()
                        .any_type_parameter("X")
                        .parameter(GenericArrayType::new(GenericType::Parameter("X")))
                        .ret(PrimitiveType::Integer)
                        .build(),
                    FunctionSignature::builder()
                        .any_type_parameter("K")
                        .any_type_parameter("V")
                        .parameter(GenericMapType::new(
                            GenericType::Parameter("K"),
                            GenericType::Parameter("V")
                        ))
                        .ret(PrimitiveType::Integer)
                        .build(),
                    FunctionSignature::builder()
                        .parameter(Type::Object)
                        .ret(PrimitiveType::Integer)
                        .build(),
                    FunctionSignature::builder()
                        .parameter(PrimitiveType::String)
                        .ret(PrimitiveType::Integer)
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
                "floor(Float) -> Int",
                "ceil(Float) -> Int",
                "round(Float) -> Int",
                "min(Int, Int) -> Int",
                "min(Int, Float) -> Float",
                "min(Float, Int) -> Float",
                "min(Float, Float) -> Float",
                "max(Int, Int) -> Int",
                "max(Int, Float) -> Float",
                "max(Float, Int) -> Float",
                "max(Float, Float) -> Float",
                "find(String, String) -> String?",
                "matches(String, String) -> Boolean",
                "sub(String, String, String) -> String",
                "basename(File, <String>) -> String",
                "basename(String, <String>) -> String",
                "basename(Directory, <String>) -> String",
                "join_paths(File, String) -> File",
                "join_paths(File, Array[String]+) -> File",
                "join_paths(Array[String]+) -> File",
                "glob(String) -> Array[File]",
                "size(None, <String>) -> Float",
                "size(File?, <String>) -> Float",
                "size(String?, <String>) -> Float",
                "size(Directory?, <String>) -> Float",
                "size(X, <String>) -> Float where `X`: any compound type that recursively \
                 contains a `File` or `Directory`",
                "stdout() -> File",
                "stderr() -> File",
                "read_string(File) -> String",
                "read_int(File) -> Int",
                "read_float(File) -> Float",
                "read_boolean(File) -> Boolean",
                "read_lines(File) -> Array[String]",
                "write_lines(Array[String]) -> File",
                "read_tsv(File) -> Array[Array[String]]",
                "read_tsv(File, Boolean) -> Array[Object]",
                "read_tsv(File, Boolean, Array[String]) -> Array[Object]",
                "write_tsv(Array[Array[String]]) -> File",
                "write_tsv(Array[Array[String]], Boolean, Array[String]) -> File",
                "write_tsv(Array[S], <Boolean>, <Array[String]>) -> File where `S`: any structure \
                 containing only primitive types",
                "read_map(File) -> Map[String, String]",
                "write_map(Map[String, String]) -> File",
                "read_json(File) -> Union",
                "write_json(X) -> File where `X`: any JSON-serializable type",
                "read_object(File) -> Object",
                "read_objects(File) -> Array[Object]",
                "write_object(Object) -> File",
                "write_object(S) -> File where `S`: any structure containing only primitive types",
                "write_objects(Array[Object]) -> File",
                "write_objects(Array[S]) -> File where `S`: any structure containing only \
                 primitive types",
                "prefix(String, Array[P]) -> Array[String] where `P`: any primitive type",
                "suffix(String, Array[P]) -> Array[String] where `P`: any primitive type",
                "quote(Array[P]) -> Array[String] where `P`: any primitive type",
                "squote(Array[P]) -> Array[String] where `P`: any primitive type",
                "sep(String, Array[P]) -> String where `P`: any primitive type",
                "range(Int) -> Array[Int]",
                "transpose(Array[Array[X]]) -> Array[Array[X]]",
                "cross(Array[X], Array[Y]) -> Array[Pair[X, Y]]",
                "zip(Array[X], Array[Y]) -> Array[Pair[X, Y]]",
                "unzip(Array[Pair[X, Y]]) -> Pair[Array[X], Array[Y]]",
                "contains(Array[P], P) -> Boolean where `P`: any primitive type",
                "chunk(Array[X], Int) -> Array[Array[X]]",
                "flatten(Array[Array[X]]) -> Array[X]",
                "select_first(Array[X], <X>) -> X",
                "select_all(Array[X]) -> Array[X]",
                "as_pairs(Map[K, V]) -> Array[Pair[K, V]] where `K`: any primitive type",
                "as_map(Array[Pair[K, V]]) -> Map[K, V] where `K`: any primitive type",
                "keys(Map[K, V]) -> Array[K] where `K`: any primitive type",
                "keys(S) -> Array[String] where `S`: any structure",
                "keys(Object) -> Array[String]",
                "contains_key(Map[K, V], K) -> Boolean where `K`: any primitive type",
                "contains_key(Object, String) -> Boolean",
                "contains_key(Map[String, V], Array[String]) -> Boolean",
                "contains_key(S, Array[String]) -> Boolean where `S`: any structure",
                "contains_key(Object, Array[String]) -> Boolean",
                "values(Map[K, V]) -> Array[V] where `K`: any primitive type",
                "collect_by_key(Array[Pair[K, V]]) -> Map[K, Array[V]] where `K`: any primitive \
                 type",
                "defined(X) -> Boolean",
                "length(Array[X]) -> Int",
                "length(Map[K, V]) -> Int",
                "length(Object) -> Int",
                "length(String) -> Int",
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
            .bind(SupportedVersion::V1(V1::Zero), &[array.clone()])
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
            .bind(SupportedVersion::V1(V1::Zero), &[array.clone()])
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
