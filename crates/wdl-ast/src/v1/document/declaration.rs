//! Declarations.

use grammar::v1::Rule;
use pest::iterators::Pair;
use wdl_grammar as grammar;

pub mod r#type;

pub mod bound;
pub mod unbound;

pub use r#type::Type;

use crate::v1::document::identifier::singular::Identifier;
use crate::v1::document::Expression;

/// An error related to a [`Declaration`].
#[derive(Debug)]
pub enum Error {
    /// A bound declaration error.
    BoundDeclaration(bound::Error),

    /// An unbound declaration error.
    UnboundDeclaration(unbound::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::BoundDeclaration(err) => write!(f, "bound declaration error: {err}"),
            Error::UnboundDeclaration(err) => write!(f, "unbound declaration error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A declaration.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Declaration {
    /// A bound declaration.
    Bound(bound::Declaration),

    /// A unbound declaration.
    Unbound(unbound::Declaration),
}

impl Declaration {
    /// Gets the name from the [`Declaration`] by reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::declaration::bound;
    /// use ast::v1::document::declaration::r#type::Kind;
    /// use ast::v1::document::declaration::unbound;
    /// use ast::v1::document::declaration::Type;
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::Declaration;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let bound = bound::Builder::default()
    ///     .name(Identifier::try_from("foo")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .value(Expression::Literal(Literal::None))?
    ///     .try_build()?;
    /// let declaration = Declaration::Bound(bound.clone());
    ///
    /// assert_eq!(declaration.name().as_str(), "foo");
    ///
    /// let unbound = unbound::Builder::default()
    ///     .name(Identifier::try_from("bar")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .try_build()?;
    /// let declaration = Declaration::Unbound(unbound);
    ///
    /// assert_eq!(declaration.name().as_str(), "bar");
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn name(&self) -> &Identifier {
        match self {
            Declaration::Bound(bound) => bound.name(),
            Declaration::Unbound(unbound) => unbound.name(),
        }
    }

    /// Gets the type from the [`Declaration`] by reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::declaration::bound;
    /// use ast::v1::document::declaration::r#type::Kind;
    /// use ast::v1::document::declaration::unbound;
    /// use ast::v1::document::declaration::Type;
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::Declaration;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let bound = bound::Builder::default()
    ///     .name(Identifier::try_from("foo")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .value(Expression::Literal(Literal::None))?
    ///     .try_build()?;
    /// let declaration = Declaration::Bound(bound.clone());
    ///
    /// assert_eq!(declaration.r#type().kind(), &Kind::Boolean);
    /// assert_eq!(declaration.r#type().optional(), false);
    ///
    /// let unbound = unbound::Builder::default()
    ///     .name(Identifier::try_from("bar")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .try_build()?;
    /// let declaration = Declaration::Unbound(unbound);
    ///
    /// assert_eq!(declaration.r#type().kind(), &Kind::Boolean);
    /// assert_eq!(declaration.r#type().optional(), false);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn r#type(&self) -> &Type {
        match self {
            Declaration::Bound(bound) => bound.r#type(),
            Declaration::Unbound(unbound) => unbound.r#type(),
        }
    }

    /// Gets the value from the [`Declaration`] by reference (if it exists).
    ///
    /// * If the declaration is bound, a reference to the value (as an
    ///   [`Expression`]) is returned wrapped in [`Some`].
    /// * If the declaration is unbound, [`None`] is returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::declaration::bound;
    /// use ast::v1::document::declaration::r#type::Kind;
    /// use ast::v1::document::declaration::unbound;
    /// use ast::v1::document::declaration::Type;
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::Declaration;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let bound = bound::Builder::default()
    ///     .name(Identifier::try_from("foo")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .value(Expression::Literal(Literal::None))?
    ///     .try_build()?;
    /// let declaration = Declaration::Bound(bound.clone());
    ///
    /// assert_eq!(
    ///     declaration.value(),
    ///     Some(&Expression::Literal(Literal::None))
    /// );
    ///
    /// let unbound = unbound::Builder::default()
    ///     .name(Identifier::try_from("bar")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .try_build()?;
    /// let declaration = Declaration::Unbound(unbound);
    ///
    /// assert_eq!(declaration.value(), None);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn value(&self) -> Option<&Expression> {
        match self {
            Declaration::Bound(bound) => Some(bound.value()),
            Declaration::Unbound(_) => None,
        }
    }

    /// Returns a reference to the [bound declaration](bound::Declaration)
    /// wrapped in [`Some`] if the [`Declaration`] is an [`Declaration::Bound`].
    /// Else, [`None`] is returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::declaration::bound;
    /// use ast::v1::document::declaration::r#type::Kind;
    /// use ast::v1::document::declaration::unbound;
    /// use ast::v1::document::declaration::Type;
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::Declaration;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let bound = bound::Builder::default()
    ///     .name(Identifier::try_from("foo")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .value(Expression::Literal(Literal::None))?
    ///     .try_build()?;
    /// let declaration = Declaration::Bound(bound.clone());
    ///
    /// assert_eq!(declaration.as_bound(), Some(&bound));
    ///
    /// let unbound = unbound::Builder::default()
    ///     .name(Identifier::try_from("bar")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .try_build()?;
    /// let declaration = Declaration::Unbound(unbound);
    ///
    /// assert_eq!(declaration.as_bound(), None);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn as_bound(&self) -> Option<&bound::Declaration> {
        match self {
            Declaration::Bound(bound) => Some(bound),
            _ => None,
        }
    }

    /// Consumes `self` and returns the [bound declaration](bound::Declaration)
    /// wrapped in [`Some`] if the [`Declaration`] is an [`Declaration::Bound`].
    /// Else, [`None`] is returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::declaration::bound;
    /// use ast::v1::document::declaration::r#type::Kind;
    /// use ast::v1::document::declaration::unbound;
    /// use ast::v1::document::declaration::Type;
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::Declaration;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let bound = bound::Builder::default()
    ///     .name(Identifier::try_from("foo")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .value(Expression::Literal(Literal::None))?
    ///     .try_build()?;
    /// let declaration = Declaration::Bound(bound.clone());
    ///
    /// assert_eq!(declaration.into_bound(), Some(bound));
    ///
    /// let unbound = unbound::Builder::default()
    ///     .name(Identifier::try_from("bar")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .try_build()?;
    /// let declaration = Declaration::Unbound(unbound);
    ///
    /// assert_eq!(declaration.into_bound(), None);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn into_bound(self) -> Option<bound::Declaration> {
        match self {
            Declaration::Bound(bound) => Some(bound),
            _ => None,
        }
    }

    /// Returns a reference to the [unbound declaration](bound::Declaration)
    /// wrapped in [`Some`] if the [`Declaration`] is an
    /// [`Declaration::Unbound`]. Else, [`None`] is returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::declaration::bound;
    /// use ast::v1::document::declaration::r#type::Kind;
    /// use ast::v1::document::declaration::unbound;
    /// use ast::v1::document::declaration::Type;
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::Declaration;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let bound = bound::Builder::default()
    ///     .name(Identifier::try_from("foo")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .value(Expression::Literal(Literal::None))?
    ///     .try_build()?;
    /// let declaration = Declaration::Bound(bound);
    ///
    /// assert_eq!(declaration.as_unbound(), None);
    ///
    /// let unbound = unbound::Builder::default()
    ///     .name(Identifier::try_from("bar")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .try_build()?;
    /// let declaration = Declaration::Unbound(unbound.clone());
    ///
    /// assert_eq!(declaration.as_unbound(), Some(&unbound));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn as_unbound(&self) -> Option<&unbound::Declaration> {
        match self {
            Declaration::Unbound(unbound) => Some(unbound),
            _ => None,
        }
    }

    /// Consumes `self` and returns the [unbound
    /// declaration](unbound::Declaration) wrapped in [`Some`] if the
    /// [`Declaration`] is an [`Declaration::Unbound`]. Else, [`None`] is
    /// returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::declaration::bound;
    /// use ast::v1::document::declaration::r#type::Kind;
    /// use ast::v1::document::declaration::unbound;
    /// use ast::v1::document::declaration::Type;
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::Declaration;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let bound = bound::Builder::default()
    ///     .name(Identifier::try_from("foo")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .value(Expression::Literal(Literal::None))?
    ///     .try_build()?;
    /// let declaration = Declaration::Bound(bound);
    ///
    /// assert_eq!(declaration.into_unbound(), None);
    ///
    /// let unbound = unbound::Builder::default()
    ///     .name(Identifier::try_from("bar")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .try_build()?;
    /// let declaration = Declaration::Unbound(unbound.clone());
    ///
    /// assert_eq!(declaration.into_unbound(), Some(unbound));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn into_unbound(self) -> Option<unbound::Declaration> {
        match self {
            Declaration::Unbound(unbound) => Some(unbound),
            _ => None,
        }
    }
}

impl TryFrom<Pair<'_, grammar::v1::Rule>> for Declaration {
    type Error = Error;

    fn try_from(node: Pair<'_, grammar::v1::Rule>) -> Result<Self, Self::Error> {
        match node.as_rule() {
            Rule::bound_declaration => {
                let declaration =
                    bound::Declaration::try_from(node).map_err(Error::BoundDeclaration)?;
                Ok(Declaration::Bound(declaration))
            }
            Rule::unbound_declaration => {
                let declaration =
                    unbound::Declaration::try_from(node).map_err(Error::UnboundDeclaration)?;
                Ok(Declaration::Unbound(declaration))
            }
            rule => panic!("declaration cannot be parsed from node type {:?}", rule),
        }
    }
}
