//! Conditionals.

use grammar::v1::Rule;
use nonempty::NonEmpty;
use pest::iterators::Pair;
use wdl_grammar as grammar;
use wdl_macros::check_node;
use wdl_macros::unwrap_one;

use crate::v1::document::expression;
use crate::v1::document::workflow::execution::statement;
use crate::v1::document::workflow::execution::Statement;
use crate::v1::document::Expression;

mod builder;

pub use builder::Builder;

/// An error rleated to a [`Conditional`].
#[derive(Debug)]
pub enum Error {
    /// A builder error.
    Builder(builder::Error),

    /// An expression error.
    Expression(expression::Error),

    /// A workflow execution statement error.
    Statement(statement::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Builder(err) => write!(f, "builder error: {err}"),
            Error::Expression(err) => write!(f, "expression error: {err}"),
            Error::Statement(err) => {
                write!(f, "workflow execution statement error: {err}")
            }
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
type Result<T> = std::result::Result<T, Error>;

/// A conditional statement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Conditional {
    /// The condition clause.
    condition: Expression,

    /// The workflow execution statements.
    statements: Option<NonEmpty<Box<Statement>>>,
}

impl Conditional {
    /// Gets the condition clause from this [`Conditional`] by reference.
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
    pub fn condition(&self) -> &Expression {
        &self.condition
    }

    /// Gets the [workflow execution statement(s)](Statement) from this
    /// [Conditional] by reference (if they exist).
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
    pub fn statements(&self) -> Option<&NonEmpty<Box<Statement>>> {
        self.statements.as_ref()
    }
}

impl TryFrom<Pair<'_, grammar::v1::Rule>> for Conditional {
    type Error = Error;

    fn try_from(node: Pair<'_, grammar::v1::Rule>) -> Result<Self> {
        check_node!(node, workflow_conditional);
        let mut builder = Builder::default();

        for node in node.into_inner() {
            match node.as_rule() {
                Rule::workflow_conditional_condition => {
                    let condition_node = unwrap_one!(node, workflow_conditional_condition);
                    let condition =
                        Expression::try_from(condition_node).map_err(Error::Expression)?;
                    builder = builder.condition(condition).map_err(Error::Builder)?;
                }
                Rule::workflow_execution_statement => {
                    let statement = Statement::try_from(node).map_err(Error::Statement)?;
                    builder = builder.push_workflow_execution_statement(statement);
                }
                Rule::WHITESPACE => {}
                Rule::COMMENT => {}
                rule => {
                    unreachable!("workflow call should not contain {:?}", rule)
                }
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
    use crate::v1::document::Expression;

    #[test]
    fn it_parses_from_a_supported_node_type() {
        let conditional = valid_node!(
            r#"if (true) {
            call foo
        }"#,
            workflow_conditional,
            Conditional
        );

        assert_eq!(
            conditional.condition(),
            &Expression::Literal(Literal::Boolean(true))
        );

        let statements = conditional.statements().unwrap();
        assert_eq!(statements.len(), 1);

        let first_call = match statements.iter().next().unwrap().as_ref() {
            Statement::Call(call) => call,
            _ => unreachable!(),
        };
        assert_eq!(first_call.name().to_string(), "foo");
        assert_eq!(first_call.body(), None);
    }

    create_invalid_node_test!(
        "version 1.1\n\ntask hello { command <<<>>> }",
        document,
        workflow_conditional,
        Conditional,
        it_fails_to_parse_from_an_unsupported_node_type
    );
}
