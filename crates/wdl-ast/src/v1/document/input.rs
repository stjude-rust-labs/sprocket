//! Inputs.

use grammar::v1::Rule;
use nonempty::NonEmpty;
use pest::iterators::Pair;
use wdl_core::file::location::Located;
use wdl_core::file::Location;
use wdl_grammar as grammar;
use wdl_macros::check_node;

use crate::v1::document::declaration;
use crate::v1::document::Declaration;

mod builder;

pub use builder::Builder;

/// An error related to a [`Input`].
#[derive(Debug)]
pub enum Error {
    /// A declaration error.
    Declaration(declaration::Error),

    /// A location error.
    Location(wdl_core::file::location::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Declaration(err) => write!(f, "declaration error: {err}"),
            Error::Location(err) => write!(f, "location error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// Bound and unbound declarations in an [`Input`].
pub type Declarations = NonEmpty<Located<Declaration>>;

/// An input.
///
/// **Note:** this struct could have been designed as a tuple struct. However,
/// it felt non-ergonomic to wrap an optional type and allow dereferencing as is
/// the convention elsewhere in the code base. As such, it is written as a
/// single field struct.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct Input {
    /// The bound and unbound declarations (if they exist).
    declarations: Option<Declarations>,
}

impl Input {
    /// Gets the [declaration(s)](Declarations) from the [`Input`] by reference
    /// (if they exist).
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
    /// use ast::v1::document::input::Builder;
    /// use ast::v1::document::Declaration;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    /// use wdl_core::file::location::Located;
    ///
    /// let bound = bound::Builder::default()
    ///     .name(Identifier::try_from("hello_world")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .value(Expression::Literal(Literal::None))?
    ///     .try_build()?;
    ///
    /// let unbound = unbound::Builder::default()
    ///     .name(Identifier::try_from("foo_bar")?)?
    ///     .r#type(Type::new(Kind::Boolean, true))?
    ///     .try_build()?;
    ///
    /// let mut input = Builder::default()
    ///     .push_declaration(Located::unplaced(Declaration::Bound(bound.clone())))
    ///     .push_declaration(Located::unplaced(Declaration::Unbound(unbound.clone())))
    ///     .build();
    ///
    /// let declarations = input.declarations().unwrap();
    /// assert_eq!(declarations.len(), 2);
    ///
    /// let mut declarations = declarations.iter();
    /// assert_eq!(declarations.next().unwrap().as_bound().unwrap(), &bound);
    /// assert_eq!(declarations.next().unwrap().as_unbound().unwrap(), &unbound);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn declarations(&self) -> Option<&Declarations> {
        self.declarations.as_ref()
    }
}

impl TryFrom<Pair<'_, grammar::v1::Rule>> for Input {
    type Error = Error;

    fn try_from(node: Pair<'_, grammar::v1::Rule>) -> Result<Self, Error> {
        check_node!(node, input);
        let mut builder = builder::Builder::default();

        for node in node.into_inner() {
            match node.as_rule() {
                Rule::bound_declaration | Rule::unbound_declaration => {
                    let location = Location::try_from(node.as_span()).map_err(Error::Location)?;
                    let declaration = Declaration::try_from(node).map_err(Error::Declaration)?;
                    builder = builder.push_declaration(Located::new(declaration, location));
                }
                Rule::COMMENT => {}
                Rule::WHITESPACE => {}
                rule => {
                    unreachable!("workflow input should not contain {:?}", rule)
                }
            }
        }

        Ok(builder.build())
    }
}
