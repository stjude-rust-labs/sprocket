//! Builder for a [`Call`].

use nonempty::NonEmpty;

use crate::v1::document::identifier::singular;
use crate::v1::document::workflow::execution::statement::call::Body;
use crate::v1::document::workflow::execution::statement::Call;
use crate::v1::document::Identifier;

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
    /// Attempted to set multiple values for the "as" field within the
    /// [`Builder`].
    As,

    /// Attempted to set multiple values for the body field within the
    /// [`Builder`].
    Body,

    /// Attempted to set multiple values for the name field within the
    /// [`Builder`].
    Name,
}

impl std::fmt::Display for MultipleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MultipleError::As => write!(f, "as"),
            MultipleError::Name => write!(f, "name"),
            MultipleError::Body => write!(f, "body"),
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

/// A builder for an [`Call`].
#[derive(Debug, Default)]
pub struct Builder {
    /// The after clauses.
    after: Option<NonEmpty<singular::Identifier>>,

    /// The body.
    body: Option<Body>,

    /// The name.
    name: Option<Identifier>,

    /// The as clause.
    r#as: Option<singular::Identifier>,
}

impl Builder {
    /// Sets the body for this [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    ///
    /// use ast::v1::document::identifier::singular;
    /// use ast::v1::document::workflow::execution::statement::call::body::Value;
    /// use ast::v1::document::workflow::execution::statement::call::Body;
    /// use ast::v1::document::workflow::execution::statement::call::Builder;
    /// use ast::v1::document::Identifier;
    /// use wdl_ast as ast;
    ///
    /// let mut map = BTreeMap::new();
    /// map.insert(singular::Identifier::try_from("a")?, Value::ImplicitBinding);
    ///
    /// let name = Identifier::from(singular::Identifier::try_from("foo")?);
    /// let body = Body::from(map);
    ///
    /// let call = Builder::default()
    ///     .name(name)?
    ///     .body(body.clone())?
    ///     .try_build()?;
    ///
    /// assert_eq!(call.body(), Some(&body));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn body(mut self, body: Body) -> Result<Self> {
        if self.body.is_some() {
            return Err(Error::Multiple(MultipleError::Body));
        }

        self.body = Some(body);
        Ok(self)
    }

    /// Sets the name for this [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::identifier::singular;
    /// use ast::v1::document::workflow::execution::statement::call::Builder;
    /// use ast::v1::document::Identifier;
    /// use wdl_ast as ast;
    ///
    /// let name = Identifier::from(singular::Identifier::try_from("foo")?);
    /// let call = Builder::default().name(name.clone())?.try_build()?;
    ///
    /// assert_eq!(call.name(), &name);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn name(mut self, identifier: Identifier) -> Result<Self> {
        if self.name.is_some() {
            return Err(Error::Multiple(MultipleError::Name));
        }

        self.name = Some(identifier);
        Ok(self)
    }

    /// Pushes an after clause into this [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::identifier::singular;
    /// use ast::v1::document::workflow::execution::statement::call::Builder;
    /// use ast::v1::document::Identifier;
    /// use wdl_ast as ast;
    ///
    /// let name = Identifier::from(singular::Identifier::try_from("foo")?);
    /// let after = singular::Identifier::try_from("baz")?;
    /// let call = Builder::default()
    ///     .name(name)?
    ///     .push_after(after.clone())
    ///     .try_build()?;
    ///
    /// assert_eq!(call.afters().unwrap().len(), 1);
    /// assert_eq!(call.afters().unwrap().iter().next().unwrap(), &after);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn push_after(mut self, identifier: singular::Identifier) -> Self {
        let after = match self.after {
            Some(mut after) => {
                after.push(identifier);
                after
            }
            None => NonEmpty::new(identifier),
        };

        self.after = Some(after);
        self
    }

    /// Sets the as clause into this [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::identifier::singular;
    /// use ast::v1::document::workflow::execution::statement::call::Builder;
    /// use ast::v1::document::Identifier;
    /// use wdl_ast as ast;
    ///
    /// let name = Identifier::from(singular::Identifier::try_from("foo")?);
    /// let r#as = singular::Identifier::try_from("bar")?;
    /// let call = Builder::default()
    ///     .name(name)?
    ///     .r#as(r#as.clone())?
    ///     .try_build()?;
    ///
    /// assert_eq!(call.r#as().unwrap(), &r#as);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn r#as(mut self, identifier: singular::Identifier) -> Result<Self> {
        if self.r#as.is_some() {
            return Err(Error::Multiple(MultipleError::As));
        }

        self.r#as = Some(identifier);
        Ok(self)
    }

    /// Consumes `self` to attempt to build a [`Call`].
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    ///
    /// use ast::v1::document::identifier::singular;
    /// use ast::v1::document::workflow::execution::statement::call::body::Value;
    /// use ast::v1::document::workflow::execution::statement::call::Body;
    /// use ast::v1::document::workflow::execution::statement::call::Builder;
    /// use ast::v1::document::Identifier;
    /// use wdl_ast as ast;
    ///
    /// let mut map = BTreeMap::new();
    /// map.insert(singular::Identifier::try_from("a")?, Value::ImplicitBinding);
    ///
    /// let name = Identifier::from(singular::Identifier::try_from("foo")?);
    /// let body = Body::from(map);
    /// let r#as = singular::Identifier::try_from("bar")?;
    /// let after = singular::Identifier::try_from("baz")?;
    ///
    /// let call = Builder::default()
    ///     .name(name.clone())?
    ///     .body(body.clone())?
    ///     .r#as(r#as.clone())?
    ///     .push_after(after.clone())
    ///     .try_build()?;
    ///
    /// assert_eq!(call.name(), &name);
    /// assert_eq!(call.body(), Some(&body));
    /// assert_eq!(call.r#as().unwrap(), &r#as);
    /// assert_eq!(call.afters().unwrap().len(), 1);
    /// assert_eq!(call.afters().unwrap().iter().next().unwrap(), &after);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn try_build(self) -> Result<Call> {
        let name = self
            .name
            .map(Ok)
            .unwrap_or(Err(Error::Missing(MissingError::Name)))?;

        Ok(Call {
            afters: self.after,
            body: self.body,
            name,
            r#as: self.r#as,
        })
    }
}
