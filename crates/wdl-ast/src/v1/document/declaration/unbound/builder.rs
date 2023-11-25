//! Builder for a [unbound declaration](Declaration).

use crate::v1::document::declaration::r#type::Type;
use crate::v1::document::declaration::unbound::Declaration;
use crate::v1::document::identifier::singular::Identifier;

/// An error that occurs when a required field is missing at build time.
#[derive(Debug)]
pub enum MissingError {
    /// A name was not provided to the [`Builder`].
    Name,

    /// A type was not provided to the [`Builder`].
    Type,
}

impl std::fmt::Display for MissingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MissingError::Name => write!(f, "name"),
            MissingError::Type => write!(f, "type"),
        }
    }
}

impl std::error::Error for MissingError {}

/// An error that occurs when a multiple values were provded for a field that
/// only accepts a single value.
#[derive(Debug)]
pub enum MultipleError {
    /// Attempted to set multiple values for the name field within the
    /// [`Builder`].
    Name,

    /// Attempted to set multiple values for the type field within the
    /// [`Builder`].
    Type,
}

impl std::fmt::Display for MultipleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MultipleError::Name => write!(f, "name"),
            MultipleError::Type => write!(f, "type"),
        }
    }
}

impl std::error::Error for MultipleError {}

/// An error related to a [`Builder`].
#[derive(Debug)]
pub enum Error {
    /// A required field was missing at build time.
    Missing(MissingError),

    /// Multiple values were provided for a field that accepts a single value.
    Multiple(MultipleError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Missing(err) => write!(f, "missing value for field: {err}"),
            Error::Multiple(err) => {
                write!(f, "multiple values provided for single value field: {err}")
            }
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
type Result<T> = std::result::Result<T, Error>;

/// A builder for a [unbound declaration](Declaration).
#[derive(Debug, Default)]
pub struct Builder {
    /// The name.
    name: Option<Identifier>,

    /// The WDL type.
    r#type: Option<Type>,
}

impl Builder {
    /// Sets the name of the [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::declaration::r#type::Kind;
    /// use ast::v1::document::declaration::unbound::Builder;
    /// use ast::v1::document::declaration::Type;
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let declaration = Builder::default()
    ///     .name(Identifier::try_from("hello_world")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .try_build()?;
    ///
    /// assert_eq!(declaration.name().as_str(), "hello_world");
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn name(mut self, name: Identifier) -> Result<Self> {
        if self.name.is_some() {
            return Err(Error::Multiple(MultipleError::Name));
        }

        self.name = Some(name);
        Ok(self)
    }

    /// Sets the WDL [type](Type) of the [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::declaration::r#type::Kind;
    /// use ast::v1::document::declaration::unbound::Builder;
    /// use ast::v1::document::declaration::Type;
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let declaration = Builder::default()
    ///     .name(Identifier::try_from("hello_world")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .try_build()?;
    ///
    /// assert!(matches!(declaration.r#type().kind(), &Kind::Boolean));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn r#type(mut self, r#type: Type) -> Result<Self> {
        if self.r#type.is_some() {
            return Err(Error::Multiple(MultipleError::Type));
        }

        self.r#type = Some(r#type);
        Ok(self)
    }

    /// Consumes `self` to attempt to build a [`Declaration`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::declaration::r#type::Kind;
    /// use ast::v1::document::declaration::unbound::Builder;
    /// use ast::v1::document::declaration::Type;
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let declaration = Builder::default()
    ///     .name(Identifier::try_from("hello_world")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .try_build()?;
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn try_build(self) -> Result<Declaration> {
        let name = self
            .name
            .map(Ok)
            .unwrap_or(Err(Error::Missing(MissingError::Name)))?;

        let r#type = self
            .r#type
            .map(Ok)
            .unwrap_or(Err(Error::Missing(MissingError::Type)))?;

        Ok(Declaration { name, r#type })
    }
}
