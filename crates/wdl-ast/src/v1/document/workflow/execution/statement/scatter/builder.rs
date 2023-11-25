//! Builder for a [`Scatter`].

use nonempty::NonEmpty;

use crate::v1::document::identifier::singular::Identifier;
use crate::v1::document::workflow::execution::statement::Scatter;
use crate::v1::document::workflow::execution::Statement;
use crate::v1::document::Expression;

/// An error that occurs when a required field is missing at build time.
#[derive(Debug)]
pub enum MissingError {
    /// A iterable was not provided to the [`Builder`].
    Iterable,

    /// A variable was not provided to the [`Builder`].
    Variable,
}

impl std::fmt::Display for MissingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MissingError::Iterable => write!(f, "iterable"),
            MissingError::Variable => write!(f, "variable"),
        }
    }
}

impl std::error::Error for MissingError {}

/// An error that occurs when a multiple values were provded for a field that
/// only accepts a single value.
#[derive(Debug)]
pub enum MultipleError {
    /// Attempted to set multiple values for the iterable field within the
    /// [`Builder`].
    Iterable,

    /// Attempted to set multiple values for the variable field within the
    /// [`Builder`].
    Variable,
}

impl std::fmt::Display for MultipleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MultipleError::Iterable => write!(f, "iterable"),
            MultipleError::Variable => write!(f, "variable"),
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

/// A builder for a [`Scatter`].
#[derive(Debug, Default)]
pub struct Builder {
    /// The entities being scattered over.
    iterable: Option<Expression>,

    /// The workflow execution statements.
    statements: Option<NonEmpty<Box<Statement>>>,

    /// The variable name.
    variable: Option<Identifier>,
}

impl Builder {
    /// Sets the iterable for this [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::identifier::singular;
    /// use ast::v1::document::workflow::execution::statement::call;
    /// use ast::v1::document::workflow::execution::statement::scatter::Builder;
    /// use ast::v1::document::workflow::execution::Statement;
    /// use ast::v1::document::Expression;
    /// use ast::v1::document::Identifier;
    /// use wdl_ast as ast;
    ///
    /// let variable = singular::Identifier::try_from("entity")?;
    /// let iterable = Expression::Literal(Literal::Identifier(singular::Identifier::try_from(
    ///     "entities",
    /// )?));
    /// let statement = Statement::Call(
    ///     call::Builder::default()
    ///         .name(Identifier::from(singular::Identifier::try_from(
    ///             "hello_world",
    ///         )?))?
    ///         .try_build()?,
    /// );
    ///
    /// let scatter = Builder::default()
    ///     .variable(variable)?
    ///     .iterable(iterable.clone())?
    ///     .push_workflow_execution_statement(statement)
    ///     .try_build()?;
    ///
    /// assert_eq!(scatter.iterable(), &iterable);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn iterable(mut self, iterable: Expression) -> Result<Self> {
        if self.iterable.is_some() {
            return Err(Error::Multiple(MultipleError::Iterable));
        }

        self.iterable = Some(iterable);
        Ok(self)
    }

    /// Pushes a [workflow execution statement](Statement) into this
    /// [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::identifier::singular;
    /// use ast::v1::document::workflow::execution::statement::call;
    /// use ast::v1::document::workflow::execution::statement::scatter::Builder;
    /// use ast::v1::document::workflow::execution::Statement;
    /// use ast::v1::document::Expression;
    /// use ast::v1::document::Identifier;
    /// use wdl_ast as ast;
    ///
    /// let variable = singular::Identifier::try_from("entity")?;
    /// let iterable = Expression::Literal(Literal::Identifier(singular::Identifier::try_from(
    ///     "entities",
    /// )?));
    /// let statement = Statement::Call(
    ///     call::Builder::default()
    ///         .name(Identifier::from(singular::Identifier::try_from(
    ///             "hello_world",
    ///         )?))?
    ///         .try_build()?,
    /// );
    ///
    /// let scatter = Builder::default()
    ///     .variable(variable)?
    ///     .iterable(iterable)?
    ///     .push_workflow_execution_statement(statement.clone())
    ///     .try_build()?;
    ///
    /// assert_eq!(scatter.statements().unwrap().len(), 1);
    /// assert_eq!(
    ///     scatter
    ///         .statements()
    ///         .unwrap()
    ///         .iter()
    ///         .next()
    ///         .unwrap()
    ///         .as_ref(),
    ///     &statement
    /// );
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn push_workflow_execution_statement(mut self, statement: Statement) -> Self {
        let statements = match self.statements {
            Some(mut statements) => {
                statements.push(Box::new(statement));
                statements
            }
            None => NonEmpty::new(Box::new(statement)),
        };

        self.statements = Some(statements);
        self
    }

    /// Sets the variable for this [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::identifier::singular;
    /// use ast::v1::document::workflow::execution::statement::call;
    /// use ast::v1::document::workflow::execution::statement::scatter::Builder;
    /// use ast::v1::document::workflow::execution::Statement;
    /// use ast::v1::document::Expression;
    /// use ast::v1::document::Identifier;
    /// use wdl_ast as ast;
    ///
    /// let variable = singular::Identifier::try_from("entity")?;
    /// let iterable = Expression::Literal(Literal::Identifier(singular::Identifier::try_from(
    ///     "entities",
    /// )?));
    /// let statement = Statement::Call(
    ///     call::Builder::default()
    ///         .name(Identifier::from(singular::Identifier::try_from(
    ///             "hello_world",
    ///         )?))?
    ///         .try_build()?,
    /// );
    ///
    /// let scatter = Builder::default()
    ///     .variable(variable.clone())?
    ///     .iterable(iterable)?
    ///     .push_workflow_execution_statement(statement)
    ///     .try_build()?;
    ///
    /// assert_eq!(scatter.variable(), &variable);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn variable(mut self, variable: Identifier) -> Result<Self> {
        if self.variable.is_some() {
            return Err(Error::Multiple(MultipleError::Variable));
        }

        self.variable = Some(variable);
        Ok(self)
    }

    /// Consumes `self` to attempt to build a [`Scatter`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::identifier::singular;
    /// use ast::v1::document::workflow::execution::statement::call;
    /// use ast::v1::document::workflow::execution::statement::scatter::Builder;
    /// use ast::v1::document::workflow::execution::Statement;
    /// use ast::v1::document::Expression;
    /// use ast::v1::document::Identifier;
    /// use wdl_ast as ast;
    ///
    /// let variable = singular::Identifier::try_from("entity")?;
    /// let iterable = Expression::Literal(Literal::Identifier(singular::Identifier::try_from(
    ///     "entities",
    /// )?));
    /// let statement = Statement::Call(
    ///     call::Builder::default()
    ///         .name(Identifier::from(singular::Identifier::try_from(
    ///             "hello_world",
    ///         )?))?
    ///         .try_build()?,
    /// );
    ///
    /// let scatter = Builder::default()
    ///     .variable(variable.clone())?
    ///     .iterable(iterable.clone())?
    ///     .push_workflow_execution_statement(statement.clone())
    ///     .try_build()?;
    ///
    /// assert_eq!(scatter.iterable(), &iterable);
    /// assert_eq!(scatter.variable(), &variable);
    /// assert_eq!(scatter.statements().unwrap().len(), 1);
    /// assert_eq!(
    ///     scatter
    ///         .statements()
    ///         .unwrap()
    ///         .iter()
    ///         .next()
    ///         .unwrap()
    ///         .as_ref(),
    ///     &statement
    /// );
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn try_build(self) -> Result<Scatter> {
        let iterable = self
            .iterable
            .map(Ok)
            .unwrap_or(Err(Error::Missing(MissingError::Iterable)))?;

        let variable = self
            .variable
            .map(Ok)
            .unwrap_or(Err(Error::Missing(MissingError::Variable)))?;

        Ok(Scatter {
            iterable,
            statements: self.statements,
            variable,
        })
    }
}
