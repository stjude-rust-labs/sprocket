//! Builder for a [`Runtime`].

use std::collections::BTreeMap;

use crate::v1::document::identifier::singular::Identifier;
use crate::v1::document::task::runtime::Value;
use crate::v1::document::task::Runtime;
use crate::v1::document::Expression;

/// An error that occurs when a required field is missing at build time.
#[derive(Debug)]
pub enum MissingError {
    /// A version was not provided to the [`Builder`].
    Container,
}

impl std::fmt::Display for MissingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MissingError::Container => write!(f, "version"),
        }
    }
}

impl std::error::Error for MissingError {}

/// An error that occurs when a multiple values were provded for a field that
/// only accepts a single value.
#[derive(Debug)]
pub enum MultipleError {
    /// Attempted to set multiple values for the `container` field within the
    /// [`Builder`].
    Container,

    /// Attempted to set multiple values for the `cpu` field within the
    /// [`Builder`].
    Cpu,

    /// Attempted to set multiple values for the `memory` field within the
    /// [`Builder`].
    Memory,

    /// Attempted to set multiple values for the `gpu` field within the
    /// [`Builder`].
    Gpu,

    /// Attempted to set multiple values for the `disks` field within the
    /// [`Builder`].
    Disks,

    /// Attempted to set multiple values for the `maxRetries` field within the
    /// [`Builder`].
    MaxRetries,

    /// Attempted to set multiple values for the `returnCodes` field within the
    /// [`Builder`].
    ReturnCodes,
}

impl std::fmt::Display for MultipleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MultipleError::Container => write!(f, "container"),
            MultipleError::Cpu => write!(f, "cpu"),
            MultipleError::Memory => write!(f, "memory"),
            MultipleError::Gpu => write!(f, "gpu"),
            MultipleError::Disks => write!(f, "disks"),
            MultipleError::MaxRetries => write!(f, "maxRetries"),
            MultipleError::ReturnCodes => write!(f, "returnCodes"),
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

/// A builder for a [`Runtime`].
#[derive(Debug, Default)]
pub struct Builder {
    /// The `container` field.
    container: Option<Value>,

    /// The `cpu` field.
    cpu: Option<Value>,

    /// The `memory` field.
    memory: Option<Value>,

    /// The `gpu` field.
    gpu: Option<Value>,

    /// The `disks` field.
    disks: Option<Value>,

    /// The `maxRetries` field.
    max_retries: Option<Value>,

    /// The `returnCodes` field.
    return_codes: Option<Value>,

    /// Other included runtime hints.
    hints: Option<BTreeMap<Identifier, Expression>>,
}

impl Builder {
    /// Sets the `container` field for this [`Runtime`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::task::runtime::Builder;
    /// use ast::v1::document::task::runtime::Value;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let container = Value::try_from(Expression::Literal(Literal::String(String::from(
    ///     "ubuntu:latest",
    /// ))))?;
    /// let runtime = Builder::default()
    ///     .container(container.clone())?
    ///     .try_build()?;
    ///
    /// assert_eq!(runtime.container(), Some(&container));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn container(mut self, container: Value) -> Result<Self> {
        if self.container.is_some() {
            return Err(Error::Multiple(MultipleError::Container));
        }

        self.container = Some(container);
        Ok(self)
    }

    /// Sets the `cpu` field for this [`Runtime`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::task::runtime::Builder;
    /// use ast::v1::document::task::runtime::Value;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let cpu = Value::try_from(Expression::Literal(Literal::Integer(4)))?;
    /// let runtime = Builder::default().cpu(cpu.clone())?.try_build()?;
    ///
    /// assert_eq!(runtime.cpu(), Some(&cpu));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn cpu(mut self, cpu: Value) -> Result<Self> {
        if self.cpu.is_some() {
            return Err(Error::Multiple(MultipleError::Cpu));
        }

        self.cpu = Some(cpu);
        Ok(self)
    }

    /// Sets the `memory` field for this [`Runtime`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::task::runtime::Builder;
    /// use ast::v1::document::task::runtime::Value;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let memory = Value::try_from(Expression::Literal(Literal::String(String::from("2 GiB"))))?;
    /// let runtime = Builder::default().memory(memory.clone())?.try_build()?;
    ///
    /// assert_eq!(runtime.memory(), Some(&memory));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn memory(mut self, memory: Value) -> Result<Self> {
        if self.memory.is_some() {
            return Err(Error::Multiple(MultipleError::Memory));
        }

        self.memory = Some(memory);
        Ok(self)
    }

    /// Sets the `gpu` field for this [`Runtime`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::task::runtime::Builder;
    /// use ast::v1::document::task::runtime::Value;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let gpu = Value::try_from(Expression::Literal(Literal::Boolean(false)))?;
    /// let runtime = Builder::default().gpu(gpu.clone())?.try_build()?;
    ///
    /// assert_eq!(runtime.gpu(), Some(&gpu));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn gpu(mut self, gpu: Value) -> Result<Self> {
        if self.gpu.is_some() {
            return Err(Error::Multiple(MultipleError::Gpu));
        }

        self.gpu = Some(gpu);
        Ok(self)
    }

    /// Sets the `disks` field for this [`Runtime`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::task::runtime::Builder;
    /// use ast::v1::document::task::runtime::Value;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let disks = Value::try_from(Expression::Literal(Literal::String(String::from("1 GiB"))))?;
    /// let runtime = Builder::default().disks(disks.clone())?.try_build()?;
    ///
    /// assert_eq!(runtime.disks(), Some(&disks));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn disks(mut self, disks: Value) -> Result<Self> {
        if self.disks.is_some() {
            return Err(Error::Multiple(MultipleError::Disks));
        }

        self.disks = Some(disks);
        Ok(self)
    }

    /// Sets the `maxRetries` field for this [`Runtime`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::task::runtime::Builder;
    /// use ast::v1::document::task::runtime::Value;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let max_retries = Value::try_from(Expression::Literal(Literal::Integer(0)))?;
    /// let runtime = Builder::default()
    ///     .max_retries(max_retries.clone())?
    ///     .try_build()?;
    ///
    /// assert_eq!(runtime.max_retries(), Some(&max_retries));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn max_retries(mut self, max_retries: Value) -> Result<Self> {
        if self.max_retries.is_some() {
            return Err(Error::Multiple(MultipleError::MaxRetries));
        }

        self.max_retries = Some(max_retries);
        Ok(self)
    }

    /// Sets the `returnCodes` field for this [`Runtime`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::task::runtime::Builder;
    /// use ast::v1::document::task::runtime::Value;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let return_codes = Value::try_from(Expression::Literal(Literal::Integer(0)))?;
    /// let runtime = Builder::default()
    ///     .return_codes(return_codes.clone())?
    ///     .try_build()?;
    ///
    /// assert_eq!(runtime.return_codes(), Some(&return_codes));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn return_codes(mut self, return_codes: Value) -> Result<Self> {
        if self.return_codes.is_some() {
            return Err(Error::Multiple(MultipleError::ReturnCodes));
        }

        self.return_codes = Some(return_codes);
        Ok(self)
    }

    /// Inserts a hint into this [`Runtime`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::task::runtime::Builder;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let runtime = Builder::default()
    ///     .insert_hint(
    ///         Identifier::try_from("hello")?,
    ///         Expression::Literal(Literal::None),
    ///     )
    ///     .try_build()?;
    ///
    /// assert_eq!(
    ///     runtime.hints().unwrap().get("hello"),
    ///     Some(&Expression::Literal(Literal::None))
    /// );
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn insert_hint(mut self, key: Identifier, value: Expression) -> Self {
        let mut hints = self.hints.unwrap_or_default();
        hints.insert(key, value);
        self.hints = Some(hints);
        self
    }

    /// Consumes `self` to attempt to build a [`Runtime`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::task::runtime::Builder;
    /// use ast::v1::document::task::runtime::Value;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let container = Value::try_from(Expression::Literal(Literal::String(String::from(
    ///     "ubuntu:latest",
    /// ))))?;
    /// let runtime = Builder::default()
    ///     .container(container.clone())?
    ///     .try_build()?;
    ///
    /// assert_eq!(runtime.container(), Some(&container));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn try_build(self) -> Result<Runtime> {
        Ok(Runtime {
            container: self.container,
            cpu: self.cpu,
            memory: self.memory,
            gpu: self.gpu,
            disks: self.disks,
            max_retries: self.max_retries,
            return_codes: self.return_codes,
            hints: self.hints,
        })
    }
}
