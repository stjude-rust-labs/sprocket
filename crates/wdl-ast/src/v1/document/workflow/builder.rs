//! Builder for a [`Workflow`].

use nonempty::NonEmpty;

use crate::v1::document;
use crate::v1::document::identifier::singular::Identifier;
use crate::v1::document::workflow::execution;
use crate::v1::document::Workflow;

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
    /// Attempted to set multiple values for the input field within the
    /// [`Builder`].
    Input,

    /// Attempted to set multiple values for the metadata field within the
    /// [`Builder`].
    Metadata,

    /// Attempted to set multiple values for the name field within the
    /// [`Builder`].
    Name,

    /// Attempted to set multiple values for the output field within the
    /// [`Builder`].
    Output,

    /// Attempted to set multiple values for the parameter metadata field
    /// within the [`Builder`].
    ParameterMetadata,
}

impl std::fmt::Display for MultipleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MultipleError::Input => write!(f, "input"),
            MultipleError::Metadata => write!(f, "metadata"),
            MultipleError::Name => write!(f, "name"),
            MultipleError::Output => write!(f, "output"),
            MultipleError::ParameterMetadata => write!(f, "parameter metadata"),
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

/// A builder for a [`Workflow`].
#[derive(Debug, Default)]
pub struct Builder {
    /// The input.
    input: Option<document::Input>,

    /// The metadata.
    metadata: Option<document::Metadata>,

    /// The name.
    name: Option<Identifier>,

    /// The output.
    output: Option<document::Output>,

    /// The parameter metadata.
    parameter_metadata: Option<document::Metadata>,

    /// The workflow execution statements.
    statements: Option<NonEmpty<execution::Statement>>,
}

impl Builder {
    /// Sets the name for the [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::workflow::Builder;
    /// use wdl_ast as ast;
    ///
    /// let workflow = Builder::default()
    ///     .name(Identifier::try_from("hello_world")?)?
    ///     .try_build()?;
    ///
    /// assert_eq!(workflow.name().as_str(), "hello_world");
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

    /// Sets the input for the [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::declaration::r#type::Kind;
    /// use ast::v1::document::declaration::unbound;
    /// use ast::v1::document::declaration::Type;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::input;
    /// use ast::v1::document::workflow::Builder;
    /// use ast::v1::document::Declaration;
    /// use ast::v1::document::Input;
    /// use wdl_ast as ast;
    /// use wdl_core::file::location::Located;
    ///
    /// let declaration = unbound::Builder::default()
    ///     .name(Identifier::try_from("hello_world")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .try_build()?;
    /// let mut input = input::Builder::default()
    ///     .push_declaration(Located::unplaced(Declaration::Unbound(declaration)))
    ///     .build();
    /// let workflow = Builder::default()
    ///     .name(Identifier::try_from("hello_world")?)?
    ///     .input(input.clone())?
    ///     .try_build()?;
    ///
    /// assert_eq!(workflow.input(), Some(&input));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn input(mut self, input: document::Input) -> Result<Self> {
        if self.input.is_some() {
            return Err(Error::Multiple(MultipleError::Input));
        }

        self.input = Some(input);
        Ok(self)
    }

    /// Sets the output for the [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::declaration::bound;
    /// use ast::v1::document::declaration::r#type::Kind;
    /// use ast::v1::document::declaration::Type;
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::output;
    /// use ast::v1::document::workflow::Builder;
    /// use ast::v1::document::Declaration;
    /// use ast::v1::document::Expression;
    /// use ast::v1::document::Output;
    /// use wdl_ast as ast;
    ///
    /// let declaration = bound::Builder::default()
    ///     .name(Identifier::try_from("hello_world")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .value(Expression::Literal(Literal::Identifier(
    ///         Identifier::try_from("foo").unwrap(),
    ///     )))?
    ///     .try_build()?;
    /// let output = output::Builder::default()
    ///     .push_bound_declaration(declaration)
    ///     .build();
    /// let workflow = Builder::default()
    ///     .name(Identifier::try_from("hello_world")?)?
    ///     .output(output.clone())?
    ///     .try_build()?;
    ///
    /// assert_eq!(workflow.output(), Some(&output));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn output(mut self, output: document::Output) -> Result<Self> {
        if self.output.is_some() {
            return Err(Error::Multiple(MultipleError::Output));
        }

        self.output = Some(output);
        Ok(self)
    }

    /// Pushes a workflow execution statement into the [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::identifier::singular;
    /// use ast::v1::document::workflow::execution::statement::call;
    /// use ast::v1::document::workflow::execution::Statement;
    /// use ast::v1::document::workflow::Builder;
    /// use ast::v1::document::Identifier;
    /// use wdl_ast as ast;
    ///
    /// let call = call::Builder::default()
    ///     .name(Identifier::from(singular::Identifier::try_from("foo")?))?
    ///     .try_build()?;
    ///
    /// let statement = Statement::Call(call);
    ///
    /// let workflow = Builder::default()
    ///     .name(singular::Identifier::try_from("hello_world")?)?
    ///     .push_workflow_execution_statement(statement)
    ///     .try_build()?;
    /// assert_eq!(workflow.statements().unwrap().len(), 1);
    ///
    /// let call = match workflow.statements().unwrap().into_iter().next().unwrap() {
    ///     Statement::Call(call) => call,
    ///     _ => unreachable!(),
    /// };
    ///
    /// assert_eq!(call.name().to_string(), "foo");
    /// assert_eq!(call.body(), None);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn push_workflow_execution_statement(mut self, statement: execution::Statement) -> Self {
        let statements = match self.statements {
            Some(mut statements) => {
                statements.push(statement);
                statements
            }
            None => NonEmpty::new(statement),
        };

        self.statements = Some(statements);
        self
    }

    /// Sets the metadata for the [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    ///
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::metadata::Value;
    /// use ast::v1::document::workflow::Builder;
    /// use ast::v1::document::Metadata;
    /// use wdl_ast as ast;
    /// use wdl_core::file::location::Located;
    ///
    /// let mut map = BTreeMap::new();
    /// map.insert(
    ///     Located::unplaced(Identifier::try_from(String::from("foo"))?),
    ///     Located::unplaced(Value::String(String::from("bar"))),
    /// );
    ///
    /// let metadata = Metadata::from(map);
    ///
    /// let workflow = Builder::default()
    ///     .name(Identifier::try_from("hello_world")?)?
    ///     .metadata(metadata)?
    ///     .try_build()?;
    ///
    /// let metadata = workflow.metadata().unwrap().inner();
    ///
    /// assert_eq!(
    ///     metadata
    ///         .get(&Identifier::try_from("foo").unwrap())
    ///         .unwrap()
    ///         .inner(),
    ///     &Value::String(String::from("bar"))
    /// );
    /// assert_eq!(metadata.get(&Identifier::try_from("baz").unwrap()), None);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn metadata(mut self, metadata: document::Metadata) -> Result<Self> {
        if self.metadata.is_some() {
            return Err(Error::Multiple(MultipleError::Metadata));
        }

        self.metadata = Some(metadata);
        Ok(self)
    }

    /// Pushes a parameter metadata into the [`Builder`].
    ///
    /// **Note:** although the convention is to only ever include _one_
    /// `parameter_meta` section, technically the specification for WDL v1.x
    /// allows for multiple `parameter_meta` blocks.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    ///
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::metadata::Value;
    /// use ast::v1::document::workflow::Builder;
    /// use ast::v1::document::Metadata;
    /// use wdl_ast as ast;
    /// use wdl_core::file::location::Located;
    ///
    /// let mut map = BTreeMap::new();
    /// map.insert(
    ///     Located::unplaced(Identifier::try_from(String::from("baz"))?),
    ///     Located::unplaced(Value::String(String::from("quux"))),
    /// );
    ///
    /// let parameter_metadata = Metadata::from(map);
    ///
    /// let workflow = Builder::default()
    ///     .name(Identifier::try_from("hello_world")?)?
    ///     .parameter_metadata(parameter_metadata)?
    ///     .try_build()?;
    ///
    /// let parameter_metadata = workflow.parameter_metadata().unwrap().inner();
    ///
    /// assert_eq!(
    ///     parameter_metadata
    ///         .get(&Identifier::try_from("baz").unwrap())
    ///         .unwrap()
    ///         .inner(),
    ///     &Value::String(String::from("quux"))
    /// );
    /// assert_eq!(
    ///     parameter_metadata.get(&Identifier::try_from("foo").unwrap()),
    ///     None
    /// );
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn parameter_metadata(mut self, metadata: document::Metadata) -> Result<Self> {
        if self.parameter_metadata.is_some() {
            return Err(Error::Multiple(MultipleError::ParameterMetadata));
        }

        self.parameter_metadata = Some(metadata);
        Ok(self)
    }

    /// Consumes `self` to attempt to build a [`Workflow`].
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    ///
    /// use ast::v1::document::declaration::bound;
    /// use ast::v1::document::declaration::r#type::Kind;
    /// use ast::v1::document::declaration::unbound;
    /// use ast::v1::document::declaration::Type;
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::identifier::singular;
    /// use ast::v1::document::input;
    /// use ast::v1::document::metadata;
    /// use ast::v1::document::metadata::Value;
    /// use ast::v1::document::output;
    /// use ast::v1::document::workflow::execution::statement::call;
    /// use ast::v1::document::workflow::execution::Statement;
    /// use ast::v1::document::workflow::Builder;
    /// use ast::v1::document::Declaration;
    /// use ast::v1::document::Expression;
    /// use ast::v1::document::Identifier;
    /// use ast::v1::document::Input;
    /// use ast::v1::document::Metadata;
    /// use ast::v1::document::Output;
    /// use nonempty::NonEmpty;
    /// use wdl_ast as ast;
    /// use wdl_core::file::location::Located;
    ///
    /// // Creating the input.
    /// let declaration = unbound::Builder::default()
    ///     .name(singular::Identifier::try_from("hello_world")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .try_build()?;
    /// let mut input = input::Builder::default()
    ///     .push_declaration(Located::unplaced(Declaration::Unbound(declaration)))
    ///     .build();
    ///
    /// // Creating the output.
    /// let declaration = bound::Builder::default()
    ///     .name(singular::Identifier::try_from("hello_world")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .value(Expression::Literal(Literal::Identifier(
    ///         singular::Identifier::try_from("foo").unwrap(),
    ///     )))?
    ///     .try_build()?;
    /// let output = output::Builder::default()
    ///     .push_bound_declaration(declaration)
    ///     .build();
    ///
    /// // Creating the workflow execution statement.
    /// let call = call::Builder::default()
    ///     .name(Identifier::from(singular::Identifier::try_from("foo")?))?
    ///     .try_build()?;
    ///
    /// let statement = Statement::Call(call);
    ///
    /// // Creating the metadata.
    /// let mut metadata_map = BTreeMap::new();
    /// metadata_map.insert(
    ///     Located::unplaced(singular::Identifier::try_from(String::from("foo"))?),
    ///     Located::unplaced(Value::String(String::from("bar"))),
    /// );
    /// let metadata = Metadata::from(metadata_map);
    ///
    /// // Creating the parameter metadata.
    /// let mut parameter_metadata_map = BTreeMap::new();
    /// parameter_metadata_map.insert(
    ///     Located::unplaced(singular::Identifier::try_from(String::from("baz"))?),
    ///     Located::unplaced(Value::String(String::from("quux"))),
    /// );
    /// let parameter_metadata = Metadata::from(parameter_metadata_map);
    ///
    /// // Building the workflow.
    /// let workflow = Builder::default()
    ///     .name(singular::Identifier::try_from("hello_world")?)?
    ///     .input(input.clone())?
    ///     .output(output.clone())?
    ///     .push_workflow_execution_statement(statement)
    ///     .metadata(metadata)?
    ///     .parameter_metadata(parameter_metadata)?
    ///     .try_build()?;
    ///
    /// // Check workflow name.
    /// assert_eq!(workflow.name().as_str(), "hello_world");
    ///
    /// // Check workflow input.
    /// assert_eq!(workflow.input(), Some(&input));
    ///
    /// // Check workflow output.
    /// assert_eq!(workflow.output(), Some(&output));
    ///
    /// // Check workflow execution statements.
    /// let call = match workflow.statements().unwrap().into_iter().next().unwrap() {
    ///     Statement::Call(call) => call,
    ///     _ => unreachable!(),
    /// };
    ///
    /// assert_eq!(call.name().to_string(), "foo");
    /// assert_eq!(call.body(), None);
    ///
    /// // Check workflow metadata.
    /// let metadata = workflow.metadata().unwrap().inner();
    ///
    /// assert_eq!(
    ///     metadata
    ///         .get(&singular::Identifier::try_from("foo").unwrap())
    ///         .unwrap()
    ///         .inner(),
    ///     &Value::String(String::from("bar"))
    /// );
    /// assert_eq!(
    ///     metadata.get(&singular::Identifier::try_from("baz").unwrap()),
    ///     None
    /// );
    ///
    /// // Check workflow parameter metadata.
    /// let parameter_metadata = workflow.parameter_metadata().unwrap().inner();
    ///
    /// assert_eq!(
    ///     parameter_metadata
    ///         .get(&singular::Identifier::try_from("baz").unwrap())
    ///         .unwrap()
    ///         .inner(),
    ///     &Value::String(String::from("quux"))
    /// );
    /// assert_eq!(
    ///     parameter_metadata.get(&singular::Identifier::try_from("foo").unwrap()),
    ///     None
    /// );
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn try_build(self) -> Result<Workflow> {
        let name = self
            .name
            .map(Ok)
            .unwrap_or(Err(Error::Missing(MissingError::Name)))?;

        Ok(Workflow {
            input: self.input,
            metadata: self.metadata,
            name,
            output: self.output,
            parameter_metadata: self.parameter_metadata,
            statements: self.statements,
        })
    }
}
