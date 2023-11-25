//! Builder for a [`PrivateDeclarations`].

use nonempty::NonEmpty;

use crate::v1::document::declaration::bound::Declaration;
use crate::v1::document::PrivateDeclarations;

/// An error related to a [`Builder`].
#[derive(Debug)]
pub enum Error {
    /// Attempted to create a [`PrivateDeclarations`] that contained no
    /// declarations, which is disallowed by the specification.
    EmptyDeclarations,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::EmptyDeclarations => write!(f, "empty declarations"),
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
type Result<T> = std::result::Result<T, Error>;

/// A builder for [`PrivateDeclarations`].
#[derive(Debug, Default)]
pub struct Builder {
    /// The bound declarations.
    declarations: Option<NonEmpty<Declaration>>,
}

impl Builder {
    /// Pushes a new [bound declaration](Declaration) into the [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::declaration::bound;
    /// use ast::v1::document::declaration::r#type::Kind;
    /// use ast::v1::document::declaration::Type;
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::private_declarations::Builder;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let declaration = bound::Builder::default()
    ///     .name(Identifier::try_from("hello_world")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .value(Expression::Literal(Literal::None))?
    ///     .try_build()?;
    ///
    /// let declarations = Builder::default()
    ///     .push_bound_declaration(declaration.clone())
    ///     .try_build()?;
    ///
    /// assert_eq!(declarations.inner().len(), 1);
    /// assert_eq!(declarations.inner().iter().next().unwrap(), &declaration);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn push_bound_declaration(mut self, declaration: Declaration) -> Self {
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

    /// Consumes `self` to attempt to build a [`PrivateDeclarations`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::declaration::bound;
    /// use ast::v1::document::declaration::r#type::Kind;
    /// use ast::v1::document::declaration::Type;
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::private_declarations::Builder;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let declaration = bound::Builder::default()
    ///     .name(Identifier::try_from("hello_world")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .value(Expression::Literal(Literal::None))?
    ///     .try_build()?;
    ///
    /// let declarations = Builder::default()
    ///     .push_bound_declaration(declaration.clone())
    ///     .try_build()?;
    ///
    /// assert_eq!(declarations.inner().len(), 1);
    /// assert_eq!(declarations.inner().iter().next().unwrap(), &declaration);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn try_build(self) -> Result<PrivateDeclarations> {
        let declarations = self
            .declarations
            .map(Ok)
            .unwrap_or(Err(Error::EmptyDeclarations))?;

        Ok(PrivateDeclarations::from(declarations))
    }
}
