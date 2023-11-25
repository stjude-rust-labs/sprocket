//! Builder for a [`Conditional`].

use nonempty::NonEmpty;

use crate::v1::document::workflow::execution::statement::Conditional;
use crate::v1::document::workflow::execution::Statement;
use crate::v1::document::Expression;

/// An error that occurs when a required field is missing at build time.
#[derive(Debug)]
pub enum MissingError {
    /// A condition clause was not provided to the [`Builder`].
    Condition,
}

impl std::fmt::Display for MissingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MissingError::Condition => write!(f, "condition"),
        }
    }
}

impl std::error::Error for MissingError {}

/// An error that occurs when a multiple values were provded for a field that
/// only accepts a single value.
#[derive(Debug)]
pub enum MultipleError {
    /// Attempted to set multiple values for the condition clause field within
    /// the [`Builder`].
    Condition,
}

impl std::fmt::Display for MultipleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MultipleError::Condition => write!(f, "condition"),
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

/// A builder for a [`Conditional`].
#[derive(Debug, Default)]
pub struct Builder {
    /// The condition clause.
    condition: Option<Expression>,

    /// The workflow execution statements.
    statements: Option<NonEmpty<Box<Statement>>>,
}

impl Builder {
    /// Sets the condition clause for this [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::workflow::execution::statement::conditional::Builder;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let condition = Expression::Literal(Literal::Boolean(true));
    /// let conditional = Builder::default()
    ///     .condition(condition.clone())?
    ///     .try_build()?;
    ///
    /// assert_eq!(conditional.condition(), &condition);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn condition(mut self, condition: Expression) -> Result<Self> {
        if self.condition.is_some() {
            return Err(Error::Multiple(MultipleError::Condition));
        }

        self.condition = Some(condition);
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
    /// use ast::v1::document::workflow::execution::statement::conditional::Builder;
    /// use ast::v1::document::workflow::execution::Statement;
    /// use ast::v1::document::Expression;
    /// use ast::v1::document::Identifier;
    /// use wdl_ast as ast;
    ///
    /// let statement = Statement::Call(
    ///     call::Builder::default()
    ///         .name(Identifier::from(singular::Identifier::try_from(
    ///             "hello_world",
    ///         )?))?
    ///         .try_build()?,
    /// );
    ///
    /// let condition = Expression::Literal(Literal::Boolean(true));
    ///
    /// let conditional = Builder::default()
    ///     .condition(condition)?
    ///     .push_workflow_execution_statement(statement.clone())
    ///     .try_build()?;
    ///
    /// assert_eq!(conditional.statements().unwrap().len(), 1);
    /// assert_eq!(
    ///     conditional
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

    /// Consumes `self` to attempt to build a [`Conditional`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::identifier::singular;
    /// use ast::v1::document::workflow::execution::statement::call;
    /// use ast::v1::document::workflow::execution::statement::conditional::Builder;
    /// use ast::v1::document::workflow::execution::Statement;
    /// use ast::v1::document::Expression;
    /// use ast::v1::document::Identifier;
    /// use wdl_ast as ast;
    ///
    /// let statement = Statement::Call(
    ///     call::Builder::default()
    ///         .name(Identifier::from(singular::Identifier::try_from(
    ///             "hello_world",
    ///         )?))?
    ///         .try_build()?,
    /// );
    ///
    /// let condition = Expression::Literal(Literal::Boolean(true));
    ///
    /// let conditional = Builder::default()
    ///     .condition(condition.clone())?
    ///     .push_workflow_execution_statement(statement.clone())
    ///     .try_build()?;
    ///
    /// assert_eq!(conditional.condition(), &condition);
    /// assert_eq!(conditional.statements().unwrap().len(), 1);
    /// assert_eq!(
    ///     conditional
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
    pub fn try_build(self) -> Result<Conditional> {
        let condition = self
            .condition
            .map(Ok)
            .unwrap_or(Err(Error::Missing(MissingError::Condition)))?;

        Ok(Conditional {
            condition,
            statements: self.statements,
        })
    }
}
