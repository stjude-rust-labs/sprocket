//! Representation of WDL standard library functions.

use std::cell::Cell;
use std::fmt;
use std::fmt::Write;
use std::sync::LazyLock;

use indexmap::IndexMap;
use indexmap::IndexSet;
use wdl_ast::version::V1;
use wdl_ast::SupportedVersion;

use crate::types::ArrayType;
use crate::types::Coercible;
use crate::types::CompoundTypeDef;
use crate::types::MapType;
use crate::types::Optional;
use crate::types::PairType;
use crate::types::PrimitiveType;
use crate::types::PrimitiveTypeKind;
use crate::types::Type;
use crate::types::TypeEq;
use crate::types::Types;

mod constraints;

pub use constraints::*;

/// The maximum number of allowable type parameters in a function signature.
///
/// This is intentionally set low to limit the amount of space needed to store
/// associated data.
///
/// Accessing `STDLIB` will panic if a signature is defined that exceeds this
/// number.
const MAX_TYPE_PARAMETERS: usize = 4;

#[allow(clippy::missing_docs_in_private_items)]
const _: () = assert!(
    MAX_TYPE_PARAMETERS < usize::BITS as usize,
    "the maximum number of type parameters cannot exceed the number of bits in usize"
);

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
    pub fn display<'a>(
        &'a self,
        types: &'a Types,
        params: &'a TypeParameters<'a>,
    ) -> impl fmt::Display + 'a {
        #[allow(clippy::missing_docs_in_private_items)]
        struct Display<'a> {
            types: &'a Types,
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
                                    ty.require().display(self.types).fmt(f)
                                } else {
                                    ty.display(self.types).fmt(f)
                                }
                            }
                            None => {
                                write!(f, "{name}")
                            }
                        }
                    }
                    GenericType::Array(ty) => ty.display(self.types, self.params).fmt(f),
                    GenericType::Pair(ty) => ty.display(self.types, self.params).fmt(f),
                    GenericType::Map(ty) => ty.display(self.types, self.params).fmt(f),
                }
            }
        }

        Display {
            types,
            params,
            ty: self,
        }
    }

    /// Infers any type parameters from the generic type.
    fn infer_type_parameters(&self, types: &Types, ty: Type, params: &mut TypeParameters<'_>) {
        match self {
            Self::Parameter(name) | Self::UnqualifiedParameter(name) => {
                // Verify the type satisfies any constraint
                let (param, _) = params.get(name).expect("should have parameter");
                if let Some(constraint) = param.constraint() {
                    if !constraint.satisfied(types, ty) {
                        return;
                    }
                }

                params.set_inferred_type(name, ty);
            }
            Self::Array(array) => array.infer_type_parameters(types, ty, params),
            Self::Pair(pair) => pair.infer_type_parameters(types, ty, params),
            Self::Map(map) => map.infer_type_parameters(types, ty, params),
        }
    }

    /// Realizes the generic type.
    fn realize(&self, types: &mut Types, params: &TypeParameters<'_>) -> Option<Type> {
        match self {
            GenericType::Parameter(name) => {
                params
                    .get(name)
                    .expect("type parameter should be present")
                    .1
            }
            GenericType::UnqualifiedParameter(name) => params
                .get(name)
                .expect("type parameter should be present")
                .1
                .map(|ty| ty.require()),
            GenericType::Array(ty) => ty.realize(types, params),
            GenericType::Pair(ty) => ty.realize(types, params),
            GenericType::Map(ty) => ty.realize(types, params),
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
    pub fn display<'a>(
        &'a self,
        types: &'a Types,
        params: &'a TypeParameters<'a>,
    ) -> impl fmt::Display + 'a {
        #[allow(clippy::missing_docs_in_private_items)]
        struct Display<'a> {
            types: &'a Types,
            params: &'a TypeParameters<'a>,
            ty: &'a GenericArrayType,
        }

        impl fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "Array[")?;
                self.ty
                    .element_type
                    .display(self.types, self.params)
                    .fmt(f)?;
                write!(f, "]")?;

                if self.ty.is_non_empty() {
                    write!(f, "+")?;
                }

                Ok(())
            }
        }

        Display {
            types,
            params,
            ty: self,
        }
    }

    /// Infers any type parameters from the generic type.
    fn infer_type_parameters(&self, types: &Types, ty: Type, params: &mut TypeParameters<'_>) {
        if let Type::Compound(ty) = ty {
            if !ty.is_optional() {
                if let CompoundTypeDef::Array(ty) = types.type_definition(ty.definition()) {
                    self.element_type
                        .infer_type_parameters(types, ty.element_type(), params);
                }
            }
        }
    }

    /// Realizes the generic type to an `Array`.
    fn realize(&self, types: &mut Types, params: &TypeParameters<'_>) -> Option<Type> {
        let ty = self.element_type.realize(types, params)?;
        Some(types.add_array(if self.non_empty {
            ArrayType::non_empty(ty)
        } else {
            ArrayType::new(ty)
        }))
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
    /// The type of the first element of the pair.
    first_type: Box<FunctionalType>,
    /// The type of the second element of the pair.
    second_type: Box<FunctionalType>,
}

impl GenericPairType {
    /// Constructs a new generic pair type.
    pub fn new(
        first_type: impl Into<FunctionalType>,
        second_type: impl Into<FunctionalType>,
    ) -> Self {
        Self {
            first_type: Box::new(first_type.into()),
            second_type: Box::new(second_type.into()),
        }
    }

    /// Gets the pairs's first type.
    pub fn first_type(&self) -> &FunctionalType {
        &self.first_type
    }

    /// Gets the pairs's second type.
    pub fn second_type(&self) -> &FunctionalType {
        &self.second_type
    }

    /// Returns an object that implements `Display` for formatting the type.
    pub fn display<'a>(
        &'a self,
        types: &'a Types,
        params: &'a TypeParameters<'a>,
    ) -> impl fmt::Display + 'a {
        #[allow(clippy::missing_docs_in_private_items)]
        struct Display<'a> {
            types: &'a Types,
            params: &'a TypeParameters<'a>,
            ty: &'a GenericPairType,
        }

        impl fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "Pair[")?;
                self.ty.first_type.display(self.types, self.params).fmt(f)?;
                write!(f, ", ")?;
                self.ty
                    .second_type
                    .display(self.types, self.params)
                    .fmt(f)?;
                write!(f, "]")
            }
        }

        Display {
            types,
            params,
            ty: self,
        }
    }

    /// Infers any type parameters from the generic type.
    fn infer_type_parameters(&self, types: &Types, ty: Type, params: &mut TypeParameters<'_>) {
        if let Type::Compound(ty) = ty {
            if !ty.is_optional() {
                if let CompoundTypeDef::Pair(ty) = types.type_definition(ty.definition()) {
                    self.first_type
                        .infer_type_parameters(types, ty.first_type(), params);
                    self.second_type
                        .infer_type_parameters(types, ty.second_type(), params);
                }
            }
        }
    }

    /// Realizes the generic type to a `Pair`.
    fn realize(&self, types: &mut Types, params: &TypeParameters<'_>) -> Option<Type> {
        let first_type = self.first_type.realize(types, params)?;
        let second_type = self.second_type.realize(types, params)?;
        Some(types.add_pair(PairType::new(first_type, second_type)))
    }

    /// Asserts that the type parameters referenced by the type are valid.
    ///
    /// # Panics
    ///
    /// Panics if referenced type parameter is invalid.
    fn assert_type_parameters(&self, parameters: &[TypeParameter]) {
        self.first_type.assert_type_parameters(parameters);
        self.second_type.assert_type_parameters(parameters);
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
    pub fn display<'a>(
        &'a self,
        types: &'a Types,
        params: &'a TypeParameters<'a>,
    ) -> impl fmt::Display + 'a {
        #[allow(clippy::missing_docs_in_private_items)]
        struct Display<'a> {
            types: &'a Types,
            params: &'a TypeParameters<'a>,
            ty: &'a GenericMapType,
        }

        impl fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "Map[")?;
                self.ty.key_type.display(self.types, self.params).fmt(f)?;
                write!(f, ", ")?;
                self.ty.value_type.display(self.types, self.params).fmt(f)?;
                write!(f, "]")
            }
        }

        Display {
            types,
            params,
            ty: self,
        }
    }

    /// Infers any type parameters from the generic type.
    fn infer_type_parameters(&self, types: &Types, ty: Type, params: &mut TypeParameters<'_>) {
        if let Type::Compound(ty) = ty {
            if !ty.is_optional() {
                if let CompoundTypeDef::Map(ty) = types.type_definition(ty.definition()) {
                    self.key_type
                        .infer_type_parameters(types, ty.key_type(), params);
                    self.value_type
                        .infer_type_parameters(types, ty.value_type(), params);
                }
            }
        }
    }

    /// Realizes the generic type to a `Map`.
    fn realize(&self, types: &mut Types, params: &TypeParameters<'_>) -> Option<Type> {
        let key_type = self.key_type.realize(types, params)?;
        let value_type = self.value_type.realize(types, params)?;
        Some(types.add_map(MapType::new(key_type, value_type)))
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
    /// Constructs a new type parameters collection.
    ///
    /// # Panics
    ///
    /// Panics if the count of the given type parameters exceeds the maximum
    /// allowed.
    fn new(parameters: &'a [TypeParameter]) -> Self {
        assert!(
            parameters.len() < MAX_TYPE_PARAMETERS,
            "no more than {MAX_TYPE_PARAMETERS} type parameters is supported"
        );

        Self {
            parameters,
            inferred_types: [None; MAX_TYPE_PARAMETERS],
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

        Some((&self.parameters[index], self.inferred_types[index]))
    }

    /// Reset any referenced type parameters.
    pub fn reset(&self) {
        self.referenced.set(0);
    }

    /// Gets an iterator of the type parameters that have been referenced since
    /// the last reset.
    pub fn referenced(&self) -> impl Iterator<Item = (&TypeParameter, Option<Type>)> {
        let mut bits = self.referenced.get();
        std::iter::from_fn(move || {
            if bits == 0 {
                return None;
            }

            let index = bits.trailing_zeros() as usize;
            let parameter = &self.parameters[index];
            let ty = self.inferred_types[index];
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
    pub fn concrete_type(&self) -> Option<Type> {
        match self {
            Self::Concrete(ty) => Some(*ty),
            Self::Generic(_) => None,
        }
    }

    /// Returns an object that implements `Display` for formatting the type.
    pub fn display<'a>(
        &'a self,
        types: &'a Types,
        params: &'a TypeParameters<'a>,
    ) -> impl fmt::Display + 'a {
        #[allow(clippy::missing_docs_in_private_items)]
        struct Display<'a> {
            types: &'a Types,
            params: &'a TypeParameters<'a>,
            ty: &'a FunctionalType,
        }

        impl fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self.ty {
                    FunctionalType::Concrete(ty) => ty.display(self.types).fmt(f),
                    FunctionalType::Generic(ty) => ty.display(self.types, self.params).fmt(f),
                }
            }
        }

        Display {
            types,
            params,
            ty: self,
        }
    }

    /// Infers any type parameters if the type is generic.
    fn infer_type_parameters(&self, types: &Types, ty: Type, params: &mut TypeParameters<'_>) {
        if let Self::Generic(generic) = self {
            generic.infer_type_parameters(types, ty, params);
        }
    }

    /// Realizes the type if the type is generic.
    fn realize(&self, types: &mut Types, params: &TypeParameters<'_>) -> Option<Type> {
        match self {
            FunctionalType::Concrete(ty) => Some(*ty),
            FunctionalType::Generic(ty) => ty.realize(types, params),
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

impl From<PrimitiveTypeKind> for FunctionalType {
    fn from(value: PrimitiveTypeKind) -> Self {
        Self::Concrete(value.into())
    }
}

impl From<PrimitiveType> for FunctionalType {
    fn from(value: PrimitiveType) -> Self {
        Self::Concrete(Type::Primitive(value))
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

/// Represents a successful binding of arguments to a function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Binding {
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

impl Binding {
    /// Gets the binding's return type.
    pub fn ret(&self) -> Type {
        match self {
            Self::Equivalence(ty) | Self::Coercion(ty) => *ty,
        }
    }
}

/// Represents a WDL function signature.
#[derive(Debug)]
pub struct FunctionSignature {
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
    pub fn display<'a>(
        &'a self,
        types: &'a Types,
        params: &'a TypeParameters<'a>,
    ) -> impl fmt::Display + 'a {
        #[allow(clippy::missing_docs_in_private_items)]
        struct Display<'a> {
            types: &'a Types,
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
                        "{param}",
                        param = parameter.display(self.types, self.params)
                    )?;

                    if i >= required {
                        f.write_char('>')?;
                    }
                }

                write!(
                    f,
                    ") -> {ret}",
                    ret = self.sig.ret.display(self.types, self.params)
                )?;
                write_uninferred_constraints(f, self.params)?;

                Ok(())
            }
        }

        Display {
            types,
            params,
            sig: self,
        }
    }

    /// Infers the concrete types of any type parameters for the function
    /// signature.
    ///
    /// Returns the collection of type parameters.
    fn infer_type_parameters(&self, types: &Types, arguments: &[Type]) -> TypeParameters<'_> {
        let mut parameters = TypeParameters::new(&self.type_parameters);
        for (parameter, argument) in self.parameters.iter().zip(arguments.iter()) {
            parameter.infer_type_parameters(types, *argument, &mut parameters);
        }

        parameters
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
    fn bind(&self, types: &mut Types, arguments: &[Type]) -> Result<Binding, FunctionBindError> {
        let required = self.required();
        if arguments.len() < required {
            return Err(FunctionBindError::TooFewArguments(required));
        }

        if arguments.len() > self.parameters.len() {
            return Err(FunctionBindError::TooManyArguments(self.parameters.len()));
        }

        // Ensure the argument types are correct for the function
        let mut coerced = false;
        let type_parameters = self.infer_type_parameters(types, arguments);
        for (i, (parameter, argument)) in self.parameters.iter().zip(arguments.iter()).enumerate() {
            match parameter.realize(types, &type_parameters) {
                Some(ty) => {
                    // If a coercion hasn't occurred yet, check for type equivalence
                    // Otherwise, fall back to coercion
                    if !coerced && !argument.type_eq(types, &ty) {
                        coerced = true;
                    }

                    if coerced && !argument.is_coercible_to(types, &ty) {
                        return Err(FunctionBindError::ArgumentTypeMismatch {
                            index: i,
                            expected: format!("`{ty}`", ty = ty.display(types)),
                        });
                    }
                }
                None if *argument == Type::Union => {
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
                        param = parameter.display(types, &type_parameters)
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
        let ret = self
            .ret()
            .realize(types, &type_parameters)
            .unwrap_or(Type::Union);

        if coerced {
            Ok(Binding::Coercion(ret))
        } else {
            Ok(Binding::Equivalence(ret))
        }
    }
}

impl Default for FunctionSignature {
    fn default() -> Self {
        Self {
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

        // Ensure any generic type parameters indexes are in range for the parameters
        for param in sig.parameters.iter() {
            param.assert_type_parameters(&sig.type_parameters)
        }

        sig.ret().assert_type_parameters(&sig.type_parameters);

        sig
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
    /// Gets the minimum supported WDL version for the function.
    pub fn minimum_version(&self) -> SupportedVersion {
        match self {
            Self::Monomorphic(f) => f.minimum_version,
            Self::Polymorphic(f) => f.minimum_version,
        }
    }

    /// Binds the function to the given arguments.
    pub fn bind(&self, types: &mut Types, arguments: &[Type]) -> Result<Type, FunctionBindError> {
        match self {
            Self::Monomorphic(f) => f.bind(types, arguments),
            Self::Polymorphic(f) => f.bind(types, arguments),
        }
    }

    /// Gets the return type of the function.
    ///
    /// Returns `None` if the function return type cannot be statically
    /// determined.
    ///
    /// This may occur for functions with a generic return type or if the
    /// function is polymorphic and not every overload of the function has the
    /// same return type.
    pub fn ret(&self, types: &Types) -> Option<Type> {
        match self {
            Self::Monomorphic(f) => f.signature.ret.concrete_type(),
            Self::Polymorphic(f) => {
                let ty = f.signatures[0].ret.concrete_type()?;
                for signature in f.signatures.iter().skip(1) {
                    if signature.ret.concrete_type()?.type_eq(types, &ty) {
                        return None;
                    }
                }

                Some(ty)
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
    /// The minimum required version for the function.
    minimum_version: SupportedVersion,
    /// The signature of the function.
    signature: FunctionSignature,
}

impl MonomorphicFunction {
    /// Constructs a new monomorphic function.
    pub fn new(minimum_version: SupportedVersion, signature: FunctionSignature) -> Self {
        Self {
            minimum_version,
            signature,
        }
    }

    /// Gets the minimum supported WDL version for the function.
    pub fn minimum_version(&self) -> SupportedVersion {
        self.minimum_version
    }

    /// Gets the signature of the function.
    pub fn signature(&self) -> &FunctionSignature {
        &self.signature
    }

    /// Binds the function to the given arguments.
    pub fn bind(&self, types: &mut Types, arguments: &[Type]) -> Result<Type, FunctionBindError> {
        Ok(self.signature.bind(types, arguments)?.ret())
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
    /// The minimum required version for the function.
    minimum_version: SupportedVersion,
    /// The signatures of the function.
    signatures: Vec<FunctionSignature>,
}

impl PolymorphicFunction {
    /// Constructs a new polymorphic function.
    ///
    /// # Panics
    ///
    /// Panics if the number of signatures is less than two.
    pub fn new(minimum_version: SupportedVersion, signatures: Vec<FunctionSignature>) -> Self {
        assert!(
            signatures.len() > 1,
            "a polymorphic function must have at least two signatures"
        );

        Self {
            minimum_version,
            signatures,
        }
    }

    /// Gets the minimum supported WDL version for the function.
    pub fn minimum_version(&self) -> SupportedVersion {
        self.minimum_version
    }

    /// Gets the signatures of the function.
    pub fn signatures(&self) -> &[FunctionSignature] {
        &self.signatures
    }

    /// Binds the function to the given arguments.
    ///
    /// This performs overload resolution for the polymorphic function.
    pub fn bind(&self, types: &mut Types, arguments: &[Type]) -> Result<Type, FunctionBindError> {
        // First check the min/max parameter counts
        let mut min = usize::MAX;
        let mut max = 0;
        for sig in &self.signatures {
            min = std::cmp::min(min, sig.required());
            max = std::cmp::max(max, sig.parameters().len());
        }

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
            for (index, signature) in self
                .signatures
                .iter()
                .enumerate()
                .filter(|(_, s)| s.is_generic() == generic)
            {
                match signature.bind(types, arguments) {
                    Ok(Binding::Equivalence(ty)) => {
                        // We cannot have more than one exact match
                        if let Some((previous, _)) = exact {
                            return Err(FunctionBindError::Ambiguous {
                                first: self.signatures[previous]
                                    .display(
                                        types,
                                        &TypeParameters::new(
                                            &self.signatures[previous].type_parameters,
                                        ),
                                    )
                                    .to_string(),
                                second: self.signatures[index]
                                    .display(
                                        types,
                                        &TypeParameters::new(
                                            &self.signatures[index].type_parameters,
                                        ),
                                    )
                                    .to_string(),
                            });
                        }

                        exact = Some((index, ty));
                    }
                    Ok(Binding::Coercion(ty)) => {
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
                        FunctionBindError::Ambiguous { .. }
                        | FunctionBindError::TooFewArguments(_)
                        | FunctionBindError::TooManyArguments(_),
                    ) => continue,
                }
            }

            if let Some((_, ty)) = exact {
                return Ok(ty);
            }

            // Ensure there wasn't more than one coercion
            if let Some(previous) = coercion2 {
                let index = coercion1.unwrap().0;
                return Err(FunctionBindError::Ambiguous {
                    first: self.signatures[previous]
                        .display(
                            types,
                            &TypeParameters::new(&self.signatures[previous].type_parameters),
                        )
                        .to_string(),
                    second: self.signatures[index]
                        .display(
                            types,
                            &TypeParameters::new(&self.signatures[index].type_parameters),
                        )
                        .to_string(),
                });
            }

            if let Some((_, ty)) = coercion1 {
                return Ok(ty);
            }
        }

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
    /// The types used to defined the standard library.
    types: Types,
    /// A map of function name to function definition.
    functions: IndexMap<&'static str, Function>,
    /// The type for `Array[String]`.
    pub(crate) array_string: Type,
    /// The type for `Map[String, Int]`.
    pub(crate) map_string_int: Type,
}

impl StandardLibrary {
    /// Gets the types used to define the standard library.
    pub fn types(&self) -> &Types {
        &self.types
    }

    /// Gets a standard library function by name.
    pub fn function(&self, name: &str) -> Option<&Function> {
        self.functions.get(name)
    }

    /// Gets an iterator over all the functions in the standard library.
    pub fn functions(&self) -> impl Iterator<Item = (&'static str, &Function)> {
        self.functions.iter().map(|(n, f)| (*n, f))
    }
}

/// Represents the WDL standard library.
pub static STDLIB: LazyLock<StandardLibrary> = LazyLock::new(|| {
    let mut types = Types::new();
    let array_int = types.add_array(ArrayType::new(PrimitiveTypeKind::Integer));
    let array_string = types.add_array(ArrayType::new(PrimitiveTypeKind::String));
    let array_file = types.add_array(ArrayType::new(PrimitiveTypeKind::File));
    let array_object = types.add_array(ArrayType::new(Type::Object));
    let array_string_non_empty = types.add_array(ArrayType::non_empty(PrimitiveTypeKind::String));
    let array_array_string = types.add_array(ArrayType::new(array_string));
    let map_string_string = types.add_map(MapType::new(
        PrimitiveTypeKind::String,
        PrimitiveTypeKind::String,
    ));
    let map_string_int = types.add_map(MapType::new(
        PrimitiveTypeKind::String,
        PrimitiveTypeKind::Integer,
    ));

    let mut functions = IndexMap::new();

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#floor
    assert!(
        functions
            .insert(
                "floor",
                MonomorphicFunction::new(
                    SupportedVersion::V1(V1::Zero),
                    FunctionSignature::builder()
                        .parameter(PrimitiveTypeKind::Float)
                        .ret(PrimitiveTypeKind::Integer)
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
                    SupportedVersion::V1(V1::Zero),
                    FunctionSignature::builder()
                        .parameter(PrimitiveTypeKind::Float)
                        .ret(PrimitiveTypeKind::Integer)
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
                    SupportedVersion::V1(V1::Zero),
                    FunctionSignature::builder()
                        .parameter(PrimitiveTypeKind::Float)
                        .ret(PrimitiveTypeKind::Integer)
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
                PolymorphicFunction::new(
                    SupportedVersion::V1(V1::One),
                    vec![
                        FunctionSignature::builder()
                            .parameter(PrimitiveTypeKind::Integer)
                            .parameter(PrimitiveTypeKind::Integer)
                            .ret(PrimitiveTypeKind::Integer)
                            .build(),
                        FunctionSignature::builder()
                            .parameter(PrimitiveTypeKind::Integer)
                            .parameter(PrimitiveTypeKind::Float)
                            .ret(PrimitiveTypeKind::Float)
                            .build(),
                        FunctionSignature::builder()
                            .parameter(PrimitiveTypeKind::Float)
                            .parameter(PrimitiveTypeKind::Integer)
                            .ret(PrimitiveTypeKind::Float)
                            .build(),
                        FunctionSignature::builder()
                            .parameter(PrimitiveTypeKind::Float)
                            .parameter(PrimitiveTypeKind::Float)
                            .ret(PrimitiveTypeKind::Float)
                            .build(),
                    ],
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#max
    assert!(
        functions
            .insert(
                "max",
                PolymorphicFunction::new(
                    SupportedVersion::V1(V1::One),
                    vec![
                        FunctionSignature::builder()
                            .parameter(PrimitiveTypeKind::Integer)
                            .parameter(PrimitiveTypeKind::Integer)
                            .ret(PrimitiveTypeKind::Integer)
                            .build(),
                        FunctionSignature::builder()
                            .parameter(PrimitiveTypeKind::Integer)
                            .parameter(PrimitiveTypeKind::Float)
                            .ret(PrimitiveTypeKind::Float)
                            .build(),
                        FunctionSignature::builder()
                            .parameter(PrimitiveTypeKind::Float)
                            .parameter(PrimitiveTypeKind::Integer)
                            .ret(PrimitiveTypeKind::Float)
                            .build(),
                        FunctionSignature::builder()
                            .parameter(PrimitiveTypeKind::Float)
                            .parameter(PrimitiveTypeKind::Float)
                            .ret(PrimitiveTypeKind::Float)
                            .build(),
                    ],
                )
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
                    SupportedVersion::V1(V1::Two),
                    FunctionSignature::builder()
                        .parameter(PrimitiveTypeKind::String)
                        .parameter(PrimitiveTypeKind::String)
                        .ret(PrimitiveType::optional(PrimitiveTypeKind::String))
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
                    SupportedVersion::V1(V1::Two),
                    FunctionSignature::builder()
                        .parameter(PrimitiveTypeKind::String)
                        .parameter(PrimitiveTypeKind::String)
                        .ret(PrimitiveTypeKind::Boolean)
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
                    SupportedVersion::V1(V1::Zero),
                    FunctionSignature::builder()
                        .parameter(PrimitiveTypeKind::String)
                        .parameter(PrimitiveTypeKind::String)
                        .parameter(PrimitiveTypeKind::String)
                        .ret(PrimitiveTypeKind::String)
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
                PolymorphicFunction::new(
                    SupportedVersion::V1(V1::Zero),
                    vec![
                        FunctionSignature::builder()
                            .required(1)
                            .parameter(PrimitiveTypeKind::File)
                            .parameter(PrimitiveTypeKind::String)
                            .ret(PrimitiveTypeKind::String)
                            .build(),
                        // This overload isn't explicitly specified in the spec, but the spec
                        // allows for `String` where file/directory are accepted; an explicit
                        // `String` overload is required as `String` may coerce to either `File` or
                        // `Directory`, which is ambiguous.
                        FunctionSignature::builder()
                            .required(1)
                            .parameter(PrimitiveTypeKind::String)
                            .parameter(PrimitiveTypeKind::String)
                            .ret(PrimitiveTypeKind::String)
                            .build(),
                        FunctionSignature::builder()
                            .required(1)
                            .parameter(PrimitiveTypeKind::Directory)
                            .parameter(PrimitiveTypeKind::String)
                            .ret(PrimitiveTypeKind::String)
                            .build(),
                    ],
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#-join_paths
    assert!(
        functions
            .insert(
                "join_paths",
                PolymorphicFunction::new(
                    SupportedVersion::V1(V1::Two),
                    vec![
                        FunctionSignature::builder()
                            .parameter(PrimitiveTypeKind::File)
                            .parameter(PrimitiveTypeKind::String)
                            .ret(PrimitiveTypeKind::File)
                            .build(),
                        FunctionSignature::builder()
                            .parameter(PrimitiveTypeKind::File)
                            .parameter(array_string_non_empty)
                            .ret(PrimitiveTypeKind::File)
                            .build(),
                        FunctionSignature::builder()
                            .parameter(array_string_non_empty)
                            .ret(PrimitiveTypeKind::File)
                            .build(),
                    ],
                )
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
                    SupportedVersion::V1(V1::Zero),
                    FunctionSignature::builder()
                        .parameter(PrimitiveTypeKind::String)
                        .ret(array_file)
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
                PolymorphicFunction::new(
                    SupportedVersion::V1(V1::Zero),
                    vec![
                        FunctionSignature::builder()
                            .required(1)
                            .parameter(PrimitiveType::optional(PrimitiveTypeKind::File))
                            .parameter(PrimitiveTypeKind::String)
                            .ret(PrimitiveTypeKind::Float)
                            .build(),
                        // This overload isn't explicitly specified in the spec, but the spec
                        // allows for `String` where file/directory are accepted; an explicit
                        // `String` overload is required as `String` may coerce to either `File` or
                        // `Directory`, which is ambiguous.
                        FunctionSignature::builder()
                            .required(1)
                            .parameter(PrimitiveType::optional(PrimitiveTypeKind::String))
                            .parameter(PrimitiveTypeKind::String)
                            .ret(PrimitiveTypeKind::Float)
                            .build(),
                        FunctionSignature::builder()
                            .required(1)
                            .parameter(PrimitiveType::optional(PrimitiveTypeKind::Directory))
                            .parameter(PrimitiveTypeKind::String)
                            .ret(PrimitiveTypeKind::Float)
                            .build(),
                        FunctionSignature::builder()
                            .required(1)
                            .type_parameter("X", SizeableConstraint)
                            .parameter(GenericType::Parameter("X"))
                            .parameter(PrimitiveTypeKind::String)
                            .ret(PrimitiveTypeKind::Float)
                            .build(),
                    ],
                )
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
                    SupportedVersion::V1(V1::Zero),
                    FunctionSignature::builder()
                        .ret(PrimitiveTypeKind::File)
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
                    SupportedVersion::V1(V1::Zero),
                    FunctionSignature::builder()
                        .ret(PrimitiveTypeKind::File)
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
                    SupportedVersion::V1(V1::Zero),
                    FunctionSignature::builder()
                        .parameter(PrimitiveTypeKind::File)
                        .ret(PrimitiveTypeKind::String)
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
                    SupportedVersion::V1(V1::Zero),
                    FunctionSignature::builder()
                        .parameter(PrimitiveTypeKind::File)
                        .ret(PrimitiveTypeKind::Integer)
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
                    SupportedVersion::V1(V1::Zero),
                    FunctionSignature::builder()
                        .parameter(PrimitiveTypeKind::File)
                        .ret(PrimitiveTypeKind::Float)
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
                    SupportedVersion::V1(V1::Zero),
                    FunctionSignature::builder()
                        .parameter(PrimitiveTypeKind::File)
                        .ret(PrimitiveTypeKind::Boolean)
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
                    SupportedVersion::V1(V1::Zero),
                    FunctionSignature::builder()
                        .parameter(PrimitiveTypeKind::File)
                        .ret(array_string)
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
                    SupportedVersion::V1(V1::Zero),
                    FunctionSignature::builder()
                        .parameter(array_string)
                        .ret(PrimitiveTypeKind::File)
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
                PolymorphicFunction::new(
                    SupportedVersion::V1(V1::Zero),
                    vec![
                        FunctionSignature::builder()
                            .parameter(PrimitiveTypeKind::File)
                            .ret(array_array_string)
                            .build(),
                        FunctionSignature::builder()
                            .parameter(PrimitiveTypeKind::File)
                            .parameter(PrimitiveTypeKind::Boolean)
                            .ret(array_object)
                            .build(),
                        FunctionSignature::builder()
                            .parameter(PrimitiveTypeKind::File)
                            .parameter(PrimitiveTypeKind::Boolean)
                            .parameter(array_string)
                            .ret(array_object)
                            .build(),
                    ],
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#write_tsv
    assert!(
        functions
            .insert(
                "write_tsv",
                PolymorphicFunction::new(
                    SupportedVersion::V1(V1::Zero),
                    vec![
                        FunctionSignature::builder()
                            .parameter(array_array_string)
                            .ret(PrimitiveTypeKind::File)
                            .build(),
                        FunctionSignature::builder()
                            .type_parameter("S", StructConstraint)
                            .parameter(GenericArrayType::new(GenericType::Parameter("S")))
                            .ret(PrimitiveTypeKind::File)
                            .build(),
                        FunctionSignature::builder()
                            .parameter(array_array_string)
                            .parameter(PrimitiveTypeKind::Boolean)
                            .parameter(array_string)
                            .ret(PrimitiveTypeKind::File)
                            .build(),
                        FunctionSignature::builder()
                            .type_parameter("S", StructConstraint)
                            .parameter(GenericArrayType::new(GenericType::Parameter("S")))
                            .parameter(PrimitiveTypeKind::Boolean)
                            .parameter(array_string)
                            .ret(PrimitiveTypeKind::File)
                            .build(),
                    ],
                )
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
                    SupportedVersion::V1(V1::Zero),
                    FunctionSignature::builder()
                        .parameter(PrimitiveTypeKind::File)
                        .ret(map_string_string)
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
                    SupportedVersion::V1(V1::Zero),
                    FunctionSignature::builder()
                        .parameter(map_string_string)
                        .ret(PrimitiveTypeKind::File)
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
                    SupportedVersion::V1(V1::Zero),
                    FunctionSignature::builder()
                        .parameter(PrimitiveTypeKind::File)
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
                    SupportedVersion::V1(V1::Zero),
                    FunctionSignature::builder()
                        .type_parameter("X", JsonSerializableConstraint)
                        .parameter(GenericType::Parameter("X"))
                        .ret(PrimitiveTypeKind::File)
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
                    SupportedVersion::V1(V1::Zero),
                    FunctionSignature::builder()
                        .parameter(PrimitiveTypeKind::File)
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
                    SupportedVersion::V1(V1::Zero),
                    FunctionSignature::builder()
                        .parameter(PrimitiveTypeKind::File)
                        .ret(array_object)
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
                PolymorphicFunction::new(
                    SupportedVersion::V1(V1::Zero),
                    vec![
                        FunctionSignature::builder()
                            .parameter(Type::Object)
                            .ret(PrimitiveTypeKind::File)
                            .build(),
                        FunctionSignature::builder()
                            .type_parameter("S", StructConstraint)
                            .parameter(GenericType::Parameter("S"))
                            .ret(PrimitiveTypeKind::File)
                            .build(),
                    ],
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#write_objects
    assert!(
        functions
            .insert(
                "write_objects",
                PolymorphicFunction::new(
                    SupportedVersion::V1(V1::Zero),
                    vec![
                        FunctionSignature::builder()
                            .parameter(array_object)
                            .ret(PrimitiveTypeKind::File)
                            .build(),
                        FunctionSignature::builder()
                            .type_parameter("S", StructConstraint)
                            .parameter(GenericArrayType::new(GenericType::Parameter("S")))
                            .ret(PrimitiveTypeKind::File)
                            .build(),
                    ],
                )
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
                    SupportedVersion::V1(V1::Zero),
                    FunctionSignature::builder()
                        .type_parameter("P", RequiredPrimitiveTypeConstraint)
                        .parameter(PrimitiveTypeKind::String)
                        .parameter(GenericArrayType::new(GenericType::Parameter("P")))
                        .ret(array_string)
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
                    SupportedVersion::V1(V1::One),
                    FunctionSignature::builder()
                        .type_parameter("P", RequiredPrimitiveTypeConstraint)
                        .parameter(PrimitiveTypeKind::String)
                        .parameter(GenericArrayType::new(GenericType::Parameter("P")))
                        .ret(array_string)
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
                    SupportedVersion::V1(V1::One),
                    FunctionSignature::builder()
                        .type_parameter("P", RequiredPrimitiveTypeConstraint)
                        .parameter(GenericArrayType::new(GenericType::Parameter("P")))
                        .ret(array_string)
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
                    SupportedVersion::V1(V1::One),
                    FunctionSignature::builder()
                        .type_parameter("P", RequiredPrimitiveTypeConstraint)
                        .parameter(GenericArrayType::new(GenericType::Parameter("P")))
                        .ret(array_string)
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
                    SupportedVersion::V1(V1::One),
                    FunctionSignature::builder()
                        .type_parameter("P", RequiredPrimitiveTypeConstraint)
                        .parameter(PrimitiveTypeKind::String)
                        .parameter(GenericArrayType::new(GenericType::Parameter("P")))
                        .ret(PrimitiveTypeKind::String)
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
                    SupportedVersion::V1(V1::Zero),
                    FunctionSignature::builder()
                        .parameter(PrimitiveTypeKind::Integer)
                        .ret(array_int)
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
                    SupportedVersion::V1(V1::Zero),
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
                    SupportedVersion::V1(V1::Zero),
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
                    SupportedVersion::V1(V1::Zero),
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
                    SupportedVersion::V1(V1::One),
                    FunctionSignature::builder()
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
                    SupportedVersion::V1(V1::Two),
                    FunctionSignature::builder()
                        .type_parameter("P", AnyPrimitiveTypeConstraint)
                        .parameter(GenericArrayType::new(GenericType::Parameter("P")))
                        .parameter(GenericType::Parameter("P"))
                        .ret(PrimitiveTypeKind::Boolean)
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
                    SupportedVersion::V1(V1::Two),
                    FunctionSignature::builder()
                        .any_type_parameter("X")
                        .parameter(GenericArrayType::new(GenericType::Parameter("X")))
                        .parameter(PrimitiveTypeKind::Integer)
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
                    SupportedVersion::V1(V1::Zero),
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
                PolymorphicFunction::new(
                    SupportedVersion::V1(V1::Zero),
                    vec![
                        FunctionSignature::builder()
                            .type_parameter("X", OptionalTypeConstraint)
                            .parameter(GenericArrayType::non_empty(GenericType::Parameter("X")))
                            .ret(GenericType::UnqualifiedParameter("X"))
                            .build(),
                        FunctionSignature::builder()
                            .type_parameter("X", OptionalTypeConstraint)
                            .required(1)
                            .parameter(GenericArrayType::new(GenericType::Parameter("X")))
                            .parameter(GenericType::UnqualifiedParameter("X"))
                            .ret(GenericType::UnqualifiedParameter("X"))
                            .build(),
                    ]
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
                    SupportedVersion::V1(V1::Zero),
                    FunctionSignature::builder()
                        .type_parameter("X", OptionalTypeConstraint)
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
                    SupportedVersion::V1(V1::One),
                    FunctionSignature::builder()
                        .type_parameter("K", RequiredPrimitiveTypeConstraint)
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
                    SupportedVersion::V1(V1::One),
                    FunctionSignature::builder()
                        .type_parameter("K", RequiredPrimitiveTypeConstraint)
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
                PolymorphicFunction::new(
                    SupportedVersion::V1(V1::One),
                    vec![
                        FunctionSignature::builder()
                            .type_parameter("K", RequiredPrimitiveTypeConstraint)
                            .any_type_parameter("V")
                            .parameter(GenericMapType::new(
                                GenericType::Parameter("K"),
                                GenericType::Parameter("V")
                            ))
                            .ret(GenericArrayType::new(GenericType::Parameter("K")))
                            .build(),
                        FunctionSignature::builder()
                            .type_parameter("S", StructConstraint)
                            .parameter(GenericType::Parameter("S"))
                            .ret(array_string)
                            .build(),
                        FunctionSignature::builder()
                            .parameter(Type::Object)
                            .ret(array_string)
                            .build(),
                    ]
                )
                .into(),
            )
            .is_none()
    );

    // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#contains_key
    assert!(
        functions
            .insert(
                "contains_key",
                PolymorphicFunction::new(
                    SupportedVersion::V1(V1::Two),
                    vec![
                        FunctionSignature::builder()
                            .type_parameter("K", RequiredPrimitiveTypeConstraint)
                            .any_type_parameter("V")
                            .parameter(GenericMapType::new(
                                GenericType::Parameter("K"),
                                GenericType::Parameter("V")
                            ))
                            .parameter(GenericType::Parameter("K"))
                            .ret(PrimitiveTypeKind::Boolean)
                            .build(),
                        FunctionSignature::builder()
                            .parameter(Type::Object)
                            .parameter(PrimitiveTypeKind::String)
                            .ret(PrimitiveTypeKind::Boolean)
                            .build(),
                        FunctionSignature::builder()
                            .any_type_parameter("V")
                            .parameter(GenericMapType::new(
                                PrimitiveTypeKind::String,
                                GenericType::Parameter("V")
                            ))
                            .parameter(array_string)
                            .ret(PrimitiveTypeKind::Boolean)
                            .build(),
                        FunctionSignature::builder()
                            .type_parameter("S", StructConstraint)
                            .parameter(GenericType::Parameter("S"))
                            .parameter(array_string)
                            .ret(PrimitiveTypeKind::Boolean)
                            .build(),
                        FunctionSignature::builder()
                            .parameter(Type::Object)
                            .parameter(array_string)
                            .ret(PrimitiveTypeKind::Boolean)
                            .build(),
                    ]
                )
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
                    SupportedVersion::V1(V1::Two),
                    FunctionSignature::builder()
                        .type_parameter("K", RequiredPrimitiveTypeConstraint)
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
                    SupportedVersion::V1(V1::One),
                    FunctionSignature::builder()
                        .type_parameter("K", RequiredPrimitiveTypeConstraint)
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
                    SupportedVersion::V1(V1::Zero),
                    FunctionSignature::builder()
                        .type_parameter("X", OptionalTypeConstraint)
                        .parameter(GenericType::Parameter("X"))
                        .ret(PrimitiveTypeKind::Boolean)
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
                PolymorphicFunction::new(
                    SupportedVersion::V1(V1::Zero),
                    vec![
                        FunctionSignature::builder()
                            .any_type_parameter("X")
                            .parameter(GenericArrayType::new(GenericType::Parameter("X")))
                            .ret(PrimitiveTypeKind::Integer)
                            .build(),
                        FunctionSignature::builder()
                            .any_type_parameter("K")
                            .any_type_parameter("V")
                            .parameter(GenericMapType::new(
                                GenericType::Parameter("K"),
                                GenericType::Parameter("V")
                            ))
                            .ret(PrimitiveTypeKind::Integer)
                            .build(),
                        FunctionSignature::builder()
                            .parameter(Type::Object)
                            .ret(PrimitiveTypeKind::Integer)
                            .build(),
                        FunctionSignature::builder()
                            .parameter(PrimitiveTypeKind::String)
                            .ret(PrimitiveTypeKind::Integer)
                            .build(),
                    ]
                )
                .into(),
            )
            .is_none()
    );

    StandardLibrary {
        types,
        functions,
        array_string,
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
                    signatures.push(format!(
                        "{name}{sig}",
                        sig = f.signature.display(STDLIB.types(), &params)
                    ));
                }
                Function::Polymorphic(f) => {
                    for signature in &f.signatures {
                        let params = TypeParameters::new(&signature.type_parameters);
                        signatures.push(format!(
                            "{name}{sig}",
                            sig = signature.display(STDLIB.types(), &params)
                        ));
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
                "write_tsv(Array[S]) -> File where `S`: any structure",
                "write_tsv(Array[Array[String]], Boolean, Array[String]) -> File",
                "write_tsv(Array[S], Boolean, Array[String]) -> File where `S`: any structure",
                "read_map(File) -> Map[String, String]",
                "write_map(Map[String, String]) -> File",
                "read_json(File) -> Union",
                "write_json(X) -> File where `X`: any JSON-serializable type",
                "read_object(File) -> Object",
                "read_objects(File) -> Array[Object]",
                "write_object(Object) -> File",
                "write_object(S) -> File where `S`: any structure",
                "write_objects(Array[Object]) -> File",
                "write_objects(Array[S]) -> File where `S`: any structure",
                "prefix(String, Array[P]) -> Array[String] where `P`: any required primitive type",
                "suffix(String, Array[P]) -> Array[String] where `P`: any required primitive type",
                "quote(Array[P]) -> Array[String] where `P`: any required primitive type",
                "squote(Array[P]) -> Array[String] where `P`: any required primitive type",
                "sep(String, Array[P]) -> String where `P`: any required primitive type",
                "range(Int) -> Array[Int]",
                "transpose(Array[Array[X]]) -> Array[Array[X]]",
                "cross(Array[X], Array[Y]) -> Array[Pair[X, Y]]",
                "zip(Array[X], Array[Y]) -> Array[Pair[X, Y]]",
                "unzip(Array[Pair[X, Y]]) -> Pair[Array[X], Array[Y]]",
                "contains(Array[P], P) -> Boolean where `P`: any primitive type",
                "chunk(Array[X], Int) -> Array[Array[X]]",
                "flatten(Array[Array[X]]) -> Array[X]",
                "select_first(Array[X]+) -> X where `X`: any optional type",
                "select_first(Array[X], <X>) -> X where `X`: any optional type",
                "select_all(Array[X]) -> Array[X] where `X`: any optional type",
                "as_pairs(Map[K, V]) -> Array[Pair[K, V]] where `K`: any required primitive type",
                "as_map(Array[Pair[K, V]]) -> Map[K, V] where `K`: any required primitive type",
                "keys(Map[K, V]) -> Array[K] where `K`: any required primitive type",
                "keys(S) -> Array[String] where `S`: any structure",
                "keys(Object) -> Array[String]",
                "contains_key(Map[K, V], K) -> Boolean where `K`: any required primitive type",
                "contains_key(Object, String) -> Boolean",
                "contains_key(Map[String, V], Array[String]) -> Boolean",
                "contains_key(S, Array[String]) -> Boolean where `S`: any structure",
                "contains_key(Object, Array[String]) -> Boolean",
                "values(Map[K, V]) -> Array[V] where `K`: any required primitive type",
                "collect_by_key(Array[Pair[K, V]]) -> Map[K, Array[V]] where `K`: any required \
                 primitive type",
                "defined(X) -> Boolean where `X`: any optional type",
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

        let mut types = Types::new();
        let e = f.bind(&mut types, &[]).expect_err("bind should fail");
        assert_eq!(e, FunctionBindError::TooFewArguments(1));

        let e = f
            .bind(
                &mut types,
                &[
                    PrimitiveTypeKind::String.into(),
                    PrimitiveTypeKind::Boolean.into(),
                ],
            )
            .expect_err("bind should fail");
        assert_eq!(e, FunctionBindError::TooManyArguments(1));

        // Check for a string argument (should be a type mismatch)
        let e = f
            .bind(&mut types, &[PrimitiveTypeKind::String.into()])
            .expect_err("bind should fail");
        assert_eq!(
            e,
            FunctionBindError::ArgumentTypeMismatch {
                index: 0,
                expected: "`Float`".into()
            }
        );

        // Check for Union (i.e. indeterminate)
        let ty = f
            .bind(&mut types, &[Type::Union])
            .expect("bind should succeed");
        assert_eq!(ty.display(&types).to_string(), "Int");

        // Check for a float argument
        let ty = f
            .bind(&mut types, &[PrimitiveTypeKind::Float.into()])
            .expect("bind should succeed");
        assert_eq!(ty.display(&types).to_string(), "Int");

        // Check for an integer argument (should coerce)
        let ty = f
            .bind(&mut types, &[PrimitiveTypeKind::Integer.into()])
            .expect("bind should succeed");
        assert_eq!(ty.display(&types).to_string(), "Int");
    }

    #[test]
    fn it_binds_a_generic_function() {
        let f = STDLIB.function("values").expect("should have function");
        assert_eq!(f.minimum_version(), SupportedVersion::V1(V1::Two));

        let mut types = Types::new();
        let e = f.bind(&mut types, &[]).expect_err("bind should fail");
        assert_eq!(e, FunctionBindError::TooFewArguments(1));

        let e = f
            .bind(
                &mut types,
                &[
                    PrimitiveTypeKind::String.into(),
                    PrimitiveTypeKind::Boolean.into(),
                ],
            )
            .expect_err("bind should fail");
        assert_eq!(e, FunctionBindError::TooManyArguments(1));

        // Check for a string argument (should be a type mismatch)
        let e = f
            .bind(&mut types, &[PrimitiveTypeKind::String.into()])
            .expect_err("bind should fail");
        assert_eq!(
            e,
            FunctionBindError::ArgumentTypeMismatch {
                index: 0,
                expected: "`Map[K, V]` where `K`: any required primitive type".into()
            }
        );

        // Check for Union (i.e. indeterminate)
        let ty = f
            .bind(&mut types, &[Type::Union])
            .expect("bind should succeed");
        assert_eq!(ty.display(&types).to_string(), "Union");

        // Check for a Map[String, String]
        let ty = types.add_map(MapType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::String,
        ));
        let ty = f.bind(&mut types, &[ty]).expect("bind should succeed");
        assert_eq!(ty.display(&types).to_string(), "Array[String]");

        // Check for a Map[String, Object]
        let ty = types.add_map(MapType::new(PrimitiveTypeKind::String, Type::Object));
        let ty = f.bind(&mut types, &[ty]).expect("bind should succeed");
        assert_eq!(ty.display(&types).to_string(), "Array[Object]");

        // Check for a map with an optional primitive type
        let ty = types.add_map(MapType::new(
            PrimitiveType::optional(PrimitiveTypeKind::String),
            PrimitiveTypeKind::Boolean,
        ));
        let e = f.bind(&mut types, &[ty]).expect_err("bind should fail");
        assert_eq!(
            e,
            FunctionBindError::ArgumentTypeMismatch {
                index: 0,
                expected: "`Map[K, Boolean]` where `K`: any required primitive type".into()
            }
        );
    }

    #[test]
    fn it_removes_qualifiers() {
        let f = STDLIB.function("select_all").expect("should have function");
        assert_eq!(f.minimum_version(), SupportedVersion::V1(V1::Zero));

        let mut types = Types::new();

        // Check for a Array[String] (type mismatch due to constraint)
        let array_string = types.add_array(ArrayType::new(PrimitiveTypeKind::String));
        let e = f
            .bind(&mut types, &[array_string])
            .expect_err("bind should fail");
        assert_eq!(
            e,
            FunctionBindError::ArgumentTypeMismatch {
                index: 0,
                expected: "`Array[X]` where `X`: any optional type".into()
            }
        );

        // Check for a Array[String?] -> Array[String]
        let array_optional_string = types.add_array(ArrayType::new(PrimitiveType::optional(
            PrimitiveTypeKind::String,
        )));
        let ty = f
            .bind(&mut types, &[array_optional_string])
            .expect("bind should succeed");
        assert_eq!(ty.display(&types).to_string(), "Array[String]");

        // Check for Union (i.e. indeterminate)
        let ty = f
            .bind(&mut types, &[Type::Union])
            .expect("bind should succeed");
        assert_eq!(ty.display(&types).to_string(), "Union");

        // Check for a Array[Array[String]?] -> Array[Array[String]]
        let array_string = types
            .add_array(ArrayType::new(PrimitiveTypeKind::String))
            .optional();
        let array_array_string = types.add_array(ArrayType::new(array_string));
        let ty = f
            .bind(&mut types, &[array_array_string])
            .expect("bind should succeed");
        assert_eq!(ty.display(&types).to_string(), "Array[Array[String]]");
    }

    #[test]
    fn it_binds_concrete_overloads() {
        let f = STDLIB.function("max").expect("should have function");
        assert_eq!(f.minimum_version(), SupportedVersion::V1(V1::One));

        let mut types = Types::new();

        let e = f.bind(&mut types, &[]).expect_err("bind should fail");
        assert_eq!(e, FunctionBindError::TooFewArguments(2));

        let e = f
            .bind(
                &mut types,
                &[
                    PrimitiveTypeKind::String.into(),
                    PrimitiveTypeKind::Boolean.into(),
                    PrimitiveTypeKind::File.into(),
                ],
            )
            .expect_err("bind should fail");
        assert_eq!(e, FunctionBindError::TooManyArguments(2));

        // Check for `(Int, Int)`
        let ty = f
            .bind(
                &mut types,
                &[
                    PrimitiveTypeKind::Integer.into(),
                    PrimitiveTypeKind::Integer.into(),
                ],
            )
            .expect("binding should succeed");
        assert_eq!(ty.display(&types).to_string(), "Int");

        // Check for `(Int, Float)`
        let ty = f
            .bind(
                &mut types,
                &[
                    PrimitiveTypeKind::Integer.into(),
                    PrimitiveTypeKind::Float.into(),
                ],
            )
            .expect("binding should succeed");
        assert_eq!(ty.display(&types).to_string(), "Float");

        // Check for `(Float, Int)`
        let ty = f
            .bind(
                &mut types,
                &[
                    PrimitiveTypeKind::Float.into(),
                    PrimitiveTypeKind::Integer.into(),
                ],
            )
            .expect("binding should succeed");
        assert_eq!(ty.display(&types).to_string(), "Float");

        // Check for `(Float, Float)`
        let ty = f
            .bind(
                &mut types,
                &[
                    PrimitiveTypeKind::Float.into(),
                    PrimitiveTypeKind::Float.into(),
                ],
            )
            .expect("binding should succeed");
        assert_eq!(ty.display(&types).to_string(), "Float");

        // Check for `(String, Int)`
        let e = f
            .bind(
                &mut types,
                &[
                    PrimitiveTypeKind::String.into(),
                    PrimitiveTypeKind::Integer.into(),
                ],
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
                &mut types,
                &[
                    PrimitiveTypeKind::Integer.into(),
                    PrimitiveTypeKind::String.into(),
                ],
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
                &mut types,
                &[
                    PrimitiveTypeKind::String.into(),
                    PrimitiveTypeKind::Float.into(),
                ],
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
                &mut types,
                &[
                    PrimitiveTypeKind::Float.into(),
                    PrimitiveTypeKind::String.into(),
                ],
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

        let mut types = Types::default();
        let e = f.bind(&mut types, &[]).expect_err("bind should fail");
        assert_eq!(e, FunctionBindError::TooFewArguments(1));

        let e = f
            .bind(
                &mut types,
                &[
                    PrimitiveTypeKind::String.into(),
                    PrimitiveTypeKind::Boolean.into(),
                    PrimitiveTypeKind::File.into(),
                ],
            )
            .expect_err("bind should fail");
        assert_eq!(e, FunctionBindError::TooManyArguments(2));

        // Check `Int`
        let e = f
            .bind(&mut types, &[PrimitiveTypeKind::Integer.into()])
            .expect_err("binding should fail");
        assert_eq!(
            e,
            FunctionBindError::ArgumentTypeMismatch {
                index: 0,
                expected: "`Array[X]+` where `X`: any optional type or `Array[X]` where `X`: any \
                           optional type"
                    .into()
            }
        );

        // Check `Array[String?]+`
        let array = types.add_array(ArrayType::non_empty(PrimitiveType::optional(
            PrimitiveTypeKind::String,
        )));
        let ty = f
            .bind(&mut types, &[array])
            .expect("binding should succeed");
        assert_eq!(ty.display(&types).to_string(), "String");

        // Check (`Array[String?]+`, `String`)
        let ty = f
            .bind(&mut types, &[array, PrimitiveTypeKind::String.into()])
            .expect("binding should succeed");
        assert_eq!(ty.display(&types).to_string(), "String");

        // Check (`Array[String?]+`, `Int`)
        let e = f
            .bind(&mut types, &[array, PrimitiveTypeKind::Integer.into()])
            .expect_err("binding should fail");
        assert_eq!(
            e,
            FunctionBindError::ArgumentTypeMismatch {
                index: 1,
                expected: "`String`".into()
            }
        );

        // Check `Array[String?]`
        let array = types.add_array(ArrayType::new(PrimitiveType::optional(
            PrimitiveTypeKind::String,
        )));
        let ty = f
            .bind(&mut types, &[array])
            .expect("binding should succeed");
        assert_eq!(ty.display(&types).to_string(), "String");

        // Check (`Array[String?]`, `String`)
        let ty = f
            .bind(&mut types, &[array, PrimitiveTypeKind::String.into()])
            .expect("binding should succeed");
        assert_eq!(ty.display(&types).to_string(), "String");

        // Check (`Array[String?]`, `Int`)
        let e = f
            .bind(&mut types, &[array, PrimitiveTypeKind::Integer.into()])
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
