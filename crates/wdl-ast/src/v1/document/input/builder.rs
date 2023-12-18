//! Builder for an [`Input`].

use nonempty::NonEmpty;
use wdl_core::file::location::Located;

use crate::v1::document::input::Declarations;
use crate::v1::document::input::Input;
use crate::v1::document::Declaration;

/// A builder for an [`Input`].
#[derive(Debug, Default)]
pub struct Builder {
    /// The bound and unbound declarations.
    declarations: Option<Declarations>,
}

impl Builder {
    /// Pushes a new [declaration](Declaration) into the [`Builder`].
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
    pub fn push_declaration(mut self, declaration: Located<Declaration>) -> Self {
        let declarations = match self.declarations {
            Some(mut declarations) => {
                declarations.push(declaration);
                declarations
            }
            None => NonEmpty::new(declaration),
        };

        self.declarations = Some(declarations);
        self
    }

    /// Consumes `self` to build an [`Input`].
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
    pub fn build(self) -> Input {
        Input {
            declarations: self.declarations,
        }
    }
}
