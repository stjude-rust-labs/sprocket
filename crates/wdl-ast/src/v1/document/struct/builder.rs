//! Builder for a [`Struct`].

use nonempty::NonEmpty;

use crate::v1::document::declaration::unbound::Declaration;
use crate::v1::document::identifier::singular::Identifier;
use crate::v1::document::r#struct::Declarations;
use crate::v1::document::Struct;

/// An error that occurs when a required field is missing at build time.
#[derive(Debug)]
pub enum MissingError {
    /// A name was not provided to the [`Builder`].
    Name,
}

impl std::fmt::Display for MissingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MissingError::Name => write!(f, "name"),
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
}

impl std::fmt::Display for MultipleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MultipleError::Name => write!(f, "name"),
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

/// A builder for an [`Struct`].
#[derive(Debug, Default)]
pub struct Builder {
    /// The unbound declarations (if they exist).
    declarations: Option<Declarations>,

    /// The name.
    name: Option<Identifier>,
}

impl Builder {
    /// Sets the name for the [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::r#struct::Builder;
    /// use wdl_ast as ast;
    ///
    /// let r#struct = Builder::default()
    ///     .name(Identifier::try_from("a_struct").unwrap())?
    ///     .try_build()?;
    ///
    /// assert_eq!(r#struct.name().as_str(), "a_struct");
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

    /// Pushes an [unbound declaration](Declaration) into this [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::declaration;
    /// use ast::v1::document::declaration::r#type::Kind;
    /// use ast::v1::document::declaration::Type;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::r#struct::Builder;
    /// use wdl_ast as ast;
    ///
    /// let declaration = declaration::unbound::Builder::default()
    ///     .name(Identifier::try_from("hello_world")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .try_build()?;
    ///
    /// let r#struct = Builder::default()
    ///     .name(Identifier::try_from("a_struct").unwrap())?
    ///     .push_unbound_declaration(declaration.clone())
    ///     .try_build()?;
    ///
    /// assert_eq!(r#struct.declarations().unwrap().len(), 1);
    /// assert_eq!(r#struct.declarations().unwrap().first(), &declaration);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn push_unbound_declaration(mut self, declaration: Declaration) -> Self {
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

    /// Consumes `self` to attempt to build a [`Struct`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::declaration;
    /// use ast::v1::document::declaration::r#type::Kind;
    /// use ast::v1::document::declaration::Type;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::r#struct::Builder;
    /// use wdl_ast as ast;
    ///
    /// let declaration = declaration::unbound::Builder::default()
    ///     .name(Identifier::try_from("hello_world")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .try_build()?;
    ///
    /// let r#struct = Builder::default()
    ///     .name(Identifier::try_from("a_struct").unwrap())?
    ///     .push_unbound_declaration(declaration.clone())
    ///     .try_build()?;
    ///
    /// assert_eq!(r#struct.name().as_str(), "a_struct");
    /// assert_eq!(r#struct.declarations().unwrap().len(), 1);
    /// assert_eq!(r#struct.declarations().unwrap().first(), &declaration);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn try_build(self) -> Result<Struct> {
        let name = self
            .name
            .map(Ok)
            .unwrap_or(Err(Error::Missing(MissingError::Name)))?;

        Ok(Struct {
            name,
            declarations: self.declarations,
        })
    }
}
