//! Scatters.

use grammar::v1::Rule;
use nonempty::NonEmpty;
use pest::iterators::Pair;
use wdl_grammar as grammar;
use wdl_macros::check_node;
use wdl_macros::dive_one;
use wdl_macros::unwrap_one;

use crate::v1::document::expression;
use crate::v1::document::identifier::singular;
use crate::v1::document::identifier::singular::Identifier;
use crate::v1::document::workflow::execution::statement;
use crate::v1::document::workflow::execution::Statement;
use crate::v1::document::Expression;

mod builder;

pub use builder::Builder;

/// An error related to [`Scatter`].
#[derive(Debug)]
pub enum Error {
    /// A builder error.
    Builder(builder::Error),

    /// An expression error.
    Expression(expression::Error),

    /// An identifier error.
    Identifier(singular::Error),

    /// A workflow execution statement error.
    Statement(statement::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Builder(err) => write!(f, "builder error: {err}"),
            Error::Expression(err) => write!(f, "expression error: {err}"),
            Error::Identifier(err) => write!(f, "identifier error: {err}"),
            Error::Statement(err) => {
                write!(f, "workflow execution statement error: {err}")
            }
        }
    }
}

impl std::error::Error for Error {}

/// A scatter statement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Scatter {
    /// The entities being scattered over.
    iterable: Expression,

    /// The workflow execution statements for each entity.
    statements: Option<NonEmpty<Box<Statement>>>,

    /// The variable name for each entity.
    variable: Identifier,
}

impl Scatter {
    /// Gets the iterables from the [`Scatter`] by reference.
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
    pub fn iterable(&self) -> &Expression {
        &self.iterable
    }

    /// Gets the [workflow execution statement(s)](Statement) from this
    /// [`Scatter`] by reference (if they exist).
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
    pub fn statements(&self) -> Option<&NonEmpty<Box<Statement>>> {
        self.statements.as_ref()
    }

    /// Gets the variable for this [`Scatter`] by reference.
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
    pub fn variable(&self) -> &Identifier {
        &self.variable
    }
}

impl TryFrom<Pair<'_, grammar::v1::Rule>> for Scatter {
    type Error = Error;

    fn try_from(node: Pair<'_, grammar::v1::Rule>) -> Result<Self, Self::Error> {
        check_node!(node, workflow_scatter);
        let mut builder = Builder::default();

        for node in node.into_inner() {
            match node.as_rule() {
                Rule::workflow_scatter_iteration_statement => {
                    // TODO: a clone is required here because Pest's `FlatPairs`
                    // type does not support creating an iterator without taking
                    // ownership (at the time of writing). This can be made
                    // better with a PR to Pest.
                    let variable_node = dive_one!(
                        node.clone(),
                        workflow_scatter_iteration_statement_variable,
                        workflow_scatter_iteration_statement
                    );
                    let variable_node =
                        unwrap_one!(variable_node, workflow_scatter_iteration_statement_variable);
                    let variable =
                        singular::Identifier::try_from(variable_node).map_err(Error::Identifier)?;

                    let iterable_node = dive_one!(
                        node.clone(),
                        workflow_scatter_iteration_statement_iterable,
                        workflow_scatter_iteration_statement
                    );
                    let iterable_node =
                        unwrap_one!(iterable_node, workflow_scatter_iteration_statement_iterable);
                    let iterable =
                        Expression::try_from(iterable_node).map_err(Error::Expression)?;

                    builder = builder.variable(variable).map_err(Error::Builder)?;
                    builder = builder.iterable(iterable).map_err(Error::Builder)?;
                }
                Rule::workflow_execution_statement => {
                    let statement = Statement::try_from(node).map_err(Error::Statement)?;
                    builder = builder.push_workflow_execution_statement(statement);
                }
                Rule::WHITESPACE => {}
                Rule::COMMENT => {}
                rule => unreachable!("scatter should not contain {:?}", rule),
            }
        }

        builder.try_build().map_err(Error::Builder)
    }
}

#[cfg(test)]
mod tests {
    use wdl_macros::test::create_invalid_node_test;
    use wdl_macros::test::valid_node;

    use super::*;
    use crate::v1::document::expression::Literal;
    use crate::v1::document::workflow::execution::statement::call::body::Value;

    #[test]
    fn it_parses_from_a_supported_node_type() {
        let scatter = valid_node!(
            r#"scatter (file in files) {
                call read {
                    input: file, foo=bar
                }

                call external.write {
                    input: file, baz=false
                }
            }"#,
            workflow_scatter,
            Scatter
        );

        assert_eq!(scatter.variable().as_str(), "file");
        assert_eq!(
            scatter.iterable(),
            &Expression::Literal(Literal::Identifier(
                singular::Identifier::try_from("files").unwrap()
            ))
        );

        let statements = scatter.statements().unwrap();
        assert_eq!(statements.len(), 2);

        let mut statements = statements.into_iter();

        let first_call = match statements.next().unwrap().as_ref() {
            Statement::Call(call) => call,
            _ => unreachable!(),
        };
        assert_eq!(first_call.name().to_string(), "read");
        assert_eq!(
            first_call.body().unwrap().get("file"),
            Some(&Value::ImplicitBinding)
        );
        assert_eq!(
            first_call.body().unwrap().get("foo"),
            Some(&Value::Expression(Expression::Literal(
                Literal::Identifier(singular::Identifier::try_from("bar").unwrap())
            )))
        );
        assert_eq!(first_call.body().unwrap().get("does_not_exist"), None);

        let second_call = match statements.next().unwrap().as_ref() {
            Statement::Call(call) => call,
            _ => unreachable!(),
        };
        assert_eq!(second_call.name().to_string(), "external.write");
        assert_eq!(
            second_call.body().unwrap().get("file"),
            Some(&Value::ImplicitBinding)
        );
        assert_eq!(
            second_call.body().unwrap().get("baz"),
            Some(&Value::Expression(Expression::Literal(Literal::Boolean(
                false
            ))))
        );
        assert_eq!(second_call.body().unwrap().get("does_not_exist"), None);
    }

    create_invalid_node_test!(
        "version 1.1\n\ntask hello { command <<<>>> }",
        document,
        workflow_scatter,
        Scatter,
        it_fails_to_parse_from_an_unsupported_node_type
    );
}
