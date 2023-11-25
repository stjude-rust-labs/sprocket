//! A pair.

use grammar::v1::Rule;
use wdl_grammar as grammar;
use wdl_macros::check_node;

use crate::v1::document::expression;
use crate::v1::document::Expression;

/// An error related to a [`Pair`].
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

/// A pair within an [`Expression`].
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Pair(Box<Expression>, Box<Expression>);

impl Pair {
    /// Creates a new [`Pair`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::expression::Pair;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let pair = Pair::new(
    ///     Expression::Literal(Literal::Boolean(true)),
    ///     Expression::Literal(Literal::Boolean(false)),
    /// );
    /// ```
    pub fn new(first: Expression, second: Expression) -> Self {
        Self(Box::new(first), Box::new(second))
    }
}

impl TryFrom<pest::iterators::Pair<'_, grammar::v1::Rule>> for Pair {
    type Error = Error;

    fn try_from(node: pest::iterators::Pair<'_, grammar::v1::Rule>) -> Result<Self> {
        check_node!(node, pair_literal);

        let expressions = node
            .into_inner()
            .filter(|node| matches!(node.as_rule(), Rule::expression))
            .collect::<Vec<_>>();

        if expressions.len() != 2 {
            unreachable!("incorrect number of expressions in pair");
        }

        let mut expressions = expressions.into_iter();

        // SAFETY: we just checked above that there are exactly two elements.
        // Thus, this will always unwrap.
        let first_node = expressions.next().unwrap();
        let first =
            Expression::try_from(first_node).map_err(|err| Error::Expression(Box::new(err)))?;

        let second_node = expressions.next().unwrap();
        let second =
            Expression::try_from(second_node).map_err(|err| Error::Expression(Box::new(err)))?;

        Ok(Pair(Box::new(first), Box::new(second)))
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
        let pair = valid_node!(r#"(true, false)"#, pair_literal, Pair);
        assert_eq!(
            pair.0,
            Box::new(Expression::Literal(Literal::Boolean(true)))
        );
        assert_eq!(
            pair.1,
            Box::new(Expression::Literal(Literal::Boolean(false)))
        );
    }

    create_invalid_node_test!(
        "version 1.1\n\ntask hello { command <<<>>> }",
        document,
        pair_literal,
        Pair,
        it_fails_to_parse_from_an_unsupported_node_type
    );
}
