//! A builder for [`Task`]s.

use nonempty::NonEmpty;

use crate::v1::document;
use crate::v1::document::identifier::singular::Identifier;
use crate::v1::document::task::Command;
use crate::v1::document::task::Runtime;
use crate::v1::document::task::Task;
use crate::v1::document::PrivateDeclarations;

/// An error that occurs when a required field is missing at build time.
#[derive(Debug)]
pub enum MissingError {
    /// A command was not provided to the [`Builder`].
    Command,

    /// A name was not provided to the [`Builder`].
    Name,
}

impl std::fmt::Display for MissingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MissingError::Command => write!(f, "command"),
            MissingError::Name => write!(f, "name"),
        }
    }
}

impl std::error::Error for MissingError {}

/// An error that occurs when a multiple values were provded for a field that
/// only accepts a single value.
#[derive(Debug)]
pub enum MultipleError {
    /// Attempted to set multiple values for the command field within the
    /// [`Builder`].
    Command,

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

    /// Attempted to set multiple values for the runtime field within the
    /// [`Builder`].
    Runtime,
}

impl std::fmt::Display for MultipleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MultipleError::Command => write!(f, "command"),
            MultipleError::Input => write!(f, "input"),
            MultipleError::Metadata => write!(f, "metadata"),
            MultipleError::Name => write!(f, "name"),
            MultipleError::Output => write!(f, "output"),
            MultipleError::ParameterMetadata => write!(f, "parameter metadata"),
            MultipleError::Runtime => write!(f, "runtime"),
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

/// A builder for a [`Task`].
#[derive(Debug, Default)]
pub struct Builder {
    /// The command.
    command: Option<Command>,

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

    /// Private declarations.
    private_declarations: Option<NonEmpty<document::PrivateDeclarations>>,

    /// The runtime.
    runtime: Option<Runtime>,
}

impl Builder {
    /// Sets the name for the [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::task;
    /// use wdl_ast as ast;
    /// use wdl_grammar as grammar;
    ///
    /// let name = Identifier::try_from(String::from("name"))?;
    /// let contents = "echo 'Hello, world!'"
    ///     .parse::<task::command::Contents>()
    ///     .unwrap();
    /// let command = task::Command::HereDoc(contents);
    /// task::Builder::default().name(name)?;
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ````
    pub fn name(mut self, name: Identifier) -> Result<Self> {
        if self.name.is_some() {
            return Err(Error::Multiple(MultipleError::Name));
        }

        self.name = Some(name);
        Ok(self)
    }

    /// Sets the command for the [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::task;
    /// use wdl_ast as ast;
    /// use wdl_grammar as grammar;
    /// let name = Identifier::try_from(String::from("name"))?;
    /// let contents = "echo 'Hello, world!'"
    ///     .parse::<task::command::Contents>()
    ///     .unwrap();
    /// let command = task::Command::HereDoc(contents);
    /// task::Builder::default().name(name)?.command(command)?;
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ````
    pub fn command(mut self, command: Command) -> Result<Self> {
        if self.command.is_some() {
            return Err(Error::Multiple(MultipleError::Command));
        }

        self.command = Some(command);
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
    /// use ast::v1::document::task;
    /// use ast::v1::document::task::Builder;
    /// use ast::v1::document::Declaration;
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
    ///
    /// let contents = "echo 'Hello, world!'"
    ///     .parse::<task::command::Contents>()
    ///     .unwrap();
    /// let command = task::Command::HereDoc(contents);
    ///
    /// let task = Builder::default()
    ///     .name(Identifier::try_from("hello_world")?)?
    ///     .command(command)?
    ///     .input(input.clone())?
    ///     .try_build()?;
    ///
    /// assert_eq!(task.input(), Some(&input));
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
    /// use ast::v1::document::task;
    /// use ast::v1::document::task::Builder;
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
    ///
    /// let contents = "echo 'Hello, world!'"
    ///     .parse::<task::command::Contents>()
    ///     .unwrap();
    /// let command = task::Command::HereDoc(contents);
    ///
    /// let task = Builder::default()
    ///     .name(Identifier::try_from("hello_world")?)?
    ///     .command(command)?
    ///     .output(output.clone())?
    ///     .try_build()?;
    ///
    /// assert_eq!(task.output(), Some(&output));
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

    /// Sets the metadata for the [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    ///
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::metadata::Value;
    /// use ast::v1::document::task;
    /// use ast::v1::document::task::Builder;
    /// use ast::v1::document::task::Command;
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
    /// let contents = "echo 'Hello, world!'"
    ///     .parse::<task::command::Contents>()
    ///     .unwrap();
    /// let command = Command::HereDoc(contents);
    ///
    /// let task = Builder::default()
    ///     .name(Identifier::try_from("hello_world")?)?
    ///     .command(command)?
    ///     .metadata(metadata)?
    ///     .try_build()?;
    ///
    /// let metadata = task.metadata().unwrap().inner();
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

    /// Sets the parameter metadata for the [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    ///
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::metadata::Value;
    /// use ast::v1::document::task;
    /// use ast::v1::document::task::Builder;
    /// use ast::v1::document::task::Command;
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
    /// let contents = "echo 'Hello, world!'"
    ///     .parse::<task::command::Contents>()
    ///     .unwrap();
    /// let command = Command::HereDoc(contents);
    ///
    /// let task = Builder::default()
    ///     .name(Identifier::try_from("hello_world")?)?
    ///     .command(command)?
    ///     .parameter_metadata(parameter_metadata)?
    ///     .try_build()?;
    ///
    /// let parameter_metadata = task.parameter_metadata().unwrap().inner();
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

    /// Pushes a [private declarations](PrivateDeclarations) into the [`Task`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::declaration::bound;
    /// use ast::v1::document::declaration::r#type::Kind;
    /// use ast::v1::document::declaration::Type;
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::task;
    /// use ast::v1::document::task::Command;
    /// use ast::v1::document::Declaration;
    /// use ast::v1::document::Expression;
    /// use ast::v1::document::PrivateDeclarations;
    /// use nonempty::NonEmpty;
    /// use wdl_ast as ast;
    ///
    /// let name = Identifier::try_from(String::from("name"))?;
    /// let contents = "echo 'Hello, world!'"
    ///     .parse::<task::command::Contents>()
    ///     .unwrap();
    /// let command = Command::HereDoc(contents);
    ///
    /// let declaration = bound::Builder::default()
    ///     .name(Identifier::try_from("hello_world")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .value(Expression::Literal(Literal::None))?
    ///     .try_build()?;
    ///
    /// let private_declarations = PrivateDeclarations::from(NonEmpty::new(declaration.clone()));
    ///
    /// let task = task::Builder::default()
    ///     .name(name)?
    ///     .command(command)?
    ///     .push_private_declarations(private_declarations.clone())
    ///     .try_build()
    ///     .unwrap();
    ///
    /// assert_eq!(task.private_declarations().unwrap(), &private_declarations);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn push_private_declarations(
        mut self,
        declarations: document::PrivateDeclarations,
    ) -> Self {
        let declarations = match self.private_declarations {
            Some(mut existing) => {
                existing.push(declarations);
                existing
            }
            None => NonEmpty::new(declarations),
        };

        self.private_declarations = Some(declarations);
        self
    }

    /// Sets the runtime for the [`Task`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::task;
    /// use ast::v1::document::task::runtime::Builder;
    /// use ast::v1::document::task::runtime::Value;
    /// use ast::v1::document::task::Command;
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
    /// let name = Identifier::try_from(String::from("name"))?;
    /// let command = Command::HereDoc("echo 'Hello, world!'".parse::<task::command::Contents>()?);
    ///
    /// let task = task::Builder::default()
    ///     .name(name)?
    ///     .command(command)?
    ///     .runtime(runtime.clone())?
    ///     .try_build()
    ///     .unwrap();
    ///
    /// assert_eq!(task.runtime(), Some(&runtime));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn runtime(mut self, runtime: Runtime) -> Result<Self> {
        if self.runtime.is_some() {
            return Err(Error::Multiple(MultipleError::Runtime));
        }

        self.runtime = Some(runtime);
        Ok(self)
    }

    /// Consumes `self` to attempt to build a [`Task`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::task;
    /// use wdl_ast as ast;
    /// use wdl_grammar as grammar;
    ///
    /// let name = Identifier::from(Identifier::try_from(String::from("name"))?);
    /// let contents = "".parse::<task::command::Contents>()?;
    /// let command = task::Command::HereDoc(contents);
    ///
    /// let task = task::Builder::default()
    ///     .name(name.clone())?
    ///     .command(command)?
    ///     .try_build()?;
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ````
    pub fn try_build(self) -> Result<Task> {
        let command = self
            .command
            .map(Ok)
            .unwrap_or(Err(Error::Missing(MissingError::Command)))?;

        let name = self
            .name
            .map(Ok)
            .unwrap_or(Err(Error::Missing(MissingError::Name)))?;

        // Before building the final [`Task`], we must flatten the private
        // declarations into a single [`PrivateDeclarations`].
        let private_declarations = self
            .private_declarations
            .into_iter()
            .flat_map(|declarations| {
                declarations.flat_map(|declarations| declarations.into_inner())
            })
            .collect::<Vec<_>>();

        let private_declarations = if !private_declarations.is_empty() {
            // SAFETY: The check above ensures that the declarations aren't
            // empty, so unwrapping is safe here.
            let mut private_declarations = private_declarations.into_iter();

            let mut result = NonEmpty::new(private_declarations.next().unwrap());
            result.extend(private_declarations);

            Some(PrivateDeclarations::from(result))
        } else {
            None
        };

        Ok(Task {
            command,
            input: self.input,
            metadata: self.metadata,
            name,
            output: self.output,
            parameter_metadata: self.parameter_metadata,
            private_declarations,
            runtime: self.runtime,
        })
    }
}
