//! V1 AST representation for declarations.

use std::fmt;

use super::EnvKeyword;
use super::Expr;
use super::Plus;
use super::QuestionMark;
use crate::AstNode;
use crate::AstToken;
use crate::Ident;
use crate::SyntaxKind;
use crate::SyntaxNode;
use crate::TreeNode;
use crate::TreeToken;

/// Represents a `Map` type.
#[derive(Clone, Debug, Eq)]
pub struct MapType<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> MapType<N> {
    /// Gets the key and value types of the `Map`.
    pub fn types(&self) -> (PrimitiveType<N>, Type<N>) {
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

impl<N: TreeNode> PartialEq for MapType<N> {
    fn eq(&self, other: &Self) -> bool {
        self.is_optional() == other.is_optional() && self.types() == other.types()
    }
}

impl<N: TreeNode> AstNode<N> for MapType<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::MapTypeNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::MapTypeNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
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
pub struct ArrayType<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> ArrayType<N> {
    /// Gets the element type of the array.
    pub fn element_type(&self) -> Type<N> {
        Type::child(&self.0).expect("array should have an element type")
    }

    /// Determines if the type has the "non-empty" qualifier.
    pub fn is_non_empty(&self) -> bool {
        self.token::<Plus<N::Token>>().is_some()
    }

    /// Determines if the type is optional.
    pub fn is_optional(&self) -> bool {
        self.last_token::<QuestionMark<N::Token>>().is_some()
    }
}

impl<N: TreeNode> PartialEq for ArrayType<N> {
    fn eq(&self, other: &Self) -> bool {
        self.is_optional() == other.is_optional()
            && self.is_non_empty() == other.is_non_empty()
            && self.element_type() == other.element_type()
    }
}

impl<N: TreeNode> AstNode<N> for ArrayType<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ArrayTypeNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::ArrayTypeNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
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
pub struct PairType<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> PairType<N> {
    /// Gets the first and second types of the `Pair`.
    pub fn types(&self) -> (Type<N>, Type<N>) {
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

impl<N: TreeNode> PartialEq for PairType<N> {
    fn eq(&self, other: &Self) -> bool {
        self.is_optional() == other.is_optional() && self.types() == other.types()
    }
}

impl<N: TreeNode> AstNode<N> for PairType<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PairTypeNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::PairTypeNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
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
pub struct ObjectType<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> ObjectType<N> {
    /// Determines if the type is optional.
    pub fn is_optional(&self) -> bool {
        matches!(
            self.0.last_token().map(|t| t.kind()),
            Some(SyntaxKind::QuestionMark)
        )
    }
}

impl<N: TreeNode> PartialEq for ObjectType<N> {
    fn eq(&self, other: &Self) -> bool {
        self.is_optional() == other.is_optional()
    }
}

impl<N: TreeNode> AstNode<N> for ObjectType<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ObjectTypeNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::ObjectTypeNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
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
pub struct TypeRef<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> TypeRef<N> {
    /// Gets the name of the type reference.
    pub fn name(&self) -> Ident<N::Token> {
        self.token().expect("type reference should have a name")
    }

    /// Determines if the type is optional.
    pub fn is_optional(&self) -> bool {
        matches!(
            self.0.last_token().map(|t| t.kind()),
            Some(SyntaxKind::QuestionMark)
        )
    }
}

impl<N: TreeNode> PartialEq for TypeRef<N> {
    fn eq(&self, other: &Self) -> bool {
        self.is_optional() == other.is_optional() && self.name().text() == other.name().text()
    }
}

impl<N: TreeNode> AstNode<N> for TypeRef<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::TypeRefNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::TypeRefNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

impl fmt::Display for TypeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{n}{o}",
            n = self.name().text(),
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
pub struct PrimitiveType<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> PrimitiveType<N> {
    /// Gets the kind of the primitive type.
    pub fn kind(&self) -> PrimitiveTypeKind {
        self.0
            .children_with_tokens()
            .find_map(|c| {
                c.into_token().and_then(|t| match t.kind() {
                    SyntaxKind::BooleanTypeKeyword => Some(PrimitiveTypeKind::Boolean),
                    SyntaxKind::IntTypeKeyword => Some(PrimitiveTypeKind::Integer),
                    SyntaxKind::FloatTypeKeyword => Some(PrimitiveTypeKind::Float),
                    SyntaxKind::StringTypeKeyword => Some(PrimitiveTypeKind::String),
                    SyntaxKind::FileTypeKeyword => Some(PrimitiveTypeKind::File),
                    SyntaxKind::DirectoryTypeKeyword => Some(PrimitiveTypeKind::Directory),
                    _ => None,
                })
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

impl<N: TreeNode> PartialEq for PrimitiveType<N> {
    fn eq(&self, other: &Self) -> bool {
        self.kind() == other.kind()
    }
}

impl<N: TreeNode> AstNode<N> for PrimitiveType<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PrimitiveTypeNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::PrimitiveTypeNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
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
#[derive(Clone, Debug, Eq)]
pub enum Type<N: TreeNode = SyntaxNode> {
    /// The type is a map.
    Map(MapType<N>),
    /// The type is an array.
    Array(ArrayType<N>),
    /// The type is a pair.
    Pair(PairType<N>),
    /// The type is an object.
    Object(ObjectType<N>),
    /// The type is a reference to custom type.
    Ref(TypeRef<N>),
    /// The type is a primitive.
    Primitive(PrimitiveType<N>),
}

impl<N: TreeNode> Type<N> {
    //// Returns whether or not the given syntax kind can be cast to
    /// [`Type`].
    pub fn can_cast(kind: SyntaxKind) -> bool {
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

    /// Casts the given node to [`Type`].
    ///
    /// Returns `None` if the node cannot be cast.
    pub fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::MapTypeNode => {
                Some(Self::Map(MapType::cast(inner).expect("map type to cast")))
            }
            SyntaxKind::ArrayTypeNode => Some(Self::Array(
                ArrayType::cast(inner).expect("array type to cast"),
            )),
            SyntaxKind::PairTypeNode => Some(Self::Pair(
                PairType::cast(inner).expect("pair type to cast"),
            )),
            SyntaxKind::ObjectTypeNode => Some(Self::Object(
                ObjectType::cast(inner).expect("object type to cast"),
            )),
            SyntaxKind::TypeRefNode => {
                Some(Self::Ref(TypeRef::cast(inner).expect("type ref to cast")))
            }
            SyntaxKind::PrimitiveTypeNode => Some(Self::Primitive(
                PrimitiveType::cast(inner).expect("primitive type to cast"),
            )),
            _ => None,
        }
    }

    /// Gets a reference to the inner node.
    pub fn inner(&self) -> &N {
        match self {
            Self::Map(ty) => ty.inner(),
            Self::Array(ty) => ty.inner(),
            Self::Pair(ty) => ty.inner(),
            Self::Object(ty) => ty.inner(),
            Self::Ref(ty) => ty.inner(),
            Self::Primitive(ty) => ty.inner(),
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
    pub fn as_map_type(&self) -> Option<&MapType<N>> {
        match self {
            Self::Map(ty) => Some(ty),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`MapType`].
    ///
    /// * If `self` is a [`Type::Map`], then the inner [`MapType`] is returned
    ///   wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_map_type(self) -> Option<MapType<N>> {
        match self {
            Self::Map(ty) => Some(ty),
            _ => None,
        }
    }

    /// Unwraps the type into a map type.
    ///
    /// # Panics
    ///
    /// Panics if the type is not a map type.
    pub fn unwrap_map_type(self) -> MapType<N> {
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
    pub fn as_array_type(&self) -> Option<&ArrayType<N>> {
        match self {
            Self::Array(ty) => Some(ty),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`ArrayType`].
    ///
    /// * If `self` is a [`Type::Array`], then the inner [`ArrayType`] is
    ///   returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_array_type(self) -> Option<ArrayType<N>> {
        match self {
            Self::Array(ty) => Some(ty),
            _ => None,
        }
    }

    /// Unwraps the type into an array type.
    ///
    /// # Panics
    ///
    /// Panics if the type is not an array type.
    pub fn unwrap_array_type(self) -> ArrayType<N> {
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
    pub fn as_pair_type(&self) -> Option<&PairType<N>> {
        match self {
            Self::Pair(ty) => Some(ty),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`PairType`].
    ///
    /// * If `self` is a [`Type::Pair`], then the inner [`PairType`] is returned
    ///   wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_pair_type(self) -> Option<PairType<N>> {
        match self {
            Self::Pair(ty) => Some(ty),
            _ => None,
        }
    }

    /// Unwraps the type into a pair type.
    ///
    /// # Panics
    ///
    /// Panics if the type is not a pair type.
    pub fn unwrap_pair_type(self) -> PairType<N> {
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
    pub fn as_object_type(&self) -> Option<&ObjectType<N>> {
        match self {
            Self::Object(ty) => Some(ty),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`ObjectType`].
    ///
    /// * If `self` is a [`Type::Object`], then the inner [`ObjectType`] is
    ///   returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_object_type(self) -> Option<ObjectType<N>> {
        match self {
            Self::Object(ty) => Some(ty),
            _ => None,
        }
    }

    /// Unwraps the type into an object type.
    ///
    /// # Panics
    ///
    /// Panics if the type is not an object type.
    pub fn unwrap_object_type(self) -> ObjectType<N> {
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
    pub fn as_type_ref(&self) -> Option<&TypeRef<N>> {
        match self {
            Self::Ref(ty) => Some(ty),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`TypeRef`].
    ///
    /// * If `self` is a [`Type::Ref`], then the inner [`TypeRef`] is returned
    ///   wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_type_ref(self) -> Option<TypeRef<N>> {
        match self {
            Self::Ref(ty) => Some(ty),
            _ => None,
        }
    }

    /// Unwraps the type into a type reference.
    ///
    /// # Panics
    ///
    /// Panics if the type is not a type reference.
    pub fn unwrap_type_ref(self) -> TypeRef<N> {
        match self {
            Self::Ref(ty) => ty,
            _ => panic!("not a type reference"),
        }
    }

    /// Attempts to get a reference to the inner [`PrimitiveType`].
    ///
    /// * If `self` is a [`Type::Primitive`], then a reference to the inner
    ///   [`PrimitiveType`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_primitive_type(&self) -> Option<&PrimitiveType<N>> {
        match self {
            Self::Primitive(ty) => Some(ty),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`PrimitiveType`].
    ///
    /// * If `self` is a [`Type::Primitive`], then the inner [`PrimitiveType`]
    ///   is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_primitive_type(self) -> Option<PrimitiveType<N>> {
        match self {
            Self::Primitive(ty) => Some(ty),
            _ => None,
        }
    }

    /// Unwraps the type into a primitive type.
    ///
    /// # Panics
    ///
    /// Panics if the type is not a primitive type.
    pub fn unwrap_primitive_type(self) -> PrimitiveType<N> {
        match self {
            Self::Primitive(ty) => ty,
            _ => panic!("not a primitive type"),
        }
    }

    /// Finds the first child that can be cast to a [`Type`].
    pub fn child(node: &N) -> Option<Self> {
        node.children().find_map(Self::cast)
    }

    /// Finds all children that can be cast to a [`Type`].
    pub fn children(node: &N) -> impl Iterator<Item = Self> + use<'_, N> {
        node.children().filter_map(Self::cast)
    }
}

impl<N: TreeNode> PartialEq for Type<N> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Map(l), Self::Map(r)) => l == r,
            (Self::Array(l), Self::Array(r)) => l == r,
            (Self::Pair(l), Self::Pair(r)) => l == r,
            (Self::Object(l), Self::Object(r)) => l == r,
            (Self::Ref(l), Self::Ref(r)) => l == r,
            (Self::Primitive(l), Self::Primitive(r)) => l == r,
            _ => false,
        }
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
pub struct UnboundDecl<N: TreeNode = SyntaxNode>(pub(crate) N);

impl<N: TreeNode> UnboundDecl<N> {
    /// Gets the `env` token, if present.
    ///
    /// This may only return a token for task inputs (WDL 1.2+).
    pub fn env(&self) -> Option<EnvKeyword<N::Token>> {
        self.token()
    }

    /// Gets the type of the declaration.
    pub fn ty(&self) -> Type<N> {
        Type::child(&self.0).expect("unbound declaration should have a type")
    }

    /// Gets the name of the declaration.
    pub fn name(&self) -> Ident<N::Token> {
        self.token()
            .expect("unbound declaration should have a name")
    }
}

impl<N: TreeNode> AstNode<N> for UnboundDecl<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::UnboundDeclNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::UnboundDeclNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a bound declaration in a task or workflow definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundDecl<N: TreeNode = SyntaxNode>(pub(crate) N);

impl<N: TreeNode> BoundDecl<N> {
    /// Gets the `env` token, if present.
    ///
    /// This may only return a token for task inputs and private declarations
    /// (WDL 1.2+).
    pub fn env(&self) -> Option<EnvKeyword<N::Token>> {
        self.token()
    }

    /// Gets the type of the declaration.
    pub fn ty(&self) -> Type<N> {
        Type::child(&self.0).expect("bound declaration should have a type")
    }

    /// Gets the name of the declaration.
    pub fn name(&self) -> Ident<N::Token> {
        self.token().expect("bound declaration should have a name")
    }

    /// Gets the expression the declaration is bound to.
    pub fn expr(&self) -> Expr<N> {
        Expr::child(&self.0).expect("bound declaration should have an expression")
    }
}

impl<N: TreeNode> AstNode<N> for BoundDecl<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::BoundDeclNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::BoundDeclNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a declaration in an input section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decl<N: TreeNode = SyntaxNode> {
    /// The declaration is bound.
    Bound(BoundDecl<N>),
    /// The declaration is unbound.
    Unbound(UnboundDecl<N>),
}

impl<N: TreeNode> Decl<N> {
    /// Returns whether or not the given syntax kind can be cast to
    /// [`Decl`].
    pub fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::BoundDeclNode || kind == SyntaxKind::UnboundDeclNode
    }

    /// Casts the given node to [`Decl`].
    ///
    /// Returns `None` if the node cannot be cast.
    pub fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::BoundDeclNode => Some(Self::Bound(
                BoundDecl::cast(inner).expect("bound decl to cast"),
            )),
            SyntaxKind::UnboundDeclNode => Some(Self::Unbound(
                UnboundDecl::cast(inner).expect("unbound decl to cast"),
            )),
            _ => None,
        }
    }

    /// Gets a reference to the inner node.
    pub fn inner(&self) -> &N {
        match self {
            Self::Bound(d) => d.inner(),
            Self::Unbound(d) => d.inner(),
        }
    }

    /// Gets the `env` token, if present.
    ///
    /// This may only return a token for task inputs and private declarations
    /// (WDL 1.2+).
    pub fn env(&self) -> Option<EnvKeyword<N::Token>> {
        match self {
            Self::Bound(d) => d.env(),
            Self::Unbound(d) => d.env(),
        }
    }

    /// Gets the type of the declaration.
    pub fn ty(&self) -> Type<N> {
        match self {
            Self::Bound(d) => d.ty(),
            Self::Unbound(d) => d.ty(),
        }
    }

    /// Gets the name of the declaration.
    pub fn name(&self) -> Ident<N::Token> {
        match self {
            Self::Bound(d) => d.name(),
            Self::Unbound(d) => d.name(),
        }
    }

    /// Gets the expression of the declaration.
    ///
    /// Returns `None` for unbound declarations.
    pub fn expr(&self) -> Option<Expr<N>> {
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
    pub fn as_bound_decl(&self) -> Option<&BoundDecl<N>> {
        match self {
            Self::Bound(d) => Some(d),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`BoundDecl`].
    ///
    /// * If `self` is a [`Decl::Bound`], then the inner [`BoundDecl`] is
    ///   returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_bound_decl(self) -> Option<BoundDecl<N>> {
        match self {
            Self::Bound(d) => Some(d),
            _ => None,
        }
    }

    /// Unwraps the declaration into a bound declaration.
    ///
    /// # Panics
    ///
    /// Panics if the declaration is not a bound declaration.
    pub fn unwrap_bound_decl(self) -> BoundDecl<N> {
        match self {
            Self::Bound(d) => d,
            _ => panic!("not a bound declaration"),
        }
    }

    /// Attempts to get a reference to the inner [`UnboundDecl`].
    ///
    /// * If `self` is a [`Decl::Unbound`], then a reference to the inner
    ///   [`UnboundDecl`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_unbound_decl(&self) -> Option<&UnboundDecl<N>> {
        match self {
            Self::Unbound(d) => Some(d),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`UnboundDecl`].
    ///
    /// * If `self` is a [`Decl::Unbound`], then the inner [`UnboundDecl`] is
    ///   returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_unbound_decl(self) -> Option<UnboundDecl<N>> {
        match self {
            Self::Unbound(d) => Some(d),
            _ => None,
        }
    }

    /// Unwraps the declaration into an unbound declaration.
    ///
    /// # Panics
    ///
    /// Panics if the declaration is not an unbound declaration.
    pub fn unwrap_unbound_decl(self) -> UnboundDecl<N> {
        match self {
            Self::Unbound(d) => d,
            _ => panic!("not an unbound declaration"),
        }
    }

    /// Finds the first child that can be cast to a [`Decl`].
    pub fn child(node: &N) -> Option<Self> {
        node.children().find_map(Self::cast)
    }

    /// Finds all children that can be cast to a [`Decl`].
    pub fn children(node: &N) -> impl Iterator<Item = Self> + use<'_, N> {
        node.children().filter_map(Self::cast)
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
        assert_eq!(tasks[0].name().text(), "test");

        // Inputs
        let input = tasks[0].input().expect("task should have an input section");
        let decls: Vec<_> = input.declarations().collect();
        assert_eq!(decls.len(), 11);

        // First input declaration
        let decl = decls[0].clone().unwrap_unbound_decl();
        assert_eq!(decl.ty().to_string(), "Boolean");
        assert_eq!(decl.name().text(), "a");

        // Second input declaration
        let decl = decls[1].clone().unwrap_bound_decl();
        assert_eq!(decl.ty().to_string(), "Int");
        assert_eq!(decl.name().text(), "b");
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
        assert_eq!(decl.name().text(), "c");
        decl.expr().unwrap_literal().unwrap_none();

        // Fourth input declaration
        let decl = decls[3].clone().unwrap_unbound_decl();
        assert_eq!(decl.ty().to_string(), "String");
        assert_eq!(decl.name().text(), "d");

        // Fifth input declaration
        let decl = decls[4].clone().unwrap_bound_decl();
        assert_eq!(decl.ty().to_string(), "File");
        assert_eq!(decl.name().text(), "e");
        assert_eq!(
            decl.expr()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "foo.wdl"
        );

        // Sixth input declaration
        let decl = decls[5].clone().unwrap_unbound_decl();
        assert_eq!(decl.ty().to_string(), "Map[Int, Int]");
        assert_eq!(decl.name().text(), "f");

        // Seventh input declaration
        let decl = decls[6].clone().unwrap_bound_decl();
        assert_eq!(decl.ty().to_string(), "Array[String]");
        assert_eq!(decl.name().text(), "g");
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
        assert_eq!(decl.name().text(), "h");

        // Ninth input declaration
        let decl = decls[8].clone().unwrap_bound_decl();
        assert_eq!(decl.ty().to_string(), "Object");
        assert_eq!(decl.name().text(), "i");
        assert_eq!(
            decl.expr().unwrap_literal().unwrap_object().items().count(),
            0
        );

        // Tenth input declaration
        let decl = decls[9].clone().unwrap_unbound_decl();
        assert_eq!(decl.ty().to_string(), "MyStruct");
        assert_eq!(decl.name().text(), "j");

        // Eleventh input declaration
        let decl = decls[10].clone().unwrap_bound_decl();
        assert_eq!(decl.ty().to_string(), "Directory");
        assert_eq!(decl.name().text(), "k");
        assert_eq!(
            decl.expr()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
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
