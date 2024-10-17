//! V1 AST representation for declarations.

use std::fmt;

use super::Expr;
use crate::AstNode;
use crate::AstToken;
use crate::Ident;
use crate::SyntaxKind;
use crate::SyntaxNode;
use crate::WorkflowDescriptionLanguage;
use crate::support;
use crate::token;

/// Represents a `Map` type.
#[derive(Clone, Debug, Eq)]
pub struct MapType(SyntaxNode);

impl MapType {
    /// Gets the key and value types of the `Map`.
    pub fn types(&self) -> (PrimitiveType, Type) {
        let mut children = self.0.children().filter_map(Type::cast);
        let key = children
            .next()
            .expect("map should have a key type")
            .unwrap_primitive_type();
        let value = children.next().expect("map should have a value type");
        (key, value)
    }

    /// Determines if the type is optional.
    pub fn is_optional(&self) -> bool {
        matches!(
            self.0.last_token().map(|t| t.kind()),
            Some(SyntaxKind::QuestionMark)
        )
    }
}

impl PartialEq for MapType {
    fn eq(&self, other: &Self) -> bool {
        self.is_optional() == other.is_optional() && self.types() == other.types()
    }
}

impl AstNode for MapType {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::MapTypeNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::MapTypeNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl fmt::Display for MapType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (key, value) = self.types();
        write!(
            f,
            "Map[{key}, {value}]{o}",
            o = if self.is_optional() { "?" } else { "" }
        )
    }
}

/// Represents an `Array` type.
#[derive(Clone, Debug, Eq)]
pub struct ArrayType(SyntaxNode);

impl ArrayType {
    /// Gets the element type of the array.
    pub fn element_type(&self) -> Type {
        Type::child(&self.0).expect("array should have an element type")
    }

    /// Determines if the type has the "non-empty" qualifier.
    pub fn is_non_empty(&self) -> bool {
        support::token(&self.0, SyntaxKind::Plus).is_some()
    }

    /// Determines if the type is optional.
    pub fn is_optional(&self) -> bool {
        matches!(
            self.0.last_token().map(|t| t.kind()),
            Some(SyntaxKind::QuestionMark)
        )
    }
}

impl PartialEq for ArrayType {
    fn eq(&self, other: &Self) -> bool {
        self.is_optional() == other.is_optional()
            && self.is_non_empty() == other.is_non_empty()
            && self.element_type() == other.element_type()
    }
}

impl AstNode for ArrayType {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::ArrayTypeNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::ArrayTypeNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl fmt::Display for ArrayType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Array[{ty}]{p}{o}",
            ty = self.element_type(),
            p = if self.is_non_empty() { "+" } else { "" },
            o = if self.is_optional() { "?" } else { "" }
        )
    }
}

/// Represents a `Pair` type.
#[derive(Clone, Debug, Eq)]
pub struct PairType(SyntaxNode);

impl PairType {
    /// Gets the first and second types of the `Pair`.
    pub fn types(&self) -> (Type, Type) {
        let mut children = self.0.children().filter_map(Type::cast);
        let left = children.next().expect("pair should have a left type");
        let right = children.next().expect("pair should have a right type");
        (left, right)
    }

    /// Determines if the type is optional.
    pub fn is_optional(&self) -> bool {
        matches!(
            self.0.last_token().map(|t| t.kind()),
            Some(SyntaxKind::QuestionMark)
        )
    }
}

impl PartialEq for PairType {
    fn eq(&self, other: &Self) -> bool {
        self.is_optional() == other.is_optional() && self.types() == other.types()
    }
}

impl AstNode for PairType {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::PairTypeNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::PairTypeNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl fmt::Display for PairType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (left, right) = self.types();
        write!(
            f,
            "Pair[{left}, {right}]{o}",
            o = if self.is_optional() { "?" } else { "" }
        )
    }
}

/// Represents a `Object` type.
#[derive(Clone, Debug, Eq)]
pub struct ObjectType(SyntaxNode);

impl ObjectType {
    /// Determines if the type is optional.
    pub fn is_optional(&self) -> bool {
        matches!(
            self.0.last_token().map(|t| t.kind()),
            Some(SyntaxKind::QuestionMark)
        )
    }
}

impl PartialEq for ObjectType {
    fn eq(&self, other: &Self) -> bool {
        self.is_optional() == other.is_optional()
    }
}

impl AstNode for ObjectType {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::ObjectTypeNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::ObjectTypeNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl fmt::Display for ObjectType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Object{o}",
            o = if self.is_optional() { "?" } else { "" }
        )
    }
}

/// Represents a reference to a type.
#[derive(Clone, Debug, Eq)]
pub struct TypeRef(SyntaxNode);

impl TypeRef {
    /// Gets the name of the type reference.
    pub fn name(&self) -> Ident {
        token(&self.0).expect("type reference should have a name")
    }

    /// Determines if the type is optional.
    pub fn is_optional(&self) -> bool {
        matches!(
            self.0.last_token().map(|t| t.kind()),
            Some(SyntaxKind::QuestionMark)
        )
    }
}

impl PartialEq for TypeRef {
    fn eq(&self, other: &Self) -> bool {
        self.is_optional() == other.is_optional() && self.name().as_str() == other.name().as_str()
    }
}

impl AstNode for TypeRef {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::TypeRefNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::TypeRefNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl fmt::Display for TypeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{n}{o}",
            n = self.name().as_str(),
            o = if self.is_optional() { "?" } else { "" }
        )
    }
}

/// Represents a kind of primitive type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PrimitiveTypeKind {
    /// The primitive is a `Boolean`.
    Boolean,
    /// The primitive is an `Int`.
    Integer,
    /// The primitive is a `Float`.
    Float,
    /// The primitive is a `String`.
    String,
    /// The primitive is a `File`.
    File,
    /// The primitive is a `Directory`
    Directory,
}

/// Represents a primitive type.
#[derive(Clone, Debug, Eq)]
pub struct PrimitiveType(SyntaxNode);

impl PrimitiveType {
    /// Gets the kind of the primitive type.
    pub fn kind(&self) -> PrimitiveTypeKind {
        self.0
            .children_with_tokens()
            .find_map(|t| match t.kind() {
                SyntaxKind::BooleanTypeKeyword => Some(PrimitiveTypeKind::Boolean),
                SyntaxKind::IntTypeKeyword => Some(PrimitiveTypeKind::Integer),
                SyntaxKind::FloatTypeKeyword => Some(PrimitiveTypeKind::Float),
                SyntaxKind::StringTypeKeyword => Some(PrimitiveTypeKind::String),
                SyntaxKind::FileTypeKeyword => Some(PrimitiveTypeKind::File),
                SyntaxKind::DirectoryTypeKeyword => Some(PrimitiveTypeKind::Directory),
                _ => None,
            })
            .expect("type should have a kind")
    }

    /// Determines if the type is optional.
    pub fn is_optional(&self) -> bool {
        matches!(
            self.0.last_token().map(|t| t.kind()),
            Some(SyntaxKind::QuestionMark)
        )
    }
}

impl PartialEq for PrimitiveType {
    fn eq(&self, other: &Self) -> bool {
        self.kind() == other.kind()
    }
}

impl AstNode for PrimitiveType {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::PrimitiveTypeNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::PrimitiveTypeNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl fmt::Display for PrimitiveType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind() {
            PrimitiveTypeKind::Boolean => write!(f, "Boolean")?,
            PrimitiveTypeKind::Integer => write!(f, "Int")?,
            PrimitiveTypeKind::Float => write!(f, "Float")?,
            PrimitiveTypeKind::String => write!(f, "String")?,
            PrimitiveTypeKind::File => write!(f, "File")?,
            PrimitiveTypeKind::Directory => write!(f, "Directory")?,
        }

        if self.is_optional() {
            write!(f, "?")
        } else {
            Ok(())
        }
    }
}

/// Represents a type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    /// The type is a map.
    Map(MapType),
    /// The type is an array.
    Array(ArrayType),
    /// The type is a pair.
    Pair(PairType),
    /// The type is an object.
    Object(ObjectType),
    /// The type is a reference to custom type.
    Ref(TypeRef),
    /// The type is a primitive.
    Primitive(PrimitiveType),
}

impl Type {
    /// Returns whether or not a [`SyntaxKind`] is able to be cast to any of the
    /// underlying members within the [`Type`].
    pub fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(
            kind,
            SyntaxKind::MapTypeNode
                | SyntaxKind::ArrayTypeNode
                | SyntaxKind::PairTypeNode
                | SyntaxKind::ObjectTypeNode
                | SyntaxKind::TypeRefNode
                | SyntaxKind::PrimitiveTypeNode
        )
    }

    /// Attempts to cast the [`SyntaxNode`] to any of the underlying members
    /// within the [`Type`].
    pub fn cast(syntax: SyntaxNode) -> Option<Self> {
        match syntax.kind() {
            SyntaxKind::MapTypeNode => {
                Some(Self::Map(MapType::cast(syntax).expect("map type to cast")))
            }
            SyntaxKind::ArrayTypeNode => Some(Self::Array(
                ArrayType::cast(syntax).expect("array type to cast"),
            )),
            SyntaxKind::PairTypeNode => Some(Self::Pair(
                PairType::cast(syntax).expect("pair type to cast"),
            )),
            SyntaxKind::ObjectTypeNode => Some(Self::Object(
                ObjectType::cast(syntax).expect("object type to cast"),
            )),
            SyntaxKind::TypeRefNode => {
                Some(Self::Ref(TypeRef::cast(syntax).expect("type ref to cast")))
            }
            SyntaxKind::PrimitiveTypeNode => Some(Self::Primitive(
                PrimitiveType::cast(syntax).expect("primitive type to cast"),
            )),
            _ => None,
        }
    }

    /// Gets a reference to the underlying [`SyntaxNode`].
    pub fn syntax(&self) -> &SyntaxNode {
        match self {
            Type::Map(element) => element.syntax(),
            Type::Array(element) => element.syntax(),
            Type::Pair(element) => element.syntax(),
            Type::Object(element) => element.syntax(),
            Type::Ref(element) => element.syntax(),
            Type::Primitive(element) => element.syntax(),
        }
    }

    /// Determines if the type is optional.
    pub fn is_optional(&self) -> bool {
        match self {
            Self::Map(m) => m.is_optional(),
            Self::Array(a) => a.is_optional(),
            Self::Pair(p) => p.is_optional(),
            Self::Object(o) => o.is_optional(),
            Self::Ref(r) => r.is_optional(),
            Self::Primitive(p) => p.is_optional(),
        }
    }

    /// Attempts to get a reference to the inner [`MapType`].
    ///
    /// * If `self` is a [`Type::Map`], then a reference to the inner
    ///   [`MapType`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_map_type(&self) -> Option<&MapType> {
        match self {
            Self::Map(map) => Some(map),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`MapType`].
    ///
    /// * If `self` is a [`Type::Map`], then the inner [`MapType`] is returned
    ///   wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_map_type(self) -> Option<MapType> {
        match self {
            Self::Map(map) => Some(map),
            _ => None,
        }
    }

    /// Unwraps the type into a map type.
    ///
    /// # Panics
    ///
    /// Panics if the type is not a map type.
    pub fn unwrap_map_type(self) -> MapType {
        match self {
            Self::Map(ty) => ty,
            _ => panic!("not a map type"),
        }
    }

    /// Attempts to get a reference to the inner [`ArrayType`].
    ///
    /// * If `self` is a [`Type::Array`], then a reference to the inner
    ///   [`ArrayType`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_array_type(&self) -> Option<&ArrayType> {
        match self {
            Self::Array(array) => Some(array),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`ArrayType`].
    ///
    /// * If `self` is a [`Type::Array`], then the inner [`ArrayType`] is
    ///   returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_array_type(self) -> Option<ArrayType> {
        match self {
            Self::Array(array) => Some(array),
            _ => None,
        }
    }

    /// Unwraps the type into an array type.
    ///
    /// # Panics
    ///
    /// Panics if the type is not an array type.
    pub fn unwrap_array_type(self) -> ArrayType {
        match self {
            Self::Array(ty) => ty,
            _ => panic!("not an array type"),
        }
    }

    /// Attempts to get a reference to the inner [`PairType`].
    ///
    /// * If `self` is a [`Type::Pair`], then a reference to the inner
    ///   [`PairType`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_pair_type(&self) -> Option<&PairType> {
        match self {
            Self::Pair(pair) => Some(pair),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`PairType`].
    ///
    /// * If `self` is a [`Type::Pair`], then the inner [`PairType`] is returned
    ///   wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_pair_type(self) -> Option<PairType> {
        match self {
            Self::Pair(pair) => Some(pair),
            _ => None,
        }
    }

    /// Unwraps the type into a pair type.
    ///
    /// # Panics
    ///
    /// Panics if the type is not a pair type.
    pub fn unwrap_pair_type(self) -> PairType {
        match self {
            Self::Pair(ty) => ty,
            _ => panic!("not a pair type"),
        }
    }

    /// Attempts to get a reference to the inner [`ObjectType`].
    ///
    /// * If `self` is a [`Type::Object`], then a reference to the inner
    ///   [`ObjectType`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_object_type(&self) -> Option<&ObjectType> {
        match self {
            Self::Object(object) => Some(object),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`ObjectType`].
    ///
    /// * If `self` is a [`Type::Object`], then the inner [`ObjectType`] is
    ///   returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_object_type(self) -> Option<ObjectType> {
        match self {
            Self::Object(object) => Some(object),
            _ => None,
        }
    }

    /// Unwraps the type into an object type.
    ///
    /// # Panics
    ///
    /// Panics if the type is not an object type.
    pub fn unwrap_object_type(self) -> ObjectType {
        match self {
            Self::Object(ty) => ty,
            _ => panic!("not an object type"),
        }
    }

    /// Attempts to get a reference to the inner [`TypeRef`].
    ///
    /// * If `self` is a [`Type::Ref`], then a reference to the inner
    ///   [`TypeRef`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_type_ref(&self) -> Option<&TypeRef> {
        match self {
            Self::Ref(type_ref) => Some(type_ref),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`TypeRef`].
    ///
    /// * If `self` is a [`Type::Ref`], then the inner [`TypeRef`] is returned
    ///   wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_type_ref(self) -> Option<TypeRef> {
        match self {
            Self::Ref(type_ref) => Some(type_ref),
            _ => None,
        }
    }

    /// Unwraps the type into a type reference.
    ///
    /// # Panics
    ///
    /// Panics if the type is not a type reference.
    pub fn unwrap_type_ref(self) -> TypeRef {
        match self {
            Self::Ref(r) => r,
            _ => panic!("not a type reference"),
        }
    }

    /// Attempts to get a reference to the inner [`PrimitiveType`].
    ///
    /// * If `self` is a [`Type::Primitive`], then a reference to the inner
    ///   [`PrimitiveType`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_primitive_type(&self) -> Option<&PrimitiveType> {
        match self {
            Self::Primitive(primitive) => Some(primitive),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`PrimitiveType`].
    ///
    /// * If `self` is a [`Type::Primitive`], then the inner [`PrimitiveType`]
    ///   is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_primitive_type(self) -> Option<PrimitiveType> {
        match self {
            Self::Primitive(primitive) => Some(primitive),
            _ => None,
        }
    }

    /// Unwraps the type into a primitive type.
    ///
    /// # Panics
    ///
    /// Panics if the type is not a primitive type.
    pub fn unwrap_primitive_type(self) -> PrimitiveType {
        match self {
            Self::Primitive(ty) => ty,
            _ => panic!("not a primitive type"),
        }
    }

    /// Finds the first child that can be cast to a [`Type`].
    ///
    /// This is meant to emulate the functionality of
    /// [`rowan::ast::support::child`] without requiring [`Type`] to implement
    /// the `AstNode` trait.
    pub fn child(syntax: &SyntaxNode) -> Option<Self> {
        syntax.children().find_map(Self::cast)
    }

    /// Finds all children that can be cast to a [`Type`].
    ///
    /// This is meant to emulate the functionality of
    /// [`rowan::ast::support::children`] without requiring [`Type`] to
    /// implement the `AstNode` trait.
    pub fn children(syntax: &SyntaxNode) -> impl Iterator<Item = Type> + use<> {
        syntax.children().filter_map(Self::cast)
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Map(m) => m.fmt(f),
            Type::Array(a) => a.fmt(f),
            Type::Pair(p) => p.fmt(f),
            Type::Object(o) => o.fmt(f),
            Type::Ref(r) => r.fmt(f),
            Type::Primitive(p) => p.fmt(f),
        }
    }
}

/// Represents an unbound declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnboundDecl(pub(crate) SyntaxNode);

impl UnboundDecl {
    /// Gets the type of the declaration.
    pub fn ty(&self) -> Type {
        Type::child(&self.0).expect("unbound declaration should have a type")
    }

    /// Gets the name of the declaration.
    pub fn name(&self) -> Ident {
        token(&self.0).expect("unbound declaration should have a name")
    }
}

impl AstNode for UnboundDecl {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::UnboundDeclNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::UnboundDeclNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents a bound declaration in a task or workflow definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundDecl(pub(crate) SyntaxNode);

impl BoundDecl {
    /// Gets the type of the declaration.
    pub fn ty(&self) -> Type {
        Type::child(&self.0).expect("bound declaration should have a type")
    }

    /// Gets the name of the declaration.
    pub fn name(&self) -> Ident {
        token(&self.0).expect("bound declaration should have a name")
    }

    /// Gets the expression the declaration is bound to.
    pub fn expr(&self) -> Expr {
        Expr::child(&self.0).expect("bound declaration should have an expression")
    }
}

impl AstNode for BoundDecl {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::BoundDeclNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::BoundDeclNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents a declaration in an input section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decl {
    /// The declaration is bound.
    Bound(BoundDecl),
    /// The declaration is unbound.
    Unbound(UnboundDecl),
}

impl Decl {
    /// Returns whether or not a [`SyntaxKind`] is able to be cast to any of the
    /// underlying members within the [`Decl`].
    pub fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::BoundDeclNode || kind == SyntaxKind::UnboundDeclNode
    }

    /// Attempts to cast the [`SyntaxNode`] to any of the underlying members
    /// within the [`Decl`].
    pub fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::BoundDeclNode => Some(Self::Bound(
                BoundDecl::cast(syntax).expect("bound decl to cast"),
            )),
            SyntaxKind::UnboundDeclNode => Some(Self::Unbound(
                UnboundDecl::cast(syntax).expect("unbound decl to cast"),
            )),
            _ => None,
        }
    }

    /// Gets a reference to the underlying [`SyntaxNode`].
    pub fn syntax(&self) -> &SyntaxNode {
        match self {
            Self::Bound(element) => element.syntax(),
            Self::Unbound(element) => element.syntax(),
        }
    }

    /// Gets the type of the declaration.
    pub fn ty(&self) -> Type {
        match self {
            Self::Bound(d) => d.ty(),
            Self::Unbound(d) => d.ty(),
        }
    }

    /// Gets the name of the declaration.
    pub fn name(&self) -> Ident {
        match self {
            Self::Bound(d) => d.name(),
            Self::Unbound(d) => d.name(),
        }
    }

    /// Gets the expression of the declaration.
    ///
    /// Returns `None` for unbound declarations.
    pub fn expr(&self) -> Option<Expr> {
        match self {
            Self::Bound(d) => Some(d.expr()),
            Self::Unbound(_) => None,
        }
    }

    /// Attempts to get a reference to the inner [`BoundDecl`].
    ///
    /// * If `self` is a [`Decl::Bound`], then a reference to the inner
    ///   [`BoundDecl`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_bound_decl(&self) -> Option<&BoundDecl> {
        match self {
            Self::Bound(bound_decl) => Some(bound_decl),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`BoundDecl`].
    ///
    /// * If `self` is a [`Decl::Bound`], then the inner [`BoundDecl`] is
    ///   returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_bound_decl(self) -> Option<BoundDecl> {
        match self {
            Self::Bound(bound_decl) => Some(bound_decl),
            _ => None,
        }
    }

    /// Unwraps the declaration into a bound declaration.
    ///
    /// # Panics
    ///
    /// Panics if the declaration is not a bound declaration.
    pub fn unwrap_bound_decl(self) -> BoundDecl {
        match self {
            Self::Bound(decl) => decl,
            _ => panic!("not a bound declaration"),
        }
    }

    /// Attempts to get a reference to the inner [`UnboundDecl`].
    ///
    /// * If `self` is a [`Decl::Unbound`], then a reference to the inner
    ///   [`UnboundDecl`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_unbound_decl(&self) -> Option<&UnboundDecl> {
        match self {
            Self::Unbound(unbound_decl) => Some(unbound_decl),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`UnboundDecl`].
    ///
    /// * If `self` is a [`Decl::Unbound`], then the inner [`UnboundDecl`] is
    ///   returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_unbound_decl(self) -> Option<UnboundDecl> {
        match self {
            Self::Unbound(unbound_decl) => Some(unbound_decl),
            _ => None,
        }
    }

    /// Unwraps the declaration into an unbound declaration.
    ///
    /// # Panics
    ///
    /// Panics if the declaration is not an unbound declaration.
    pub fn unwrap_unbound_decl(self) -> UnboundDecl {
        match self {
            Self::Unbound(decl) => decl,
            _ => panic!("not an unbound declaration"),
        }
    }

    /// Finds the first child that can be cast to a [`Decl`].
    ///
    /// This is meant to emulate the functionality of
    /// [`rowan::ast::support::child`] without requiring [`Decl`] to implement
    /// the `AstNode` trait.
    pub fn child(syntax: &SyntaxNode) -> Option<Self> {
        syntax.children().find_map(Self::cast)
    }

    /// Finds all children that can be cast to a [`Decl`].
    ///
    /// This is meant to emulate the functionality of
    /// [`rowan::ast::support::children`] without requiring [`Decl`] to
    /// implement the `AstNode` trait.
    pub fn children(syntax: &SyntaxNode) -> impl Iterator<Item = Decl> + use<> {
        syntax.children().filter_map(Self::cast)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::Document;
    use crate::SupportedVersion;
    use crate::VisitReason;
    use crate::Visitor;

    #[test]
    fn decls() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    input {
        Boolean a
        Int b = 42
        Float? c = None
        String d
        File e = "foo.wdl"
        Map[Int, Int] f
        Array[String] g = []
        Pair[Boolean, Int] h
        Object i = object {}
        MyStruct j
        Directory k = "foo"
    }
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().as_str(), "test");

        // Inputs
        let input = tasks[0].input().expect("task should have an input section");
        let decls: Vec<_> = input.declarations().collect();
        assert_eq!(decls.len(), 11);

        // First input declaration
        let decl = decls[0].clone().unwrap_unbound_decl();
        assert_eq!(decl.ty().to_string(), "Boolean");
        assert_eq!(decl.name().as_str(), "a");

        // Second input declaration
        let decl = decls[1].clone().unwrap_bound_decl();
        assert_eq!(decl.ty().to_string(), "Int");
        assert_eq!(decl.name().as_str(), "b");
        assert_eq!(
            decl.expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            42
        );

        // Third input declaration
        let decl = decls[2].clone().unwrap_bound_decl();
        assert_eq!(decl.ty().to_string(), "Float?");
        assert_eq!(decl.name().as_str(), "c");
        decl.expr().unwrap_literal().unwrap_none();

        // Fourth input declaration
        let decl = decls[3].clone().unwrap_unbound_decl();
        assert_eq!(decl.ty().to_string(), "String");
        assert_eq!(decl.name().as_str(), "d");

        // Fifth input declaration
        let decl = decls[4].clone().unwrap_bound_decl();
        assert_eq!(decl.ty().to_string(), "File");
        assert_eq!(decl.name().as_str(), "e");
        assert_eq!(
            decl.expr()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .as_str(),
            "foo.wdl"
        );

        // Sixth input declaration
        let decl = decls[5].clone().unwrap_unbound_decl();
        assert_eq!(decl.ty().to_string(), "Map[Int, Int]");
        assert_eq!(decl.name().as_str(), "f");

        // Seventh input declaration
        let decl = decls[6].clone().unwrap_bound_decl();
        assert_eq!(decl.ty().to_string(), "Array[String]");
        assert_eq!(decl.name().as_str(), "g");
        assert_eq!(
            decl.expr()
                .unwrap_literal()
                .unwrap_array()
                .elements()
                .count(),
            0
        );

        // Eighth input declaration
        let decl = decls[7].clone().unwrap_unbound_decl();
        assert_eq!(decl.ty().to_string(), "Pair[Boolean, Int]");
        assert_eq!(decl.name().as_str(), "h");

        // Ninth input declaration
        let decl = decls[8].clone().unwrap_bound_decl();
        assert_eq!(decl.ty().to_string(), "Object");
        assert_eq!(decl.name().as_str(), "i");
        assert_eq!(
            decl.expr().unwrap_literal().unwrap_object().items().count(),
            0
        );

        // Tenth input declaration
        let decl = decls[9].clone().unwrap_unbound_decl();
        assert_eq!(decl.ty().to_string(), "MyStruct");
        assert_eq!(decl.name().as_str(), "j");

        // Eleventh input declaration
        let decl = decls[10].clone().unwrap_bound_decl();
        assert_eq!(decl.ty().to_string(), "Directory");
        assert_eq!(decl.name().as_str(), "k");
        assert_eq!(
            decl.expr()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .as_str(),
            "foo"
        );

        // Use a visitor to count the number of declarations
        #[derive(Default)]
        struct MyVisitor {
            bound: usize,
            unbound: usize,
        }

        impl Visitor for MyVisitor {
            type State = ();

            fn document(
                &mut self,
                _: &mut Self::State,
                _: VisitReason,
                _: &Document,
                _: SupportedVersion,
            ) {
            }

            fn bound_decl(&mut self, _: &mut Self::State, reason: VisitReason, _: &BoundDecl) {
                if reason == VisitReason::Enter {
                    self.bound += 1;
                }
            }

            fn unbound_decl(&mut self, _: &mut Self::State, reason: VisitReason, _: &UnboundDecl) {
                if reason == VisitReason::Enter {
                    self.unbound += 1;
                }
            }
        }

        let mut visitor = MyVisitor::default();
        document.visit(&mut (), &mut visitor);
        assert_eq!(visitor.bound, 6);
        assert_eq!(visitor.unbound, 5);
    }
}
