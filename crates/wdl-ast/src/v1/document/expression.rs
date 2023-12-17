//! Expressions.

use std::num::ParseFloatError;
use std::num::ParseIntError;

use grammar::v1::Rule;
use lazy_static::lazy_static;
use ordered_float::OrderedFloat;
use pest::pratt_parser::Assoc;
use pest::pratt_parser::Op;
use pest::pratt_parser::PrattParser;
use wdl_grammar as grammar;
use wdl_macros::check_node;
use wdl_macros::extract_one;

use crate::v1::document::identifier::singular;
use crate::v1::document::identifier::singular::Identifier;

mod array;
mod r#if;
mod literal;
mod map;
mod object;
mod pair;
mod r#struct;
mod unary_signed;

pub use array::Array;
pub use r#if::If;
pub use literal::Literal;
pub use map::Map;
pub use object::Object;
pub use pair::Pair;
pub use r#struct::Struct;
pub use unary_signed::UnarySigned;

lazy_static! {
    static ref PRATT_PARSER: PrattParser<Rule> = PrattParser::new()
        // [#1] Logical OR
        .op(Op::infix(Rule::or, Assoc::Left))
        // [#2] Logical AND
        .op(Op::infix(Rule::and, Assoc::Left))
        // [#3] Equality | Inequality
        .op(Op::infix(Rule::eq, Assoc::Left) | Op::infix(Rule::neq, Assoc::Left))
        // [#4] Less Than | Less Than Or Equal | Greater Than | Greater Than Or Equal
        .op(Op::infix(Rule::lt, Assoc::Left) | Op::infix(Rule::lte, Assoc::Left) | Op::infix(Rule::gt, Assoc::Left) | Op::infix(Rule::gte, Assoc::Left))
        // [#5] Addition | Subtraction
        .op(Op::infix(Rule::add, Assoc::Left) | Op::infix(Rule::sub, Assoc::Left))
        // [#6] Multiplication | Division | Remainder
        .op(Op::infix(Rule::mul, Assoc::Left) | Op::infix(Rule::div, Assoc::Left) | Op::infix(Rule::remainder, Assoc::Left))
        // [#7] Logical NOT | Unary signed positive | Unary signed negative.
        .op(Op::prefix(Rule::negation) | Op::prefix(Rule::unary_signed_positive) | Op::prefix(Rule::unary_signed_negative))
        // [#8] Function call.
        .op(Op::postfix(Rule::call))
        // [#9] Index.
        .op(Op::postfix(Rule::index))
        // [#10] Member access.
        .op(Op::postfix(Rule::member));
}

/// An error related to an [`Expression`].
#[derive(Debug)]
pub enum Error {
    /// An array error.
    Array(array::Error),

    /// An identifier error.
    Identifier(singular::Error),

    /// An if error.
    If(r#if::Error),

    /// A map error.
    Map(map::Error),

    /// An object error.
    Object(object::Error),

    /// A pair error.
    Pair(pair::Error),

    /// A [`ParseIntError`].
    ParseInt(ParseIntError),

    /// A [`ParseFloatError`].
    ParseFloat(ParseFloatError),

    /// A struct error.
    Struct(r#struct::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Array(err) => write!(f, "array error: {err}"),
            Error::Identifier(err) => write!(f, "identifier error: {err}"),
            Error::If(err) => write!(f, "if error: {err}"),
            Error::Map(err) => write!(f, "map error: {err}"),
            Error::Object(err) => write!(f, "object error: {err}"),
            Error::Pair(err) => write!(f, "pair error: {err}"),
            Error::ParseInt(err) => write!(f, "parse int error: {err}"),
            Error::ParseFloat(err) => write!(f, "parse float error: {err}"),
            Error::Struct(err) => write!(f, "struct error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
type Result<T> = std::result::Result<T, Error>;

/// An expression.
#[derive(Clone, Debug, Hash, Eq, Ord, PartialEq, PartialOrd)]
pub enum Expression {
    /// Addition.
    Add(Box<Expression>, Box<Expression>),

    /// Logical AND.
    And(Box<Expression>, Box<Expression>),

    /// An array literal.
    Array(Array),

    /// Function call.
    Call(Box<Expression>),

    /// Division.
    Divide(Box<Expression>, Box<Expression>),

    /// Equal.
    Equal(Box<Expression>, Box<Expression>),

    /// A group.
    Group(Box<Expression>),

    /// Greater than.
    GreaterThan(Box<Expression>, Box<Expression>),

    /// Greater than or equal.
    GreaterThanOrEqual(Box<Expression>, Box<Expression>),

    /// An if statement.
    If(If),

    /// Index access.
    Index(Box<Expression>),

    /// A literal value.
    Literal(Literal),

    /// Less than.
    LessThan(Box<Expression>, Box<Expression>),

    /// Less than or equal.
    LessThanOrEqual(Box<Expression>, Box<Expression>),

    /// A map literal.
    Map(Map),

    /// Member access.
    Member(Box<Expression>),

    /// Multiplication.
    Multiply(Box<Expression>, Box<Expression>),

    /// Negation.
    Negation(Box<Expression>),

    /// Not equal.
    NotEqual(Box<Expression>, Box<Expression>),

    /// An object literal.
    Object(Object),

    /// Logical OR.
    Or(Box<Expression>, Box<Expression>),

    /// A pair literal.
    Pair(Pair),

    /// Remainder.
    Remainder(Box<Expression>, Box<Expression>),

    /// A struct literal.
    Struct(Struct),

    /// Subtraction.
    Subtract(Box<Expression>, Box<Expression>),

    /// Unary signed.
    UnarySigned(UnarySigned),
}

/// Parses an expression using a [`PrattParser`].
fn parse<'a, P: Iterator<Item = pest::iterators::Pair<'a, grammar::v1::Rule>>>(
    pairs: P,
) -> Result<Expression> {
    let pairs = pairs.filter(|node| {
        !matches!(node.as_rule(), wdl_grammar::v1::Rule::WHITESPACE)
            && !matches!(node.as_rule(), wdl_grammar::v1::Rule::WHITESPACE)
    });

    PRATT_PARSER
        .map_primary(|node| match node.as_rule() {
            Rule::group => Ok(Expression::Group(Box::new(parse(node.into_inner())?))),
            Rule::expression => parse(node.into_inner()),
            Rule::r#if => {
                let r#if = If::try_from(node).map_err(Error::If)?;
                Ok(Expression::If(r#if))
            }
            Rule::object_literal => {
                let object = Object::try_from(node).map_err(Error::Object)?;
                Ok(Expression::Object(object))
            }
            Rule::struct_literal => {
                let r#struct = Struct::try_from(node).map_err(Error::Struct)?;
                Ok(Expression::Struct(r#struct))
            }
            Rule::map_literal => {
                let map = Map::try_from(node).map_err(Error::Map)?;
                Ok(Expression::Map(map))
            }
            Rule::array_literal => {
                let array = Array::try_from(node).map_err(Error::Array)?;
                Ok(Expression::Array(array))
            }
            Rule::pair_literal => {
                let pair = Pair::try_from(node).map_err(Error::Pair)?;
                Ok(Expression::Pair(pair))
            }
            Rule::boolean => match node.as_str() {
                "true" => Ok(Expression::Literal(Literal::Boolean(true))),
                "false" => Ok(Expression::Literal(Literal::Boolean(false))),
                value => {
                    unreachable!("unknown boolean literal value: {}", value)
                }
            },
            Rule::integer => Ok(Expression::Literal(Literal::Integer(
                node.as_str().parse::<i64>().map_err(Error::ParseInt)?,
            ))),
            Rule::float => Ok(Expression::Literal(Literal::Float(
                node.as_str()
                    .parse::<OrderedFloat<f64>>()
                    .map_err(Error::ParseFloat)?,
            ))),
            Rule::string => {
                // TODO: parse strings with placeholders properly.
                let inner = extract_one!(node, string_inner, string)?;
                Ok(Expression::Literal(Literal::String(
                    inner.as_str().to_owned(),
                )))
            }
            Rule::none => Ok(Expression::Literal(Literal::None)),
            Rule::singular_identifier => {
                let identifier =
                    Identifier::try_from(node.as_str().to_owned()).map_err(Error::Identifier)?;
                Ok(Expression::Literal(Literal::Identifier(identifier)))
            }
            _ => unreachable!("unknown primary in expression: {:?}", node.as_rule()),
        })
        .map_infix(|lhs, node, rhs| match node.as_rule() {
            Rule::or => Ok(Expression::Or(Box::new(lhs?), Box::new(rhs?))),
            Rule::and => Ok(Expression::And(Box::new(lhs?), Box::new(rhs?))),
            Rule::add => Ok(Expression::Add(Box::new(lhs?), Box::new(rhs?))),
            Rule::sub => Ok(Expression::Subtract(Box::new(lhs?), Box::new(rhs?))),
            Rule::mul => Ok(Expression::Multiply(Box::new(lhs?), Box::new(rhs?))),
            Rule::div => Ok(Expression::Divide(Box::new(lhs?), Box::new(rhs?))),
            Rule::remainder => Ok(Expression::Remainder(Box::new(lhs?), Box::new(rhs?))),
            Rule::eq => Ok(Expression::Equal(Box::new(lhs?), Box::new(rhs?))),
            Rule::neq => Ok(Expression::NotEqual(Box::new(lhs?), Box::new(rhs?))),
            Rule::lt => Ok(Expression::LessThan(Box::new(lhs?), Box::new(rhs?))),
            Rule::lte => Ok(Expression::LessThanOrEqual(Box::new(lhs?), Box::new(rhs?))),
            Rule::gt => Ok(Expression::GreaterThan(Box::new(lhs?), Box::new(rhs?))),
            Rule::gte => Ok(Expression::GreaterThanOrEqual(
                Box::new(lhs?),
                Box::new(rhs?),
            )),
            _ => unreachable!(
                "unknown infix operation in expression: {:?}",
                node.as_rule()
            ),
        })
        .map_prefix(|node, rhs| match node.as_rule() {
            Rule::negation => Ok(Expression::Negation(Box::new(rhs?))),
            Rule::unary_signed_positive => Ok(Expression::UnarySigned(UnarySigned::Positive(
                Box::new(rhs?),
            ))),
            Rule::unary_signed_negative => Ok(Expression::UnarySigned(UnarySigned::Negative(
                Box::new(rhs?),
            ))),
            _ => unreachable!(
                "unknown prefix operation in expression: {:?}",
                node.as_rule()
            ),
        })
        .map_postfix(|lhs, node| match node.as_rule() {
            Rule::member => Ok(Expression::Member(Box::new(lhs?))),
            Rule::index => Ok(Expression::Index(Box::new(lhs?))),
            Rule::call => Ok(Expression::Call(Box::new(lhs?))),
            _ => unreachable!(
                "unknown postfix operation in expression: {:?}",
                node.as_rule()
            ),
        })
        .parse(pairs)
}

impl TryFrom<pest::iterators::Pair<'_, grammar::v1::Rule>> for Expression {
    type Error = Error;

    fn try_from(node: pest::iterators::Pair<'_, grammar::v1::Rule>) -> Result<Self> {
        check_node!(node, expression);
        parse(node.into_inner())
    }
}

/// Ensures that an expression is a number. This includes floats and integers
/// that are wrapped in
pub fn ensure_number(expr: &Expression) -> Option<&Expression> {
    match expr {
        Expression::Literal(Literal::Float(_)) => Some(expr),
        Expression::Literal(Literal::Integer(_)) => Some(expr),
        Expression::UnarySigned(UnarySigned::Positive(inner)) => {
            if ensure_number(inner).is_some() {
                Some(expr)
            } else {
                None
            }
        }
        Expression::UnarySigned(UnarySigned::Negative(inner)) => {
            if ensure_number(inner).is_some() {
                Some(expr)
            } else {
                None
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use wdl_macros;

    use super::*;

    #[test]
    fn ensure_number_works_correctly() {
        let expr = wdl_macros::test::valid_node!("1", expression, Expression);
        assert_eq!(
            ensure_number(&expr),
            Some(&Expression::Literal(Literal::Integer(1)))
        );

        let expr = wdl_macros::test::valid_node!("-1", expression, Expression);
        assert_eq!(
            ensure_number(&expr),
            Some(&Expression::UnarySigned(UnarySigned::Negative(Box::new(
                Expression::Literal(Literal::Integer(1))
            ))))
        );

        let expr = wdl_macros::test::valid_node!("+-1", expression, Expression);
        assert_eq!(
            ensure_number(&expr),
            Some(&Expression::UnarySigned(UnarySigned::Positive(Box::new(
                Expression::UnarySigned(UnarySigned::Negative(Box::new(Expression::Literal(
                    Literal::Integer(1)
                ))))
            ))))
        );

        let expr = wdl_macros::test::valid_node!("-+-1", expression, Expression);
        assert_eq!(
            ensure_number(&expr),
            Some(&Expression::UnarySigned(UnarySigned::Negative(Box::new(
                Expression::UnarySigned(UnarySigned::Positive(Box::new(Expression::UnarySigned(
                    UnarySigned::Negative(Box::new(Expression::Literal(Literal::Integer(1))))
                ))))
            ))))
        );
        let expr = wdl_macros::test::valid_node!("1.0", expression, Expression);
        assert_eq!(
            ensure_number(&expr),
            Some(&Expression::Literal(Literal::Float(OrderedFloat(1.0))))
        );

        let expr = wdl_macros::test::valid_node!("-1.0", expression, Expression);
        assert_eq!(
            ensure_number(&expr),
            Some(&Expression::UnarySigned(UnarySigned::Negative(Box::new(
                Expression::Literal(Literal::Float(OrderedFloat(1.0)))
            ))))
        );

        let expr = wdl_macros::test::valid_node!("+-1.0", expression, Expression);
        assert_eq!(
            ensure_number(&expr),
            Some(&Expression::UnarySigned(UnarySigned::Positive(Box::new(
                Expression::UnarySigned(UnarySigned::Negative(Box::new(Expression::Literal(
                    Literal::Float(OrderedFloat(1.0))
                ))))
            ))))
        );

        let expr = wdl_macros::test::valid_node!("-+-1.0", expression, Expression);
        assert_eq!(
            ensure_number(&expr),
            Some(&Expression::UnarySigned(UnarySigned::Negative(Box::new(
                Expression::UnarySigned(UnarySigned::Positive(Box::new(Expression::UnarySigned(
                    UnarySigned::Negative(Box::new(Expression::Literal(Literal::Float(
                        OrderedFloat(1.0)
                    ))))
                ))))
            ))))
        );

        let expr = wdl_macros::test::valid_node!("-+-false", expression, Expression);
        assert_eq!(ensure_number(&expr), None);
    }

    #[test]
    fn it_correctly_parses_floats() {
        let value = wdl_macros::test::valid_node!("1.0", expression, Expression);
        assert_eq!(
            value,
            Expression::Literal(Literal::Float(OrderedFloat(1.0)))
        );

        let value = wdl_macros::test::valid_node!("1.0e0", expression, Expression);
        assert_eq!(
            value,
            Expression::Literal(Literal::Float(OrderedFloat(1.0)))
        );

        let value = wdl_macros::test::valid_node!("1.e0", expression, Expression);
        assert_eq!(
            value,
            Expression::Literal(Literal::Float(OrderedFloat(1.0)))
        );

        let value = wdl_macros::test::valid_node!("1e0", expression, Expression);
        assert_eq!(
            value,
            Expression::Literal(Literal::Float(OrderedFloat(1.0)))
        );

        let value = wdl_macros::test::valid_node!("1e+0", expression, Expression);
        assert_eq!(
            value,
            Expression::Literal(Literal::Float(OrderedFloat(1.0)))
        );

        // Positive signed.

        let value = wdl_macros::test::valid_node!("+1.0", expression, Expression);
        assert_eq!(
            value,
            Expression::UnarySigned(UnarySigned::Positive(Box::new(Expression::Literal(
                Literal::Float(OrderedFloat(1.0))
            ))))
        );

        let value = wdl_macros::test::valid_node!("+1.0e0", expression, Expression);
        assert_eq!(
            value,
            Expression::UnarySigned(UnarySigned::Positive(Box::new(Expression::Literal(
                Literal::Float(OrderedFloat(1.0))
            ))))
        );

        let value = wdl_macros::test::valid_node!("+1.e0", expression, Expression);
        assert_eq!(
            value,
            Expression::UnarySigned(UnarySigned::Positive(Box::new(Expression::Literal(
                Literal::Float(OrderedFloat(1.0))
            ))))
        );

        let value = wdl_macros::test::valid_node!("+1e0", expression, Expression);
        assert_eq!(
            value,
            Expression::UnarySigned(UnarySigned::Positive(Box::new(Expression::Literal(
                Literal::Float(OrderedFloat(1.0))
            ))))
        );

        let value = wdl_macros::test::valid_node!("+1e+0", expression, Expression);
        assert_eq!(
            value,
            Expression::UnarySigned(UnarySigned::Positive(Box::new(Expression::Literal(
                Literal::Float(OrderedFloat(1.0))
            ))))
        );

        // Negative signed.

        let value = wdl_macros::test::valid_node!("-1.0", expression, Expression);
        assert_eq!(
            value,
            Expression::UnarySigned(UnarySigned::Negative(Box::new(Expression::Literal(
                Literal::Float(OrderedFloat(1.0))
            ))))
        );

        let value = wdl_macros::test::valid_node!("-1.0e0", expression, Expression);
        assert_eq!(
            value,
            Expression::UnarySigned(UnarySigned::Negative(Box::new(Expression::Literal(
                Literal::Float(OrderedFloat(1.0))
            ))))
        );

        let value = wdl_macros::test::valid_node!("-1.e0", expression, Expression);
        assert_eq!(
            value,
            Expression::UnarySigned(UnarySigned::Negative(Box::new(Expression::Literal(
                Literal::Float(OrderedFloat(1.0))
            ))))
        );

        let value = wdl_macros::test::valid_node!("-1e0", expression, Expression);
        assert_eq!(
            value,
            Expression::UnarySigned(UnarySigned::Negative(Box::new(Expression::Literal(
                Literal::Float(OrderedFloat(1.0))
            ))))
        );

        let value = wdl_macros::test::valid_node!("-1e+0", expression, Expression);
        assert_eq!(
            value,
            Expression::UnarySigned(UnarySigned::Negative(Box::new(Expression::Literal(
                Literal::Float(OrderedFloat(1.0))
            ))))
        );
    }
}
