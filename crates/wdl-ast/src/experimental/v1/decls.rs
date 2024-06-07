//! V1 AST representation for declarations.

use std::fmt;

use rowan::ast::support;
use rowan::ast::support::child;
use rowan::ast::AstNode;

use super::Expr;
use crate::experimental::token;
use crate::experimental::AstToken;
use crate::experimental::Ident;
use crate::experimental::SyntaxKind;
use crate::experimental::SyntaxNode;
use crate::experimental::WorkflowDescriptionLanguage;

/// Represents a `Map` type.
#[derive(Clone, Debug, PartialEq, Eq)]
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArrayType(SyntaxNode);

impl ArrayType {
    /// Gets the element type of the array.
    pub fn element_type(&self) -> Type {
        child(&self.0).expect("array should have an element type")
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairType(SyntaxNode);

impl PairType {
    /// Gets the first and second types of the `Pair`.
    pub fn types(&self) -> (Type, Type) {
        let mut children = self.0.children().filter_map(Type::cast);
        let first = children.next().expect("pair should have a first type");
        let second = children.next().expect("pair should have a second type");
        (first, second)
    }

    /// Determines if the type is optional.
    pub fn is_optional(&self) -> bool {
        matches!(
            self.0.last_token().map(|t| t.kind()),
            Some(SyntaxKind::QuestionMark)
        )
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
        let (first, second) = self.types();
        write!(
            f,
            "Pair[{first}, {second}]{o}",
            o = if self.is_optional() { "?" } else { "" }
        )
    }
}

/// Represents a `Object` type.
#[derive(Clone, Debug, PartialEq, Eq)]
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
#[derive(Clone, Debug, PartialEq, Eq)]
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
}

/// Represents a primitive type.
#[derive(Clone, Debug, PartialEq, Eq)]
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

    /// Unwraps the type into an object type.
    ///
    /// # Panics
    ///
    /// Panics if the type is not an object type.
    pub fn unwrap_objet_type(self) -> ObjectType {
        match self {
            Self::Object(ty) => ty,
            _ => panic!("not an object type"),
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
}

impl AstNode for Type {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
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

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::MapTypeNode => Some(Self::Map(MapType(syntax))),
            SyntaxKind::ArrayTypeNode => Some(Self::Array(ArrayType(syntax))),
            SyntaxKind::PairTypeNode => Some(Self::Pair(PairType(syntax))),
            SyntaxKind::ObjectTypeNode => Some(Self::Object(ObjectType(syntax))),
            SyntaxKind::TypeRefNode => Some(Self::Ref(TypeRef(syntax))),
            SyntaxKind::PrimitiveTypeNode => Some(Self::Primitive(PrimitiveType(syntax))),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        match self {
            Type::Map(m) => &m.0,
            Type::Array(a) => &a.0,
            Type::Pair(p) => &p.0,
            Type::Object(o) => &o.0,
            Type::Ref(r) => &r.0,
            Type::Primitive(t) => &t.0,
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
pub struct UnboundDecl(pub(super) SyntaxNode);

impl UnboundDecl {
    /// Gets the type of the declaration.
    pub fn ty(&self) -> Type {
        child(&self.0).expect("unbound declaration should have a type")
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
pub struct BoundDecl(pub(super) SyntaxNode);

impl BoundDecl {
    /// Gets the type of the declaration.
    pub fn ty(&self) -> Type {
        child(&self.0).expect("bound declaration should have a type")
    }

    /// Gets the name of the declaration.
    pub fn name(&self) -> Ident {
        token(&self.0).expect("bound declaration should have a name")
    }

    /// Gets the expression the declaration is bound to.
    pub fn expr(&self) -> Expr {
        child(&self.0).expect("bound declaration should have an expression")
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
}

impl AstNode for Decl {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::BoundDeclNode || kind == SyntaxKind::UnboundDeclNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::BoundDeclNode => Some(Self::Bound(BoundDecl(syntax))),
            SyntaxKind::UnboundDeclNode => Some(Self::Unbound(UnboundDecl(syntax))),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        match self {
            Self::Bound(b) => &b.0,
            Self::Unbound(u) => &u.0,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::experimental::v1::Visitor;
    use crate::experimental::Document;
    use crate::experimental::VisitReason;

    #[test]
    fn decls() {
        let parse = Document::parse(
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
    }
}
"#,
        );

        let document = parse.into_result().expect("there should be no errors");
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().as_str(), "test");

        // Inputs
        let inputs: Vec<_> = tasks[0].inputs().collect();
        assert_eq!(inputs.len(), 1);
        let decls: Vec<_> = inputs[0].declarations().collect();
        assert_eq!(decls.len(), 10);

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

        // Use a visitor to count the number of declarations
        #[derive(Default)]
        struct MyVisitor {
            bound: usize,
            unbound: usize,
        }

        impl Visitor for MyVisitor {
            type State = ();

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
        ast.visit(&mut (), &mut visitor);
        assert_eq!(visitor.bound, 5);
        assert_eq!(visitor.unbound, 5);
    }
}
