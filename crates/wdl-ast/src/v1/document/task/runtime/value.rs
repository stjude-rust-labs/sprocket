//! A runtime value.

use grammar::v1::Rule;
use pest::iterators::Pair;
use wdl_grammar as grammar;
use wdl_macros::check_node;
use wdl_macros::unwrap_one;

use crate::v1::document::expression;
use crate::v1::document::Expression;

/// An error related to a [`Value`].
#[derive(Debug)]
pub enum Error {
    /// An expression error.
    Expression(expression::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Expression(err) => write!(f, "expression error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A runtime value.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Value(Expression);

impl Value {
    /// Gets the inner [`Expression`] of the [`Value`] by reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::task::runtime::Value;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let expr = Expression::Literal(Literal::Integer(4));
    /// let cpu = Value::try_from(expr.clone())?;
    ///
    /// assert_eq!(cpu.inner(), &expr);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn inner(&self) -> &Expression {
        &self.0
    }

    /// Consumes `self` to return the inner [`Expression`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::task::runtime::Value;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let expr = Expression::Literal(Literal::Integer(4));
    /// let cpu = Value::try_from(expr.clone())?;
    ///
    /// assert_eq!(cpu.into_inner(), expr);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn into_inner(self) -> Expression {
        self.0
    }
}

impl TryFrom<Expression> for Value {
    type Error = Error;

    fn try_from(expression: Expression) -> Result<Self, Self::Error> {
        Ok(Value(expression))
    }
}

impl TryFrom<Pair<'_, Rule>> for Value {
    type Error = Error;

    fn try_from(node: Pair<'_, Rule>) -> Result<Self, Self::Error> {
        check_node!(node, task_runtime_mapping_value);

        let expression_node = unwrap_one!(node, task_runtime_mapping_value);
        let expression = Expression::try_from(expression_node).map_err(Error::Expression)?;

        Self::try_from(expression)
    }
}

#[cfg(test)]
mod tests {
    use ordered_float::OrderedFloat;

    use super::*;
    use crate::v1::document::expression::Literal;
    use crate::v1::document::expression::UnarySigned;

    #[test]
    fn it_correctly_parses_integers() {
        let value = wdl_macros::test::valid_node!("1", task_runtime_mapping_value, Value);
        assert_eq!(value.into_inner(), Expression::Literal(Literal::Integer(1)));

        let value = wdl_macros::test::valid_node!("+1", task_runtime_mapping_value, Value);
        assert_eq!(
            value.into_inner(),
            Expression::UnarySigned(UnarySigned::Positive(Box::new(Expression::Literal(
                Literal::Integer(1)
            ))))
        );

        let value = wdl_macros::test::valid_node!("-1", task_runtime_mapping_value, Value);
        assert_eq!(
            value.into_inner(),
            Expression::UnarySigned(UnarySigned::Negative(Box::new(Expression::Literal(
                Literal::Integer(1)
            ))))
        );

        let value = wdl_macros::test::valid_node!("-+--1", task_runtime_mapping_value, Value);
        assert_eq!(
            value.into_inner(),
            Expression::UnarySigned(UnarySigned::Negative(Box::new(Expression::UnarySigned(
                UnarySigned::Positive(Box::new(Expression::UnarySigned(UnarySigned::Negative(
                    Box::new(Expression::UnarySigned(UnarySigned::Negative(Box::new(
                        Expression::Literal(Literal::Integer(1))
                    ))))
                ))))
            ))))
        )
    }

    #[test]
    fn it_correctly_parses_floats() {
        let value = wdl_macros::test::valid_node!("1.0", task_runtime_mapping_value, Value);
        assert_eq!(
            value.into_inner(),
            Expression::Literal(Literal::Float(OrderedFloat(1.0)))
        );

        let value = wdl_macros::test::valid_node!("+1.0", task_runtime_mapping_value, Value);
        assert_eq!(
            value.into_inner(),
            Expression::UnarySigned(UnarySigned::Positive(Box::new(Expression::Literal(
                Literal::Float(OrderedFloat(1.0))
            ))))
        );

        let value = wdl_macros::test::valid_node!("-1.0", task_runtime_mapping_value, Value);
        assert_eq!(
            value.into_inner(),
            Expression::UnarySigned(UnarySigned::Negative(Box::new(Expression::Literal(
                Literal::Float(OrderedFloat(1.0))
            ))))
        );

        let value = wdl_macros::test::valid_node!("-+--1.5", task_runtime_mapping_value, Value);
        assert_eq!(
            value.into_inner(),
            Expression::UnarySigned(UnarySigned::Negative(Box::new(Expression::UnarySigned(
                UnarySigned::Positive(Box::new(Expression::UnarySigned(UnarySigned::Negative(
                    Box::new(Expression::UnarySigned(UnarySigned::Negative(Box::new(
                        Expression::Literal(Literal::Float(OrderedFloat(1.5)))
                    ))))
                ))))
            ))))
        )
    }
}
