//! An if statement.

use grammar::v1::Rule;
use pest::iterators::Pair;
use wdl_grammar as grammar;
use wdl_macros::check_node;

use crate::v1::document::expression;
use crate::v1::document::Expression;

/// An error related to an [`If`].
#[derive(Debug)]
pub enum Error {
    /// An expression error.
    Expression(Box<expression::Error>),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Expression(err) => write!(f, "expression error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
type Result<T> = std::result::Result<T, Error>;

/// An if statement within an [`Expression`].
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct If {
    /// The conditional clause of the if statement.
    condition: Box<Expression>,

    /// The then clause of the if statement.
    then: Box<Expression>,

    /// The else clause of the if statement.
    r#else: Box<Expression>,
}

impl If {
    /// Creates a new [`If`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::If;
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let r#if = If::new(
    ///     Box::new(Expression::Literal(Literal::Boolean(false))),
    ///     Box::new(Expression::Literal(Literal::String(String::from("foo")))),
    ///     Box::new(Expression::Literal(Literal::Boolean(true))),
    /// );
    ///
    /// assert!(matches!(
    ///     r#if.condition(),
    ///     Expression::Literal(Literal::Boolean(false))
    /// ));
    /// assert!(matches!(
    ///     r#if.then(),
    ///     Expression::Literal(Literal::String(_))
    /// ));
    /// assert!(matches!(
    ///     r#if.r#else(),
    ///     Expression::Literal(Literal::Boolean(true))
    /// ));
    /// ```
    pub fn new(condition: Box<Expression>, then: Box<Expression>, r#else: Box<Expression>) -> Self {
        Self {
            condition,
            then,
            r#else,
        }
    }

    /// Gets the conditional clause of the [`If`] as an [`Expression`] by
    /// reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::If;
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let r#if = If::new(
    ///     Box::new(Expression::Literal(Literal::Boolean(false))),
    ///     Box::new(Expression::Literal(Literal::String(String::from("foo")))),
    ///     Box::new(Expression::Literal(Literal::Boolean(true))),
    /// );
    ///
    /// assert!(matches!(
    ///     r#if.condition(),
    ///     Expression::Literal(Literal::Boolean(false))
    /// ));
    /// ```
    pub fn condition(&self) -> &Expression {
        &self.condition
    }

    /// Gets the then clause of the [`If`] as an [`Expression`] by reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::If;
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let r#if = If::new(
    ///     Box::new(Expression::Literal(Literal::Boolean(false))),
    ///     Box::new(Expression::Literal(Literal::String(String::from("foo")))),
    ///     Box::new(Expression::Literal(Literal::Boolean(true))),
    /// );
    ///
    /// assert!(matches!(
    ///     r#if.then(),
    ///     Expression::Literal(Literal::String(_))
    /// ));
    /// ```
    pub fn then(&self) -> &Expression {
        &self.then
    }

    /// Gets the else clause of the [`If`] as an [`Expression`] by reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::If;
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let r#if = If::new(
    ///     Box::new(Expression::Literal(Literal::Boolean(false))),
    ///     Box::new(Expression::Literal(Literal::String(String::from("foo")))),
    ///     Box::new(Expression::Literal(Literal::Boolean(true))),
    /// );
    /// assert!(matches!(
    ///     r#if.r#else(),
    ///     Expression::Literal(Literal::Boolean(true))
    /// ));
    /// ```
    pub fn r#else(&self) -> &Expression {
        &self.r#else
    }
}

impl TryFrom<Pair<'_, grammar::v1::Rule>> for If {
    type Error = Error;

    fn try_from(node: Pair<'_, grammar::v1::Rule>) -> Result<Self> {
        check_node!(node, r#if);

        let expressions = node
            .into_inner()
            .filter(|node| matches!(node.as_rule(), Rule::expression))
            .collect::<Vec<_>>();

        if expressions.len() != 3 {
            unreachable!("incorrect number of expressions in if statement");
        }

        let mut expressions = expressions.into_iter();

        // SAFETY: we just checked above that there are exactly three elements.
        // Thus, this will always unwrap.
        let condition_node = expressions.next().unwrap();
        let condition =
            Expression::try_from(condition_node).map_err(|err| Error::Expression(Box::new(err)))?;

        let then_node = expressions.next().unwrap();
        let then =
            Expression::try_from(then_node).map_err(|err| Error::Expression(Box::new(err)))?;

        let else_node = expressions.next().unwrap();
        let r#else =
            Expression::try_from(else_node).map_err(|err| Error::Expression(Box::new(err)))?;

        Ok(If {
            condition: Box::new(condition),
            then: Box::new(then),
            r#else: Box::new(r#else),
        })
    }
}

#[cfg(test)]
mod tests {
    use wdl_macros::test::create_invalid_node_test;
    use wdl_macros::test::valid_node;

    use super::*;
    use crate::v1::document::expression::Literal;

    #[test]
    fn it_parses_from_a_supported_node_type() {
        let r#if = valid_node!(r#"if true then "foo" else false"#, r#if, If);
        assert!(matches!(
            r#if.condition(),
            Expression::Literal(Literal::Boolean(true))
        ));
        assert!(matches!(
            r#if.then(),
            Expression::Literal(Literal::String(_))
        ));
        assert!(matches!(
            r#if.r#else(),
            Expression::Literal(Literal::Boolean(false))
        ));
    }

    create_invalid_node_test!(
        "version 1.1\n\ntask hello { command <<<>>> }",
        document,
        r#if,
        If,
        it_fails_to_parse_from_an_unsupported_node_type
    );
}
