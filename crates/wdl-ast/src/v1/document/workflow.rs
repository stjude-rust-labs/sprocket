//! Workflows.

use grammar::v1::Rule;
use nonempty::NonEmpty;
use pest::iterators::Pair;
use wdl_grammar as grammar;
use wdl_macros::check_node;

use crate::v1::document;
use crate::v1::document::identifier::singular;
use crate::v1::document::identifier::singular::Identifier;
use crate::v1::document::workflow;

mod builder;
pub mod execution;

pub use builder::Builder;

/// An error related to a [`Workflow`].
#[derive(Debug)]
pub enum Error {
    /// A builder error.
    Builder(builder::Error),

    /// An execution statement error.
    ExecutionStatement(execution::statement::Error),

    /// An identifier error.
    Identifier(singular::Error),

    /// A input error.
    Input(document::input::Error),

    /// A metadata error.
    Metadata(document::metadata::Error),

    /// An output error.
    Output(document::output::Error),

    /// A parameter metadata error.
    ParameterMetadata(document::metadata::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Builder(err) => write!(f, "builder error: {err}"),
            Error::ExecutionStatement(err) => {
                write!(f, "execution statement error: {err}")
            }
            Error::Identifier(err) => write!(f, "identifier error: {err}"),
            Error::Input(err) => write!(f, "input error: {err}"),
            Error::Metadata(err) => write!(f, "metadata error: {err}"),
            Error::Output(err) => write!(f, "output error: {err}"),
            Error::ParameterMetadata(err) => {
                write!(f, "parameter metadata error: {err}")
            }
        }
    }
}

impl std::error::Error for Error {}

/// A workflow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Workflow {
    /// The input.
    input: Option<document::Input>,

    /// The metadata.
    metadata: Option<document::Metadata>,

    /// The name.
    name: Identifier,

    /// The output.
    output: Option<document::Output>,

    /// The parameter metadata.
    parameter_metadata: Option<document::Metadata>,

    /// The workflow execution statements.
    statements: Option<NonEmpty<execution::Statement>>,
}

impl Workflow {
    /// Gets the input from the [`Workflow`] by reference (if it exists).
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
    /// use wdl_ast as ast;
    /// use wdl_core::fs::location::Located;
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
    pub fn input(&self) -> Option<&document::Input> {
        self.input.as_ref()
    }

    /// Gets the metadata from the [`Workflow`] by reference (if it exists).
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
    /// use wdl_core::fs::location::Located;
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
    pub fn metadata(&self) -> Option<&document::Metadata> {
        self.metadata.as_ref()
    }

    /// Gets the name from the [`Workflow`] by reference.
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
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    /// Gets the output from the [`Workflow`] by reference (if it exists).
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
    pub fn output(&self) -> Option<&document::Output> {
        self.output.as_ref()
    }

    /// Gets the parameter metadata from the [`Workflow`] by reference (if it
    /// exists).
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
    /// use wdl_core::fs::location::Located;
    ///
    /// let mut map = BTreeMap::new();
    /// map.insert(
    ///     Located::unplaced(Identifier::try_from(String::from("baz")).unwrap()),
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
    pub fn parameter_metadata(&self) -> Option<&document::Metadata> {
        self.parameter_metadata.as_ref()
    }

    /// Gets the workflow execution statements from the [`Workflow`] by
    /// reference (if they exist).
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
    pub fn statements(&self) -> Option<&NonEmpty<execution::Statement>> {
        self.statements.as_ref()
    }
}

impl TryFrom<Pair<'_, grammar::v1::Rule>> for Workflow {
    type Error = Error;

    fn try_from(node: Pair<'_, grammar::v1::Rule>) -> Result<Self, Self::Error> {
        check_node!(node, workflow);
        let mut builder = builder::Builder::default();

        for node in node.into_inner() {
            match node.as_rule() {
                Rule::workflow_name => {
                    let name = Identifier::try_from(node.as_str().to_owned())
                        .map_err(Error::Identifier)?;
                    builder = builder.name(name).map_err(Error::Builder)?;
                }
                Rule::input => {
                    let input = document::Input::try_from(node).map_err(Error::Input)?;
                    builder = builder.input(input).map_err(Error::Builder)?;
                }
                Rule::workflow_execution_statement => {
                    let statement = workflow::execution::Statement::try_from(node)
                        .map_err(Error::ExecutionStatement)?;
                    builder = builder.push_workflow_execution_statement(statement);
                }
                Rule::output => {
                    let output = document::Output::try_from(node).map_err(Error::Output)?;
                    builder = builder.output(output).map_err(Error::Builder)?;
                }
                Rule::metadata => {
                    let metadata = document::Metadata::try_from(node).map_err(Error::Metadata)?;
                    builder = builder.metadata(metadata).map_err(Error::Builder)?;
                }
                Rule::parameter_metadata => {
                    let parameter_metadata =
                        document::Metadata::try_from(node).map_err(Error::ParameterMetadata)?;
                    builder = builder
                        .parameter_metadata(parameter_metadata)
                        .map_err(Error::Builder)?;
                }
                Rule::COMMENT => {}
                Rule::WHITESPACE => {}
                rule => unreachable!("workflow should not contain {:?}", rule),
            }
        }

        builder.try_build().map_err(Error::Builder)
    }
}
