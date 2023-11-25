//! Builder for an [`Import`].

use crate::v1::document::identifier::singular::Identifier;
use crate::v1::document::import::Aliases;
use crate::v1::document::Import;

/// An error that occurs when a required field is missing at build time.
#[derive(Debug)]
pub enum MissingError {
    /// A URI was not provided to the [`Builder`].
    Uri,
}

impl std::fmt::Display for MissingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MissingError::Uri => write!(f, "uri"),
        }
    }
}

impl std::error::Error for MissingError {}

/// An error that occurs when a multiple values were provded for a field that
/// only accepts a single value.
#[derive(Debug)]
pub enum MultipleError {
    /// Attempted to set multiple values for the "as" field within the
    /// [`Builder`].
    As,

    /// Attempted to set multiple values for the URI field within the
    /// [`Builder`].
    Uri,
}

impl std::fmt::Display for MultipleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MultipleError::As => write!(f, "as"),
            MultipleError::Uri => write!(f, "uri"),
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

/// A builder for an [`Import`].
#[derive(Debug, Default)]
pub struct Builder {
    /// The import aliases.
    aliases: Option<Aliases>,

    /// The as clause.
    r#as: Option<Identifier>,

    /// The URI.
    uri: Option<String>,
}

impl Builder {
    /// Inserts an alias into the [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::import::Builder;
    /// use wdl_ast as ast;
    ///
    /// let import = Builder::default()
    ///     .insert_alias(
    ///         Identifier::try_from("hello_world")?,
    ///         Identifier::try_from("foo_bar").unwrap(),
    ///     )
    ///     .r#as(Identifier::try_from("baz_quux").unwrap())?
    ///     .uri(String::from("../mapping.wdl"))?
    ///     .try_build()?;
    ///
    /// assert_eq!(
    ///     import.aliases().unwrap().get("hello_world"),
    ///     Some(&Identifier::try_from("foo_bar").unwrap())
    /// );
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn insert_alias(mut self, from: Identifier, to: Identifier) -> Self {
        let mut aliases = self.aliases.unwrap_or_default();
        aliases.insert(from, to);
        self.aliases = Some(aliases);
        self
    }

    /// Sets the as for the [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::import::Builder;
    /// use wdl_ast as ast;
    ///
    /// let import = Builder::default()
    ///     .insert_alias(
    ///         Identifier::try_from("hello_world")?,
    ///         Identifier::try_from("foo_bar").unwrap(),
    ///     )
    ///     .r#as(Identifier::try_from("baz_quux").unwrap())?
    ///     .uri(String::from("../mapping.wdl"))?
    ///     .try_build()?;
    ///
    /// assert_eq!(
    ///     import.r#as().unwrap(),
    ///     &Identifier::try_from("baz_quux").unwrap()
    /// );
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn r#as(mut self, identifier: Identifier) -> Result<Self> {
        if self.r#as.is_some() {
            return Err(Error::Multiple(MultipleError::As));
        }

        self.r#as = Some(identifier);
        Ok(self)
    }

    /// Sets the URI for the [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::import::Builder;
    /// use wdl_ast as ast;
    ///
    /// let import = Builder::default()
    ///     .insert_alias(
    ///         Identifier::try_from("hello_world")?,
    ///         Identifier::try_from("foo_bar").unwrap(),
    ///     )
    ///     .r#as(Identifier::try_from("baz_quux").unwrap())?
    ///     .uri(String::from("../mapping.wdl"))?
    ///     .try_build()?;
    ///
    /// assert_eq!(import.uri(), "../mapping.wdl");
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn uri(mut self, uri: String) -> Result<Self> {
        if self.uri.is_some() {
            return Err(Error::Multiple(MultipleError::Uri));
        }

        self.uri = Some(uri);
        Ok(self)
    }

    /// Consumes `self` to attempt to build an [`Import`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::import::Builder;
    /// use wdl_ast as ast;
    ///
    /// let import = Builder::default()
    ///     .insert_alias(
    ///         Identifier::try_from("hello_world")?,
    ///         Identifier::try_from("foo_bar").unwrap(),
    ///     )
    ///     .r#as(Identifier::try_from("baz_quux").unwrap())?
    ///     .uri(String::from("../mapping.wdl"))?
    ///     .try_build()?;
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn try_build(self) -> Result<Import> {
        let uri = self
            .uri
            .map(Ok)
            .unwrap_or(Err(Error::Missing(MissingError::Uri)))?;

        Ok(Import {
            uri,
            r#as: self.r#as,
            aliases: self.aliases,
        })
    }
}
