//! Tasks.

use grammar::v1::Rule;
use pest::iterators::Pair;
use wdl_grammar as grammar;

mod builder;
pub mod command;
pub mod runtime;

pub use builder::Builder;
pub use command::Command;
pub use runtime::Runtime;
use wdl_macros::check_node;
use wdl_macros::unwrap_one;

use crate::v1::document;
use crate::v1::document::identifier::singular;
use crate::v1::document::identifier::singular::Identifier;
use crate::v1::document::Input;
use crate::v1::document::Metadata;
use crate::v1::document::Output;
use crate::v1::document::PrivateDeclarations;

/// An error related to a [`Task`].
#[derive(Debug)]
pub enum Error {
    /// A builder error.
    Builder(builder::Error),

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

    /// A private declarations error.
    PrivateDeclarations(document::private_declarations::Error),

    /// A runtime error.
    Runtime(runtime::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Builder(err) => write!(f, "builder error: {err}"),
            Error::Identifier(err) => write!(f, "identifier error: {err}"),
            Error::Input(err) => write!(f, "input error: {err}"),
            Error::Metadata(err) => write!(f, "metadata error: {err}"),
            Error::Output(err) => write!(f, "output error: {err}"),
            Error::ParameterMetadata(err) => {
                write!(f, "parameter metadata error: {err}")
            }
            Error::PrivateDeclarations(err) => {
                write!(f, "private declarations error: {err}")
            }
            Error::Runtime(err) => write!(f, "runtime error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A task.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Task {
    /// The command.
    command: Command,

    /// The input.
    input: Option<Input>,

    /// The metadata.
    metadata: Option<Metadata>,

    /// The name.
    name: Identifier,

    /// The output.
    output: Option<Output>,

    /// The parameter metadata.
    parameter_metadata: Option<Metadata>,

    /// Private declarations.
    private_declarations: Option<PrivateDeclarations>,

    /// The runtime.
    runtime: Option<Runtime>,
}

impl Task {
    /// Gets the command from the [`Task`] by reference.
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
    /// let contents = "echo 'Hello, world!'".parse::<task::command::Contents>()?;
    /// let command = task::Command::HereDoc(contents);
    ///
    /// let task = task::Builder::default()
    ///     .name(name)?
    ///     .command(command.clone())?
    ///     .try_build()?;
    ///
    /// assert_eq!(task.command(), &command);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ````
    pub fn command(&self) -> &Command {
        &self.command
    }

    /// Gets the input from the [`Task`] by reference (if it exists).
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::declaration::bound;
    /// use ast::v1::document::declaration::r#type::Kind;
    /// use ast::v1::document::declaration::unbound;
    /// use ast::v1::document::declaration::Type;
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::input;
    /// use ast::v1::document::task;
    /// use ast::v1::document::task::Command;
    /// use ast::v1::document::Declaration;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    /// use wdl_core::file::location::Located;
    /// use wdl_grammar as grammar;
    ///
    /// let name = Identifier::try_from(String::from("name"))?;
    /// let contents = "echo 'Hello, world!'"
    ///     .parse::<task::command::Contents>()
    ///     .unwrap();
    /// let command = Command::HereDoc(contents);
    ///
    /// let bound = bound::Builder::default()
    ///     .name(Identifier::try_from("hello_world")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .value(Expression::Literal(Literal::None))?
    ///     .try_build()?;
    ///
    /// let unbound = unbound::Builder::default()
    ///     .name(Identifier::try_from("foo_bar")?)?
    ///     .r#type(Type::new(Kind::Boolean, true))?
    ///     .try_build()?;
    ///
    /// let mut input = input::Builder::default()
    ///     .push_declaration(Located::unplaced(Declaration::Bound(bound.clone())))
    ///     .push_declaration(Located::unplaced(Declaration::Unbound(unbound.clone())))
    ///     .build();
    ///
    /// let task = task::Builder::default()
    ///     .name(name)?
    ///     .command(command)?
    ///     .input(input.clone())?
    ///     .try_build()?;
    ///
    /// assert_eq!(task.input().unwrap(), &input);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ````
    pub fn input(&self) -> Option<&Input> {
        self.input.as_ref()
    }

    /// Gets the metadata from the [`Task`] by reference (if it exists).
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    ///
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::metadata::Value;
    /// use ast::v1::document::task;
    /// use ast::v1::document::task::Command;
    /// use ast::v1::document::Metadata;
    /// use wdl_ast as ast;
    /// use wdl_core::file::location::Located;
    ///
    /// let name = Identifier::try_from(String::from("name"))?;
    /// let contents = "echo 'Hello, world!'".parse::<task::command::Contents>()?;
    /// let command = Command::HereDoc(contents);
    ///
    /// let mut map = BTreeMap::new();
    /// map.insert(
    ///     Located::unplaced(Identifier::try_from("hello")?),
    ///     Located::unplaced(Value::String(String::from("world"))),
    /// );
    /// map.insert(
    ///     Located::unplaced(Identifier::try_from("foo")?),
    ///     Located::unplaced(Value::Null),
    /// );
    ///
    /// let metadata = Metadata::from(map);
    ///
    /// let task = task::Builder::default()
    ///     .name(name)?
    ///     .command(command)?
    ///     .metadata(metadata.clone())?
    ///     .try_build()?;
    ///
    /// assert_eq!(task.metadata().unwrap(), &metadata);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn metadata(&self) -> Option<&Metadata> {
        self.metadata.as_ref()
    }

    /// Gets the name from the [`Task`] by reference.
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
    /// let contents = "echo 'Hello, world!'".parse::<task::command::Contents>()?;
    /// let command = task::Command::HereDoc(contents);
    ///
    /// let task = task::Builder::default()
    ///     .name(name.clone())?
    ///     .command(command)?
    ///     .try_build()
    ///     .unwrap();
    ///
    /// assert_eq!(task.name(), &name);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ````
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    /// Gets the output from the [`Task`] by reference (if it exists).
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
    /// use ast::v1::document::task::Command;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    /// use wdl_grammar as grammar;
    ///
    /// let name = Identifier::try_from(String::from("name"))?;
    /// let contents = "echo 'Hello, world!'"
    ///     .parse::<task::command::Contents>()
    ///     .unwrap();
    /// let command = Command::HereDoc(contents);
    ///
    /// let bound = bound::Builder::default()
    ///     .name(Identifier::try_from("hello_world")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .value(Expression::Literal(Literal::None))?
    ///     .try_build()?;
    ///
    /// let mut output = output::Builder::default()
    ///     .push_bound_declaration(bound.clone())
    ///     .build();
    ///
    /// let task = task::Builder::default()
    ///     .name(name)?
    ///     .command(command)?
    ///     .output(output.clone())?
    ///     .try_build()
    ///     .unwrap();
    ///
    /// assert_eq!(task.output().unwrap(), &output);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ````
    pub fn output(&self) -> Option<&Output> {
        self.output.as_ref()
    }

    /// Gets the parameter metadata from the [`Task`] by reference (if it
    /// exists).
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    ///
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::metadata::Value;
    /// use ast::v1::document::task;
    /// use ast::v1::document::task::Command;
    /// use ast::v1::document::Metadata;
    /// use wdl_ast as ast;
    /// use wdl_core::file::location::Located;
    ///
    /// let name = Identifier::try_from(String::from("name"))?;
    /// let contents = "echo 'Hello, world!'"
    ///     .parse::<task::command::Contents>()
    ///     .unwrap();
    /// let command = Command::HereDoc(contents);
    ///
    /// let mut map = BTreeMap::new();
    /// map.insert(
    ///     Located::unplaced(Identifier::try_from("hello").unwrap()),
    ///     Located::unplaced(Value::String(String::from("world"))),
    /// );
    /// map.insert(
    ///     Located::unplaced(Identifier::try_from("foo").unwrap()),
    ///     Located::unplaced(Value::Null),
    /// );
    ///
    /// let parameter_metadata = Metadata::from(map);
    ///
    /// let task = task::Builder::default()
    ///     .name(name)?
    ///     .command(command)?
    ///     .parameter_metadata(parameter_metadata.clone())?
    ///     .try_build()
    ///     .unwrap();
    ///
    /// assert_eq!(task.parameter_metadata().unwrap(), &parameter_metadata);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn parameter_metadata(&self) -> Option<&Metadata> {
        self.parameter_metadata.as_ref()
    }

    /// Gets the [private declarations](PrivateDeclarations) from the [`Task`]
    /// by reference (if they exist).
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
    pub fn private_declarations(&self) -> Option<&PrivateDeclarations> {
        self.private_declarations.as_ref()
    }

    /// Gets the [runtime](Runtime) for the [`Task`] by reference (if it
    /// exists).
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
    pub fn runtime(&self) -> Option<&Runtime> {
        self.runtime.as_ref()
    }
}

impl TryFrom<Pair<'_, grammar::v1::Rule>> for Task {
    type Error = Error;

    fn try_from(node: Pair<'_, grammar::v1::Rule>) -> Result<Self, Self::Error> {
        check_node!(node, task);
        let mut builder = builder::Builder::default();

        for node in node.into_inner() {
            match node.as_rule() {
                Rule::singular_identifier => {
                    let identifier = Identifier::try_from(node.as_str().to_owned())
                        .map_err(Error::Identifier)?;
                    builder = builder.name(identifier).map_err(Error::Builder)?;
                }
                Rule::task_element => {
                    let node = unwrap_one!(node, task_element);

                    match node.as_rule() {
                        Rule::input => {
                            let input = Input::try_from(node).map_err(Error::Input)?;
                            builder = builder.input(input).map_err(Error::Builder)?;
                        }
                        Rule::metadata => {
                            let metadata = Metadata::try_from(node).map_err(Error::Metadata)?;
                            builder = builder.metadata(metadata).map_err(Error::Builder)?;
                        }
                        Rule::output => {
                            let output = Output::try_from(node).map_err(Error::Output)?;
                            builder = builder.output(output).map_err(Error::Builder)?;
                        }
                        Rule::parameter_metadata => {
                            let parameter_metadata =
                                Metadata::try_from(node).map_err(Error::ParameterMetadata)?;
                            builder = builder
                                .parameter_metadata(parameter_metadata)
                                .map_err(Error::Builder)?;
                        }
                        Rule::private_declarations => {
                            let declarations = PrivateDeclarations::try_from(node)
                                .map_err(Error::PrivateDeclarations)?;
                            builder = builder.push_private_declarations(declarations);
                        }
                        Rule::task_command => {
                            let command = Command::from(node);
                            builder = builder.command(command).map_err(Error::Builder)?;
                        }
                        Rule::task_runtime => {
                            let runtime = Runtime::try_from(node).map_err(Error::Runtime)?;
                            builder = builder.runtime(runtime).map_err(Error::Builder)?;
                        }
                        rule => unreachable!("task element should not contain {:?}", rule),
                    }
                }
                Rule::COMMENT => {}
                Rule::WHITESPACE => {}
                rule => unreachable!("task should not contain {:?}", rule),
            }
        }

        builder.try_build().map_err(Error::Builder)
    }
}

#[cfg(test)]
mod tests {
    use nonempty::NonEmpty;

    use super::*;
    use crate::v1::document::declaration::bound;
    use crate::v1::document::declaration::r#type::Kind;
    use crate::v1::document::declaration::Type;
    use crate::v1::document::expression::Literal;
    use crate::v1::document::task;
    use crate::v1::document::Expression;

    #[test]
    fn multiple_private_declarations_are_squashed() -> Result<(), Box<dyn std::error::Error>> {
        let name = Identifier::try_from(String::from("name"))?;
        let contents = "echo 'Hello, world!'"
            .parse::<task::command::Contents>()
            .unwrap();
        let command = Command::HereDoc(contents);

        let one = bound::Builder::default()
            .name(Identifier::try_from("hello_world")?)?
            .r#type(Type::new(Kind::Boolean, false))?
            .value(Expression::Literal(Literal::None))?
            .try_build()?;

        let two = bound::Builder::default()
            .name(Identifier::try_from("foo")?)?
            .r#type(Type::new(Kind::String, false))?
            .value(Expression::Literal(Literal::String(String::from("baz"))))?
            .try_build()?;

        let task = task::Builder::default()
            .name(name)?
            .command(command)?
            .push_private_declarations(PrivateDeclarations::from(NonEmpty::new(one.clone())))
            .push_private_declarations(PrivateDeclarations::from(NonEmpty::new(two.clone())))
            .try_build()
            .unwrap();

        let mut private_declarations = task.private_declarations().unwrap().inner().into_iter();

        assert_eq!(private_declarations.next(), Some(&one));
        assert_eq!(private_declarations.next(), Some(&two));
        assert_eq!(private_declarations.next(), None);

        Ok(())
    }
}
