//! V1 AST representation for expressions.

use rowan::NodeOrToken;
use wdl_grammar::lexer::v1::EscapeToken;
use wdl_grammar::lexer::v1::Logos;

use super::Minus;
use crate::AstNode;
use crate::AstToken;
use crate::Ident;
use crate::SyntaxKind;
use crate::SyntaxNode;
use crate::SyntaxToken;
use crate::TreeNode;
use crate::TreeToken;

/// Represents an expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr<N: TreeNode = SyntaxNode> {
    /// The expression is a literal.
    Literal(LiteralExpr<N>),
    /// The expression is a name reference.
    NameRef(NameRefExpr<N>),
    /// The expression is a parenthesized expression.
    Parenthesized(ParenthesizedExpr<N>),
    /// The expression is an `if` expression.
    If(IfExpr<N>),
    /// The expression is a "logical not" expression.
    LogicalNot(LogicalNotExpr<N>),
    /// The expression is a negation expression.
    Negation(NegationExpr<N>),
    /// The expression is a "logical or" expression.
    LogicalOr(LogicalOrExpr<N>),
    /// The expression is a "logical and" expression.
    LogicalAnd(LogicalAndExpr<N>),
    /// The expression is an equality expression.
    Equality(EqualityExpr<N>),
    /// The expression is an inequality expression.
    Inequality(InequalityExpr<N>),
    /// The expression is a "less than" expression.
    Less(LessExpr<N>),
    /// The expression is a "less than or equal to" expression.
    LessEqual(LessEqualExpr<N>),
    /// The expression is a "greater" expression.
    Greater(GreaterExpr<N>),
    /// The expression is a "greater than or equal to" expression.
    GreaterEqual(GreaterEqualExpr<N>),
    /// The expression is an addition expression.
    Addition(AdditionExpr<N>),
    /// The expression is a subtraction expression.
    Subtraction(SubtractionExpr<N>),
    /// The expression is a multiplication expression.
    Multiplication(MultiplicationExpr<N>),
    /// The expression is a division expression.
    Division(DivisionExpr<N>),
    /// The expression is a modulo expression.
    Modulo(ModuloExpr<N>),
    /// The expression is an exponentiation expression.
    Exponentiation(ExponentiationExpr<N>),
    /// The expression is a call expression.
    Call(CallExpr<N>),
    /// The expression is an index expression.
    Index(IndexExpr<N>),
    /// The expression is a member access expression.
    Access(AccessExpr<N>),
}

impl<N: TreeNode> Expr<N> {
    /// Attempts to get a reference to the inner [`LiteralExpr`].
    ///
    /// * If `self` is a [`Expr::Literal`], then a reference to the inner
    ///   [`LiteralExpr`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_literal(&self) -> Option<&LiteralExpr<N>> {
        match self {
            Self::Literal(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`LiteralExpr`].
    ///
    /// * If `self` is a [`Expr::Literal`], then the inner [`LiteralExpr`] is
    ///   returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_literal(self) -> Option<LiteralExpr<N>> {
        match self {
            Self::Literal(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a literal expression.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a literal expression.
    pub fn unwrap_literal(self) -> LiteralExpr<N> {
        match self {
            Self::Literal(e) => e,
            _ => panic!("not a literal expression"),
        }
    }

    /// Attempts to get a reference to the inner [`NameRefExpr`].
    ///
    /// * If `self` is a [`Expr::NameRef`], then a reference to the inner
    ///   [`NameRefExpr`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_name_ref(&self) -> Option<&NameRefExpr<N>> {
        match self {
            Self::NameRef(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`NameRefExpr`].
    ///
    /// * If `self` is a [`Expr::NameRef`], then the inner [`NameRefExpr`] is
    ///   returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_name_ref(self) -> Option<NameRefExpr<N>> {
        match self {
            Self::NameRef(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a name reference.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a name reference.
    pub fn unwrap_name_ref(self) -> NameRefExpr<N> {
        match self {
            Self::NameRef(e) => e,
            _ => panic!("not a name reference"),
        }
    }

    /// Attempts to get a reference to the inner [`ParenthesizedExpr`].
    ///
    /// * If `self` is a [`Expr::Parenthesized`], then a reference to the inner
    ///   [`ParenthesizedExpr`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_parenthesized(&self) -> Option<&ParenthesizedExpr<N>> {
        match self {
            Self::Parenthesized(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`ParenthesizedExpr`].
    ///
    /// * If `self` is a [`Expr::Parenthesized`], then the inner
    ///   [`ParenthesizedExpr`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_parenthesized(self) -> Option<ParenthesizedExpr<N>> {
        match self {
            Self::Parenthesized(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a parenthesized expression.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a parenthesized expression.
    pub fn unwrap_parenthesized(self) -> ParenthesizedExpr<N> {
        match self {
            Self::Parenthesized(e) => e,
            _ => panic!("not a parenthesized expression"),
        }
    }

    /// Attempts to get a reference to the inner [`IfExpr`].
    ///
    /// * If `self` is a [`Expr::If`], then a reference to the inner [`IfExpr`]
    ///   is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_if(&self) -> Option<&IfExpr<N>> {
        match self {
            Self::If(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`IfExpr`].
    ///
    /// * If `self` is a [`Expr::If`], then the inner [`IfExpr`] is returned
    ///   wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_if(self) -> Option<IfExpr<N>> {
        match self {
            Self::If(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into an `if` expression.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not an `if` expression.
    pub fn unwrap_if(self) -> IfExpr<N> {
        match self {
            Self::If(e) => e,
            _ => panic!("not an `if` expression"),
        }
    }

    /// Attempts to get a reference to the inner [`LogicalNotExpr`].
    ///
    /// * If `self` is a [`Expr::LogicalNot`], then a reference to the inner
    ///   [`LogicalNotExpr`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_logical_not(&self) -> Option<&LogicalNotExpr<N>> {
        match self {
            Self::LogicalNot(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`LogicalNotExpr`].
    ///
    /// * If `self` is a [`Expr::LogicalNot`], then the inner [`LogicalNotExpr`]
    ///   is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_logical_not(self) -> Option<LogicalNotExpr<N>> {
        match self {
            Self::LogicalNot(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a logical `not` expression.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a logical `not` expression.
    pub fn unwrap_logical_not(self) -> LogicalNotExpr<N> {
        match self {
            Self::LogicalNot(e) => e,
            _ => panic!("not a logical `not` expression"),
        }
    }

    /// Attempts to get a reference to the inner [`NegationExpr`].
    ///
    /// * If `self` is a [`Expr::Negation`], then a reference to the inner
    ///   [`NegationExpr`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_negation(&self) -> Option<&NegationExpr<N>> {
        match self {
            Self::Negation(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`NegationExpr`].
    ///
    /// * If `self` is a [`Expr::Negation`], then the inner [`NegationExpr`] is
    ///   returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_negation(self) -> Option<NegationExpr<N>> {
        match self {
            Self::Negation(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a negation expression.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a negation expression.
    pub fn unwrap_negation(self) -> NegationExpr<N> {
        match self {
            Self::Negation(e) => e,
            _ => panic!("not a negation expression"),
        }
    }

    /// Attempts to get a reference to the inner [`LogicalOrExpr`].
    ///
    /// * If `self` is a [`Expr::LogicalOr`], then a reference to the inner
    ///   [`LogicalOrExpr`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_logical_or(&self) -> Option<&LogicalOrExpr<N>> {
        match self {
            Self::LogicalOr(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`LogicalOrExpr`].
    ///
    /// * If `self` is a [`Expr::LogicalOr`], then the inner [`LogicalOrExpr`]
    ///   is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_logical_or(self) -> Option<LogicalOrExpr<N>> {
        match self {
            Self::LogicalOr(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a logical `or` expression.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a logical `or` expression.
    pub fn unwrap_logical_or(self) -> LogicalOrExpr<N> {
        match self {
            Self::LogicalOr(e) => e,
            _ => panic!("not a logical `or` expression"),
        }
    }

    /// Attempts to get a reference to the inner [`LogicalAndExpr`].
    ///
    /// * If `self` is a [`Expr::LogicalAnd`], then a reference to the inner
    ///   [`LogicalAndExpr`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_logical_and(&self) -> Option<&LogicalAndExpr<N>> {
        match self {
            Self::LogicalAnd(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`LogicalAndExpr`].
    ///
    /// * If `self` is a [`Expr::LogicalAnd`], then the inner [`LogicalAndExpr`]
    ///   is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_logical_and(self) -> Option<LogicalAndExpr<N>> {
        match self {
            Self::LogicalAnd(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a logical `and` expression.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a logical `and` expression.
    pub fn unwrap_logical_and(self) -> LogicalAndExpr<N> {
        match self {
            Self::LogicalAnd(e) => e,
            _ => panic!("not a logical `and` expression"),
        }
    }

    /// Attempts to get a reference to the inner [`EqualityExpr`].
    ///
    /// * If `self` is a [`Expr::Equality`], then a reference to the inner
    ///   [`EqualityExpr`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_equality(&self) -> Option<&EqualityExpr<N>> {
        match self {
            Self::Equality(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`EqualityExpr`].
    ///
    /// * If `self` is a [`Expr::Equality`], then the inner [`EqualityExpr`] is
    ///   returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_equality(self) -> Option<EqualityExpr<N>> {
        match self {
            Self::Equality(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into an equality expression.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not an equality expression.
    pub fn unwrap_equality(self) -> EqualityExpr<N> {
        match self {
            Self::Equality(e) => e,
            _ => panic!("not an equality expression"),
        }
    }

    /// Attempts to get a reference to the inner [`InequalityExpr`].
    ///
    /// * If `self` is a [`Expr::Inequality`], then a reference to the inner
    ///   [`InequalityExpr`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_inequality(&self) -> Option<&InequalityExpr<N>> {
        match self {
            Self::Inequality(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`InequalityExpr`].
    ///
    /// * If `self` is a [`Expr::Inequality`], then the inner [`InequalityExpr`]
    ///   is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_inequality(self) -> Option<InequalityExpr<N>> {
        match self {
            Self::Inequality(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into an inequality expression.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not an inequality expression.
    pub fn unwrap_inequality(self) -> InequalityExpr<N> {
        match self {
            Self::Inequality(e) => e,
            _ => panic!("not an inequality expression"),
        }
    }

    /// Attempts to get a reference to the inner [`LessExpr`].
    ///
    /// * If `self` is a [`Expr::Less`], then a reference to the inner
    ///   [`LessExpr`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_less(&self) -> Option<&LessExpr<N>> {
        match self {
            Self::Less(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`LessExpr`].
    ///
    /// * If `self` is a [`Expr::Less`], then the inner [`LessExpr`] is returned
    ///   wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_less(self) -> Option<LessExpr<N>> {
        match self {
            Self::Less(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a "less than" expression.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a "less than" expression.
    pub fn unwrap_less(self) -> LessExpr<N> {
        match self {
            Self::Less(e) => e,
            _ => panic!("not a \"less than\" expression"),
        }
    }

    /// Attempts to get a reference to the inner [`LessEqualExpr`].
    ///
    /// * If `self` is a [`Expr::LessEqual`], then a reference to the inner
    ///   [`LessEqualExpr`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_less_equal(&self) -> Option<&LessEqualExpr<N>> {
        match self {
            Self::LessEqual(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`LessEqualExpr`].
    ///
    /// * If `self` is a [`Expr::LessEqual`], then the inner [`LessEqualExpr`]
    ///   is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_less_equal(self) -> Option<LessEqualExpr<N>> {
        match self {
            Self::LessEqual(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a "less than or equal to" expression.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a "less than or equal to" expression.
    pub fn unwrap_less_equal(self) -> LessEqualExpr<N> {
        match self {
            Self::LessEqual(e) => e,
            _ => panic!("not a \"less than or equal to\" expression"),
        }
    }

    /// Attempts to get a reference to the inner [`GreaterExpr`].
    ///
    /// * If `self` is a [`Expr::Greater`], then a reference to the inner
    ///   [`GreaterExpr`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_greater(&self) -> Option<&GreaterExpr<N>> {
        match self {
            Self::Greater(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`GreaterExpr`].
    ///
    /// * If `self` is a [`Expr::Greater`], then the inner [`GreaterExpr`] is
    ///   returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_greater(self) -> Option<GreaterExpr<N>> {
        match self {
            Self::Greater(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a "greater than" expression.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a "greater than" expression.
    pub fn unwrap_greater(self) -> GreaterExpr<N> {
        match self {
            Self::Greater(e) => e,
            _ => panic!("not a \"greater than\" expression"),
        }
    }

    /// Attempts to get a reference to the inner [`GreaterEqualExpr`].
    ///
    /// * If `self` is a [`Expr::GreaterEqual`], then a reference to the inner
    ///   [`GreaterEqualExpr`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_greater_equal(&self) -> Option<&GreaterEqualExpr<N>> {
        match self {
            Self::GreaterEqual(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`GreaterEqualExpr`].
    ///
    /// * If `self` is a [`Expr::GreaterEqual`], then the inner
    ///   [`GreaterEqualExpr`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_greater_equal(self) -> Option<GreaterEqualExpr<N>> {
        match self {
            Self::GreaterEqual(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a "greater than or equal to" expression.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a "greater than or equal to" expression.
    pub fn unwrap_greater_equal(self) -> GreaterEqualExpr<N> {
        match self {
            Self::GreaterEqual(e) => e,
            _ => panic!("not a \"greater than or equal to\" expression"),
        }
    }

    /// Attempts to get a reference to the inner [`AdditionExpr`].
    ///
    /// * If `self` is a [`Expr::Addition`], then a reference to the inner
    ///   [`AdditionExpr`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_addition(&self) -> Option<&AdditionExpr<N>> {
        match self {
            Self::Addition(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`AdditionExpr`].
    ///
    /// * If `self` is a [`Expr::Addition`], then the inner [`AdditionExpr`] is
    ///   returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_addition(self) -> Option<AdditionExpr<N>> {
        match self {
            Self::Addition(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into an addition expression.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not an addition expression.
    pub fn unwrap_addition(self) -> AdditionExpr<N> {
        match self {
            Self::Addition(e) => e,
            _ => panic!("not an addition expression"),
        }
    }

    /// Attempts to get a reference to the inner [`SubtractionExpr`].
    ///
    /// * If `self` is a [`Expr::Subtraction`], then a reference to the inner
    ///   [`SubtractionExpr`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_subtraction(&self) -> Option<&SubtractionExpr<N>> {
        match self {
            Self::Subtraction(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`SubtractionExpr`].
    ///
    /// * If `self` is a [`Expr::Subtraction`], then the inner
    ///   [`SubtractionExpr`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_subtraction(self) -> Option<SubtractionExpr<N>> {
        match self {
            Self::Subtraction(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a subtraction expression.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a subtraction expression.
    pub fn unwrap_subtraction(self) -> SubtractionExpr<N> {
        match self {
            Self::Subtraction(e) => e,
            _ => panic!("not a subtraction expression"),
        }
    }

    /// Attempts to get a reference to the inner [`MultiplicationExpr`].
    ///
    /// * If `self` is a [`Expr::Multiplication`], then a reference to the inner
    ///   [`MultiplicationExpr`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_multiplication(&self) -> Option<&MultiplicationExpr<N>> {
        match self {
            Self::Multiplication(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`MultiplicationExpr`].
    ///
    /// * If `self` is a [`Expr::Multiplication`], then the inner
    ///   [`MultiplicationExpr`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_multiplication(self) -> Option<MultiplicationExpr<N>> {
        match self {
            Self::Multiplication(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a multiplication expression.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a multiplication expression.
    pub fn unwrap_multiplication(self) -> MultiplicationExpr<N> {
        match self {
            Self::Multiplication(e) => e,
            _ => panic!("not a multiplication expression"),
        }
    }

    /// Attempts to get a reference to the inner [`DivisionExpr`].
    ///
    /// * If `self` is a [`Expr::Division`], then a reference to the inner
    ///   [`DivisionExpr`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_division(&self) -> Option<&DivisionExpr<N>> {
        match self {
            Self::Division(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`DivisionExpr`].
    ///
    /// * If `self` is a [`Expr::Division`], then the inner [`DivisionExpr`] is
    ///   returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_division(self) -> Option<DivisionExpr<N>> {
        match self {
            Self::Division(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a division expression.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a division expression.
    pub fn unwrap_division(self) -> DivisionExpr<N> {
        match self {
            Self::Division(e) => e,
            _ => panic!("not a division expression"),
        }
    }

    /// Attempts to get a reference to the inner [`ModuloExpr`].
    ///
    /// * If `self` is a [`Expr::Modulo`], then a reference to the inner
    ///   [`ModuloExpr`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_modulo(&self) -> Option<&ModuloExpr<N>> {
        match self {
            Self::Modulo(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`ModuloExpr`].
    ///
    /// * If `self` is a [`Expr::Modulo`], then the inner [`ModuloExpr`] is
    ///   returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_modulo(self) -> Option<ModuloExpr<N>> {
        match self {
            Self::Modulo(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a modulo expression.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a modulo expression.
    pub fn unwrap_modulo(self) -> ModuloExpr<N> {
        match self {
            Self::Modulo(e) => e,
            _ => panic!("not a modulo expression"),
        }
    }

    /// Attempts to get a reference to the inner [`ExponentiationExpr`].
    ///
    /// * If `self` is a [`Expr::Exponentiation`], then a reference to the inner
    ///   [`ExponentiationExpr`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_exponentiation(&self) -> Option<&ExponentiationExpr<N>> {
        match self {
            Self::Exponentiation(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`ExponentiationExpr`].
    ///
    /// * If `self` is a [`Expr::Exponentiation`], then the inner
    ///   [`ExponentiationExpr`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_exponentiation(self) -> Option<ExponentiationExpr<N>> {
        match self {
            Self::Exponentiation(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into an exponentiation expression.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not an exponentiation expression.
    pub fn unwrap_exponentiation(self) -> ExponentiationExpr<N> {
        match self {
            Self::Exponentiation(e) => e,
            _ => panic!("not an exponentiation expression"),
        }
    }

    /// Attempts to get a reference to the inner [`CallExpr`].
    ///
    /// * If `self` is a [`Expr::Call`], then a reference to the inner
    ///   [`CallExpr`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_call(&self) -> Option<&CallExpr<N>> {
        match self {
            Self::Call(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`CallExpr`].
    ///
    /// * If `self` is a [`Expr::Call`], then the inner [`CallExpr`] is returned
    ///   wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_call(self) -> Option<CallExpr<N>> {
        match self {
            Self::Call(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a call expression.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a call expression.
    pub fn unwrap_call(self) -> CallExpr<N> {
        match self {
            Self::Call(e) => e,
            _ => panic!("not a call expression"),
        }
    }

    /// Attempts to get a reference to the inner [`IndexExpr`].
    ///
    /// * If `self` is a [`Expr::Index`], then a reference to the inner
    ///   [`IndexExpr`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_index(&self) -> Option<&IndexExpr<N>> {
        match self {
            Self::Index(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`IndexExpr`].
    ///
    /// * If `self` is a [`Expr::Index`], then the inner [`IndexExpr`] is
    ///   returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_index(self) -> Option<IndexExpr<N>> {
        match self {
            Self::Index(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into an index expression.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not an index expression.
    pub fn unwrap_index(self) -> IndexExpr<N> {
        match self {
            Self::Index(e) => e,
            _ => panic!("not an index expression"),
        }
    }

    /// Attempts to get a reference to the inner [`AccessExpr`].
    ///
    /// * If `self` is a [`Expr::Access`], then a reference to the inner
    ///   [`AccessExpr`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_access(&self) -> Option<&AccessExpr<N>> {
        match self {
            Self::Access(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`AccessExpr`].
    ///
    /// * If `self` is a [`Expr::Access`], then the inner [`AccessExpr`] is
    ///   returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_access(self) -> Option<AccessExpr<N>> {
        match self {
            Self::Access(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into an access expression.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not an access expression.
    pub fn unwrap_access(self) -> AccessExpr<N> {
        match self {
            Self::Access(e) => e,
            _ => panic!("not an access expression"),
        }
    }

    /// Finds the first child that can be cast to an [`Expr`].
    pub fn child(node: &N) -> Option<Self> {
        node.children().find_map(Self::cast)
    }

    /// Finds all children that can be cast to an [`Expr`].
    pub fn children(node: &N) -> impl Iterator<Item = Self> + use<'_, N> {
        node.children().filter_map(Self::cast)
    }

    /// Determines if the expression is an empty array literal or any number of
    /// parenthesized expressions that terminate with an empty array literal.
    pub fn is_empty_array_literal(&self) -> bool {
        match self {
            Self::Parenthesized(expr) => expr.expr().is_empty_array_literal(),
            Self::Literal(LiteralExpr::Array(expr)) => expr.elements().next().is_none(),
            _ => false,
        }
    }
}

impl<N: TreeNode> AstNode<N> for Expr<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        if LiteralExpr::<N>::can_cast(kind) {
            return true;
        }

        matches!(
            kind,
            SyntaxKind::NameRefExprNode
                | SyntaxKind::ParenthesizedExprNode
                | SyntaxKind::IfExprNode
                | SyntaxKind::LogicalNotExprNode
                | SyntaxKind::NegationExprNode
                | SyntaxKind::LogicalOrExprNode
                | SyntaxKind::LogicalAndExprNode
                | SyntaxKind::EqualityExprNode
                | SyntaxKind::InequalityExprNode
                | SyntaxKind::LessExprNode
                | SyntaxKind::LessEqualExprNode
                | SyntaxKind::GreaterExprNode
                | SyntaxKind::GreaterEqualExprNode
                | SyntaxKind::AdditionExprNode
                | SyntaxKind::SubtractionExprNode
                | SyntaxKind::MultiplicationExprNode
                | SyntaxKind::DivisionExprNode
                | SyntaxKind::ModuloExprNode
                | SyntaxKind::ExponentiationExprNode
                | SyntaxKind::CallExprNode
                | SyntaxKind::IndexExprNode
                | SyntaxKind::AccessExprNode
        )
    }

    fn cast(inner: N) -> Option<Self> {
        if LiteralExpr::<N>::can_cast(inner.kind()) {
            return LiteralExpr::cast(inner).map(Self::Literal);
        }

        match inner.kind() {
            SyntaxKind::NameRefExprNode => Some(Self::NameRef(NameRefExpr(inner))),
            SyntaxKind::ParenthesizedExprNode => {
                Some(Self::Parenthesized(ParenthesizedExpr(inner)))
            }
            SyntaxKind::IfExprNode => Some(Self::If(IfExpr(inner))),
            SyntaxKind::LogicalNotExprNode => Some(Self::LogicalNot(LogicalNotExpr(inner))),
            SyntaxKind::NegationExprNode => Some(Self::Negation(NegationExpr(inner))),
            SyntaxKind::LogicalOrExprNode => Some(Self::LogicalOr(LogicalOrExpr(inner))),
            SyntaxKind::LogicalAndExprNode => Some(Self::LogicalAnd(LogicalAndExpr(inner))),
            SyntaxKind::EqualityExprNode => Some(Self::Equality(EqualityExpr(inner))),
            SyntaxKind::InequalityExprNode => Some(Self::Inequality(InequalityExpr(inner))),
            SyntaxKind::LessExprNode => Some(Self::Less(LessExpr(inner))),
            SyntaxKind::LessEqualExprNode => Some(Self::LessEqual(LessEqualExpr(inner))),
            SyntaxKind::GreaterExprNode => Some(Self::Greater(GreaterExpr(inner))),
            SyntaxKind::GreaterEqualExprNode => Some(Self::GreaterEqual(GreaterEqualExpr(inner))),
            SyntaxKind::AdditionExprNode => Some(Self::Addition(AdditionExpr(inner))),
            SyntaxKind::SubtractionExprNode => Some(Self::Subtraction(SubtractionExpr(inner))),
            SyntaxKind::MultiplicationExprNode => {
                Some(Self::Multiplication(MultiplicationExpr(inner)))
            }
            SyntaxKind::DivisionExprNode => Some(Self::Division(DivisionExpr(inner))),
            SyntaxKind::ModuloExprNode => Some(Self::Modulo(ModuloExpr(inner))),
            SyntaxKind::ExponentiationExprNode => {
                Some(Self::Exponentiation(ExponentiationExpr(inner)))
            }
            SyntaxKind::CallExprNode => Some(Self::Call(CallExpr(inner))),
            SyntaxKind::IndexExprNode => Some(Self::Index(IndexExpr(inner))),
            SyntaxKind::AccessExprNode => Some(Self::Access(AccessExpr(inner))),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        match self {
            Self::Literal(l) => l.inner(),
            Self::NameRef(n) => &n.0,
            Self::Parenthesized(p) => &p.0,
            Self::If(i) => &i.0,
            Self::LogicalNot(n) => &n.0,
            Self::Negation(n) => &n.0,
            Self::LogicalOr(o) => &o.0,
            Self::LogicalAnd(a) => &a.0,
            Self::Equality(e) => &e.0,
            Self::Inequality(i) => &i.0,
            Self::Less(l) => &l.0,
            Self::LessEqual(l) => &l.0,
            Self::Greater(g) => &g.0,
            Self::GreaterEqual(g) => &g.0,
            Self::Addition(a) => &a.0,
            Self::Subtraction(s) => &s.0,
            Self::Multiplication(m) => &m.0,
            Self::Division(d) => &d.0,
            Self::Modulo(m) => &m.0,
            Self::Exponentiation(e) => &e.0,
            Self::Call(c) => &c.0,
            Self::Index(i) => &i.0,
            Self::Access(a) => &a.0,
        }
    }
}

/// Represents a literal expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiteralExpr<N: TreeNode = SyntaxNode> {
    /// The literal is a `Boolean`.
    Boolean(LiteralBoolean<N>),
    /// The literal is an `Int`.
    Integer(LiteralInteger<N>),
    /// The literal is a `Float`.
    Float(LiteralFloat<N>),
    /// The literal is a `String`.
    String(LiteralString<N>),
    /// The literal is an `Array`.
    Array(LiteralArray<N>),
    /// The literal is a `Pair`.
    Pair(LiteralPair<N>),
    /// The literal is a `Map`.
    Map(LiteralMap<N>),
    /// The literal is an `Object`.
    Object(LiteralObject<N>),
    /// The literal is a struct.
    Struct(LiteralStruct<N>),
    /// The literal is a `None`.
    None(LiteralNone<N>),
    /// The literal is a `hints`.
    Hints(LiteralHints<N>),
    /// The literal is an `input`.
    Input(LiteralInput<N>),
    /// The literal is an `output`.
    Output(LiteralOutput<N>),
}

impl<N: TreeNode> LiteralExpr<N> {
    /// Returns whether or not the given syntax kind can be cast to
    /// [`LiteralExpr`].
    pub fn can_cast(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::LiteralBooleanNode
                | SyntaxKind::LiteralIntegerNode
                | SyntaxKind::LiteralFloatNode
                | SyntaxKind::LiteralStringNode
                | SyntaxKind::LiteralArrayNode
                | SyntaxKind::LiteralPairNode
                | SyntaxKind::LiteralMapNode
                | SyntaxKind::LiteralObjectNode
                | SyntaxKind::LiteralStructNode
                | SyntaxKind::LiteralNoneNode
                | SyntaxKind::LiteralHintsNode
                | SyntaxKind::LiteralInputNode
                | SyntaxKind::LiteralOutputNode
        )
    }

    /// Casts the given node to [`LiteralExpr`].
    ///
    /// Returns `None` if the node cannot be cast.
    pub fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::LiteralBooleanNode => Some(Self::Boolean(
                LiteralBoolean::cast(inner).expect("literal boolean to cast"),
            )),
            SyntaxKind::LiteralIntegerNode => Some(Self::Integer(
                LiteralInteger::cast(inner).expect("literal integer to cast"),
            )),
            SyntaxKind::LiteralFloatNode => Some(Self::Float(
                LiteralFloat::cast(inner).expect("literal float to cast"),
            )),
            SyntaxKind::LiteralStringNode => Some(Self::String(
                LiteralString::cast(inner).expect("literal string to cast"),
            )),
            SyntaxKind::LiteralArrayNode => Some(Self::Array(
                LiteralArray::cast(inner).expect("literal array to cast"),
            )),
            SyntaxKind::LiteralPairNode => Some(Self::Pair(
                LiteralPair::cast(inner).expect("literal pair to cast"),
            )),
            SyntaxKind::LiteralMapNode => Some(Self::Map(
                LiteralMap::cast(inner).expect("literal map to case"),
            )),
            SyntaxKind::LiteralObjectNode => Some(Self::Object(
                LiteralObject::cast(inner).expect("literal object to cast"),
            )),
            SyntaxKind::LiteralStructNode => Some(Self::Struct(
                LiteralStruct::cast(inner).expect("literal struct to cast"),
            )),
            SyntaxKind::LiteralNoneNode => Some(Self::None(
                LiteralNone::cast(inner).expect("literal none to cast"),
            )),
            SyntaxKind::LiteralHintsNode => Some(Self::Hints(
                LiteralHints::cast(inner).expect("literal hints to cast"),
            )),
            SyntaxKind::LiteralInputNode => Some(Self::Input(
                LiteralInput::cast(inner).expect("literal input to cast"),
            )),
            SyntaxKind::LiteralOutputNode => Some(Self::Output(
                LiteralOutput::cast(inner).expect("literal output to cast"),
            )),
            _ => None,
        }
    }

    /// Gets a reference to the inner node.
    pub fn inner(&self) -> &N {
        match self {
            Self::Boolean(e) => e.inner(),
            Self::Integer(e) => e.inner(),
            Self::Float(e) => e.inner(),
            Self::String(e) => e.inner(),
            Self::Array(e) => e.inner(),
            Self::Pair(e) => e.inner(),
            Self::Map(e) => e.inner(),
            Self::Object(e) => e.inner(),
            Self::Struct(e) => e.inner(),
            Self::None(e) => e.inner(),
            Self::Hints(e) => e.inner(),
            Self::Input(e) => e.inner(),
            Self::Output(e) => e.inner(),
        }
    }

    /// Attempts to get a reference to the inner [`LiteralBoolean`].
    ///
    /// * If `self` is a [`LiteralExpr::Boolean`], then a reference to the inner
    ///   [`LiteralBoolean`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_boolean(&self) -> Option<&LiteralBoolean<N>> {
        match self {
            Self::Boolean(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`LiteralBoolean`].
    ///
    /// * If `self` is a [`LiteralExpr::Boolean`], then the inner
    ///   [`LiteralBoolean`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_boolean(self) -> Option<LiteralBoolean<N>> {
        match self {
            Self::Boolean(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a literal boolean.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a literal boolean.
    pub fn unwrap_boolean(self) -> LiteralBoolean<N> {
        match self {
            Self::Boolean(e) => e,
            _ => panic!("not a literal boolean"),
        }
    }

    /// Attempts to get a reference to the inner [`LiteralInteger`].
    ///
    /// * If `self` is a [`LiteralExpr::Integer`], then a reference to the inner
    ///   [`LiteralInteger`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_integer(&self) -> Option<&LiteralInteger<N>> {
        match self {
            Self::Integer(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`LiteralInteger`].
    ///
    /// * If `self` is a [`LiteralExpr::Integer`], then the inner
    ///   [`LiteralInteger`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_integer(self) -> Option<LiteralInteger<N>> {
        match self {
            Self::Integer(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a literal integer.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a literal integer.
    pub fn unwrap_integer(self) -> LiteralInteger<N> {
        match self {
            Self::Integer(e) => e,
            _ => panic!("not a literal integer"),
        }
    }

    /// Attempts to get a reference to the inner [`LiteralFloat`].
    ///
    /// * If `self` is a [`LiteralExpr::Float`], then a reference to the inner
    ///   [`LiteralFloat`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_float(&self) -> Option<&LiteralFloat<N>> {
        match self {
            Self::Float(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`LiteralFloat`].
    ///
    /// * If `self` is a [`LiteralExpr::Float`], then the inner [`LiteralFloat`]
    ///   is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_float(self) -> Option<LiteralFloat<N>> {
        match self {
            Self::Float(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a literal float.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a literal float.
    pub fn unwrap_float(self) -> LiteralFloat<N> {
        match self {
            Self::Float(e) => e,
            _ => panic!("not a literal float"),
        }
    }

    /// Attempts to get a reference to the inner [`LiteralString`].
    ///
    /// * If `self` is a [`LiteralExpr::String`], then a reference to the inner
    ///   [`LiteralString`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_string(&self) -> Option<&LiteralString<N>> {
        match self {
            Self::String(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`LiteralString`].
    ///
    /// * If `self` is a [`LiteralExpr::String`], then the inner
    ///   [`LiteralString`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_string(self) -> Option<LiteralString<N>> {
        match self {
            Self::String(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a literal string.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a literal string.
    pub fn unwrap_string(self) -> LiteralString<N> {
        match self {
            Self::String(e) => e,
            _ => panic!("not a literal string"),
        }
    }

    /// Attempts to get a reference to the inner [`LiteralArray`].
    ///
    /// * If `self` is a [`LiteralExpr::Array`], then a reference to the inner
    ///   [`LiteralArray`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_array(&self) -> Option<&LiteralArray<N>> {
        match self {
            Self::Array(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`LiteralArray`].
    ///
    /// * If `self` is a [`LiteralExpr::Array`], then the inner [`LiteralArray`]
    ///   is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_array(self) -> Option<LiteralArray<N>> {
        match self {
            Self::Array(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a literal array.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a literal array.
    pub fn unwrap_array(self) -> LiteralArray<N> {
        match self {
            Self::Array(e) => e,
            _ => panic!("not a literal array"),
        }
    }

    /// Attempts to get a reference to the inner [`LiteralPair`].
    ///
    /// * If `self` is a [`LiteralExpr::Pair`], then a reference to the inner
    ///   [`LiteralPair`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_pair(&self) -> Option<&LiteralPair<N>> {
        match self {
            Self::Pair(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`LiteralPair`].
    ///
    /// * If `self` is a [`LiteralExpr::Pair`], then the inner [`LiteralPair`]
    ///   is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_pair(self) -> Option<LiteralPair<N>> {
        match self {
            Self::Pair(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a literal pair.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a literal pair.
    pub fn unwrap_pair(self) -> LiteralPair<N> {
        match self {
            Self::Pair(e) => e,
            _ => panic!("not a literal pair"),
        }
    }

    /// Attempts to get a reference to the inner [`LiteralMap`].
    ///
    /// * If `self` is a [`LiteralExpr::Map`], then a reference to the inner
    ///   [`LiteralMap`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_map(&self) -> Option<&LiteralMap<N>> {
        match self {
            Self::Map(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`LiteralMap`].
    ///
    /// * If `self` is a [`LiteralExpr::Map`], then the inner [`LiteralMap`] is
    ///   returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_map(self) -> Option<LiteralMap<N>> {
        match self {
            Self::Map(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a literal map.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a literal map.
    pub fn unwrap_map(self) -> LiteralMap<N> {
        match self {
            Self::Map(e) => e,
            _ => panic!("not a literal map"),
        }
    }

    /// Attempts to get a reference to the inner [`LiteralObject`].
    ///
    /// * If `self` is a [`LiteralExpr::Object`], then a reference to the inner
    ///   [`LiteralObject`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_object(&self) -> Option<&LiteralObject<N>> {
        match self {
            Self::Object(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`LiteralObject`].
    ///
    /// * If `self` is a [`LiteralExpr::Object`], then the inner
    ///   [`LiteralObject`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_object(self) -> Option<LiteralObject<N>> {
        match self {
            Self::Object(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a literal object.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a literal object.
    pub fn unwrap_object(self) -> LiteralObject<N> {
        match self {
            Self::Object(e) => e,
            _ => panic!("not a literal object"),
        }
    }

    /// Attempts to get a reference to the inner [`LiteralStruct`].
    ///
    /// * If `self` is a [`LiteralExpr::Struct`], then a reference to the inner
    ///   [`LiteralStruct`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_struct(&self) -> Option<&LiteralStruct<N>> {
        match self {
            Self::Struct(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`LiteralStruct`].
    ///
    /// * If `self` is a [`LiteralExpr::Struct`], then the inner
    ///   [`LiteralStruct`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_struct(self) -> Option<LiteralStruct<N>> {
        match self {
            Self::Struct(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a literal struct.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a literal struct.
    pub fn unwrap_struct(self) -> LiteralStruct<N> {
        match self {
            Self::Struct(e) => e,
            _ => panic!("not a literal struct"),
        }
    }

    /// Attempts to get a reference to the inner [`LiteralNone`].
    ///
    /// * If `self` is a [`LiteralExpr::None`], then a reference to the inner
    ///   [`LiteralNone`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_none(&self) -> Option<&LiteralNone<N>> {
        match self {
            Self::None(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`LiteralNone`].
    ///
    /// * If `self` is a [`LiteralExpr::None`], then the inner [`LiteralNone`]
    ///   is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_none(self) -> Option<LiteralNone<N>> {
        match self {
            Self::None(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a literal `None`.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a literal `None`.
    pub fn unwrap_none(self) -> LiteralNone<N> {
        match self {
            Self::None(e) => e,
            _ => panic!("not a literal `None`"),
        }
    }

    /// Attempts to get a reference to the inner [`LiteralHints`].
    ///
    /// * If `self` is a [`LiteralExpr::Hints`], then a reference to the inner
    ///   [`LiteralHints`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_hints(&self) -> Option<&LiteralHints<N>> {
        match self {
            Self::Hints(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`LiteralHints`].
    ///
    /// * If `self` is a [`LiteralExpr::Hints`], then the inner [`LiteralHints`]
    ///   is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_hints(self) -> Option<LiteralHints<N>> {
        match self {
            Self::Hints(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a literal `hints`.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a literal `hints`.
    pub fn unwrap_hints(self) -> LiteralHints<N> {
        match self {
            Self::Hints(e) => e,
            _ => panic!("not a literal `hints`"),
        }
    }

    /// Attempts to get a reference to the inner [`LiteralInput`].
    ///
    /// * If `self` is a [`LiteralExpr::Input`], then a reference to the inner
    ///   [`LiteralInput`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_input(&self) -> Option<&LiteralInput<N>> {
        match self {
            Self::Input(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`LiteralInput`].
    ///
    /// * If `self` is a [`LiteralExpr::Input`], then the inner [`LiteralInput`]
    ///   is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_input(self) -> Option<LiteralInput<N>> {
        match self {
            Self::Input(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a literal `input`.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a literal `input`.
    pub fn unwrap_input(self) -> LiteralInput<N> {
        match self {
            Self::Input(e) => e,
            _ => panic!("not a literal `input`"),
        }
    }

    /// Attempts to get a reference to the inner [`LiteralOutput`].
    ///
    /// * If `self` is a [`LiteralExpr::Output`], then a reference to the inner
    ///   [`LiteralOutput`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_output(&self) -> Option<&LiteralOutput<N>> {
        match self {
            Self::Output(e) => Some(e),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`LiteralOutput`].
    ///
    /// * If `self` is a [`LiteralExpr::Output`], then the inner
    ///   [`LiteralOutput`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_output(self) -> Option<LiteralOutput<N>> {
        match self {
            Self::Output(e) => Some(e),
            _ => None,
        }
    }

    /// Unwraps the expression into a literal `output`.
    ///
    /// # Panics
    ///
    /// Panics if the expression is not a literal `output`.
    pub fn unwrap_output(self) -> LiteralOutput<N> {
        match self {
            Self::Output(e) => e,
            _ => panic!("not a literal `output`"),
        }
    }

    /// Finds the first child that can be cast to a [`LiteralExpr`].
    pub fn child(node: &N) -> Option<Self> {
        node.children().find_map(Self::cast)
    }

    /// Finds all children that can be cast to a [`LiteralExpr`].
    pub fn children(node: &N) -> impl Iterator<Item = Self> + use<'_, N> {
        node.children().filter_map(Self::cast)
    }
}

/// Represents a literal boolean.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiteralBoolean<N: TreeNode = SyntaxNode>(pub(super) N);

impl<N: TreeNode> LiteralBoolean<N> {
    /// Gets the value of the literal boolean.
    pub fn value(&self) -> bool {
        self.0
            .children_with_tokens()
            .find_map(|c| {
                c.into_token().and_then(|t| match t.kind() {
                    SyntaxKind::TrueKeyword => Some(true),
                    SyntaxKind::FalseKeyword => Some(false),
                    _ => None,
                })
            })
            .expect("`true` or `false` keyword should be present")
    }
}

impl<N: TreeNode> AstNode<N> for LiteralBoolean<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LiteralBooleanNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::LiteralBooleanNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents an integer token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Integer<T: TreeToken = SyntaxToken>(T);

impl<T: TreeToken> AstToken<T> for Integer<T> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::Integer
    }

    fn cast(inner: T) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::Integer => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &T {
        &self.0
    }
}

/// Represents a literal integer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiteralInteger<N: TreeNode = SyntaxNode>(pub(super) N);

impl<N: TreeNode> LiteralInteger<N> {
    /// Gets the minus token for the literal integer.
    ///
    /// A minus token *only* occurs in metadata sections, where
    /// expressions are not allowed and a prefix `-` is included
    /// in the literal integer itself.
    ///
    /// Otherwise, a prefix `-` would be a negation expression and not
    /// part of the literal integer.
    pub fn minus(&self) -> Option<Minus<N::Token>> {
        self.token()
    }

    /// Gets the integer token for the literal.
    pub fn integer(&self) -> Integer<N::Token> {
        self.token().expect("should have integer token")
    }

    /// Gets the value of the literal integer.
    ///
    /// Returns `None` if the value is out of range.
    pub fn value(&self) -> Option<i64> {
        let value = self.as_u64()?;

        // If there's a minus sign present, negate the value; this may
        // only occur in metadata sections
        if self.minus().is_some() {
            if value == (i64::MAX as u64) + 1 {
                return Some(i64::MIN);
            }

            return Some(-(value as i64));
        }

        if value == (i64::MAX as u64) + 1 {
            return None;
        }

        Some(value as i64)
    }

    /// Gets the negated value of the literal integer.
    ///
    /// Returns `None` if the resulting negation would overflow.
    ///
    /// This is used as part of negation expressions.
    pub fn negate(&self) -> Option<i64> {
        let value = self.as_u64()?;

        // Check for "double" negation
        if self.minus().is_some() {
            // Can't negate i64::MIN as that would overflow
            if value == (i64::MAX as u64) + 1 {
                return None;
            }

            return Some(value as i64);
        }

        if value == (i64::MAX as u64) + 1 {
            return Some(i64::MIN);
        }

        Some(-(value as i64))
    }

    /// Gets the unsigned representation of the literal integer.
    ///
    /// This returns `None` if the integer is out of range for a 64-bit signed
    /// integer, excluding `i64::MAX + 1` to allow for negation.
    fn as_u64(&self) -> Option<u64> {
        let token = self.integer();
        let text = token.text();
        let i = if text == "0" {
            0
        } else if text.starts_with("0x") || text.starts_with("0X") {
            u64::from_str_radix(&text[2..], 16).ok()?
        } else if text.starts_with('0') {
            u64::from_str_radix(text, 8).ok()?
        } else {
            text.parse::<u64>().ok()?
        };

        // Allow 1 more than the maximum to account for negation
        if i > (i64::MAX as u64) + 1 {
            None
        } else {
            Some(i)
        }
    }
}

impl<N: TreeNode> AstNode<N> for LiteralInteger<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LiteralIntegerNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::LiteralIntegerNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a float token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Float<T: TreeToken = SyntaxToken>(T);

impl<T: TreeToken> AstToken<T> for Float<T> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::Float
    }

    fn cast(inner: T) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::Float => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &T {
        &self.0
    }
}

/// Represents a literal float.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiteralFloat<N: TreeNode = SyntaxNode>(pub(crate) N);

impl<N: TreeNode> LiteralFloat<N> {
    /// Gets the minus token for the literal float.
    ///
    /// A minus token *only* occurs in metadata sections, where
    /// expressions are not allowed and a prefix `-` is included
    /// in the literal float itself.
    ///
    /// Otherwise, a prefix `-` would be a negation expression and not
    /// part of the literal float.
    pub fn minus(&self) -> Option<Minus<N::Token>> {
        self.token()
    }

    /// Gets the float token for the literal.
    pub fn float(&self) -> Float<N::Token> {
        self.token().expect("should have float token")
    }

    /// Gets the value of the literal float.
    ///
    /// Returns `None` if the literal value is not in range.
    pub fn value(&self) -> Option<f64> {
        self.float()
            .text()
            .parse()
            .ok()
            .and_then(|f: f64| if f.is_infinite() { None } else { Some(f) })
    }
}

impl<N: TreeNode> AstNode<N> for LiteralFloat<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LiteralFloatNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::LiteralFloatNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents the kind of a literal string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiteralStringKind {
    /// The string is a single quoted string.
    SingleQuoted,
    /// The string is a double quoted string.
    DoubleQuoted,
    /// The string is a multi-line string.
    Multiline,
}

/// Represents a multi-line string that's been stripped of leading whitespace
/// and it's line continuations parsed. Placeholders are not changed and are
/// copied as-is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StrippedStringPart<N: TreeNode = SyntaxNode> {
    /// A textual part of the string.
    Text(String),
    /// A placeholder encountered in the string.
    Placeholder(Placeholder<N>),
}

/// Unescapes a multiline string.
///
/// This unescapes both line continuations and `\>` sequences.
fn unescape_multiline_string(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.peek() {
                Some('\r') => {
                    chars.next();
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                        while let Some(&next) = chars.peek() {
                            if next == ' ' || next == '\t' {
                                chars.next();
                                continue;
                            }

                            break;
                        }
                    } else {
                        result.push_str("\\\r");
                    }
                }
                Some('\n') => {
                    chars.next();
                    while let Some(&next) = chars.peek() {
                        if next == ' ' || next == '\t' {
                            chars.next();
                            continue;
                        }

                        break;
                    }
                }
                Some('\\') | Some('>') | Some('~') | Some('$') => {
                    result.push(chars.next().unwrap());
                }
                _ => {
                    result.push('\\');
                }
            },
            _ => {
                result.push(c);
            }
        }
    }
    result
}

/// Represents a literal string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiteralString<N: TreeNode = SyntaxNode>(pub(super) N);

impl<N: TreeNode> LiteralString<N> {
    /// Gets the kind of the string literal.
    pub fn kind(&self) -> LiteralStringKind {
        self.0
            .children_with_tokens()
            .find_map(|c| {
                c.into_token().and_then(|t| match t.kind() {
                    SyntaxKind::SingleQuote => Some(LiteralStringKind::SingleQuoted),
                    SyntaxKind::DoubleQuote => Some(LiteralStringKind::DoubleQuoted),
                    SyntaxKind::OpenHeredoc => Some(LiteralStringKind::Multiline),
                    _ => None,
                })
            })
            .expect("string is missing opening token")
    }

    /// Determines if the literal is the empty string.
    pub fn is_empty(&self) -> bool {
        self.0
            .children_with_tokens()
            .filter_map(StringPart::cast)
            .next()
            .is_none()
    }

    /// Gets the parts of the string.
    ///
    /// A part may be literal text or an interpolated expression.
    pub fn parts(&self) -> impl Iterator<Item = StringPart<N>> + use<'_, N> {
        self.0.children_with_tokens().filter_map(StringPart::cast)
    }

    /// Gets the string text if the string is not empty and is not interpolated (i.e.
    /// has no placeholders).
    ///
    /// Returns `None` if the string is interpolated, as
    /// interpolated strings cannot be represented as a single
    /// span of text.
    pub fn text(&self) -> Option<StringText<N::Token>> {
        let mut parts = self.parts();
        if let Some(StringPart::Text(text)) = parts.next()
            && parts.next().is_none()
        {
            return Some(text);
        }

        None
    }

    /// Strips leading whitespace from a multi-line string.
    ///
    /// This function will remove leading and trailing whitespace and handle
    /// unescaping the string.
    ///
    /// Returns `None` if not a multi-line string.
    pub fn strip_whitespace(&self) -> Option<Vec<StrippedStringPart<N>>> {
        if self.kind() != LiteralStringKind::Multiline {
            return None;
        }

        // Unescape each line
        let mut result = Vec::new();
        for part in self.parts() {
            match part {
                StringPart::Text(text) => {
                    result.push(StrippedStringPart::Text(unescape_multiline_string(
                        text.text(),
                    )));
                }
                StringPart::Placeholder(placeholder) => {
                    result.push(StrippedStringPart::Placeholder(placeholder));
                }
            }
        }

        // Trim the first line
        let mut whole_first_line_trimmed = false;
        if let Some(StrippedStringPart::Text(text)) = result.first_mut() {
            let end_of_first_line = text.find('\n').map(|p| p + 1).unwrap_or(text.len());
            let line = &text[..end_of_first_line];
            let len = line.len() - line.trim_start().len();
            whole_first_line_trimmed = len == line.len();
            text.replace_range(..len, "");
        }

        // Trim the last line
        if let Some(StrippedStringPart::Text(text)) = result.last_mut() {
            if let Some(index) = text.rfind(|c| !matches!(c, ' ' | '\t')) {
                text.truncate(index + 1);
            } else {
                text.clear();
            }

            if text.ends_with('\n') {
                text.pop();
            }

            if text.ends_with('\r') {
                text.pop();
            }
        }

        // Now that the string has been unescaped and the first and last lines trimmed,
        // we can detect any leading whitespace and trim it.
        let mut leading_whitespace = usize::MAX;
        let mut parsing_leading_whitespace = true;
        let mut iter = result.iter().peekable();
        while let Some(part) = iter.next() {
            match part {
                StrippedStringPart::Text(text) => {
                    for (i, line) in text.lines().enumerate() {
                        if i > 0 {
                            parsing_leading_whitespace = true;
                        }

                        if parsing_leading_whitespace {
                            let mut ws_count = 0;
                            for c in line.chars() {
                                if c == ' ' || c == '\t' {
                                    ws_count += 1;
                                } else {
                                    break;
                                }
                            }

                            // Don't include blank lines in determining leading whitespace, unless
                            // the next part is a placeholder
                            if ws_count == line.len()
                                && iter
                                    .peek()
                                    .map(|p| !matches!(p, StrippedStringPart::Placeholder(_)))
                                    .unwrap_or(true)
                            {
                                continue;
                            }

                            leading_whitespace = leading_whitespace.min(ws_count);
                        }
                    }
                }
                StrippedStringPart::Placeholder(_) => {
                    parsing_leading_whitespace = false;
                }
            }
        }

        // Finally, strip the leading whitespace on each line
        // This is done in place using the `replace_range` method; the method will
        // internally do moves without allocations
        let mut strip_leading_whitespace = whole_first_line_trimmed;
        for part in &mut result {
            match part {
                StrippedStringPart::Text(text) => {
                    let mut offset = 0;
                    while let Some(next) = text[offset..].find('\n') {
                        let next = next + offset;
                        if offset > 0 {
                            strip_leading_whitespace = true;
                        }

                        if !strip_leading_whitespace {
                            offset = next + 1;
                            continue;
                        }

                        let line = &text[offset..next];
                        let line = line.strip_suffix('\r').unwrap_or(line);
                        let len = line.len().min(leading_whitespace);
                        text.replace_range(offset..offset + len, "");
                        offset = next + 1 - len;
                    }

                    // Replace any remaining text
                    if strip_leading_whitespace || offset > 0 {
                        let line = &text[offset..];
                        let line = line.strip_suffix('\r').unwrap_or(line);
                        let len = line.len().min(leading_whitespace);
                        text.replace_range(offset..offset + len, "");
                    }
                }
                StrippedStringPart::Placeholder(_) => {
                    strip_leading_whitespace = false;
                }
            }
        }

        Some(result)
    }
}

impl<N: TreeNode> AstNode<N> for LiteralString<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LiteralStringNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::LiteralStringNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a part of a string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StringPart<N: TreeNode = SyntaxNode> {
    /// A textual part of the string.
    Text(StringText<N::Token>),
    /// A placeholder encountered in the string.
    Placeholder(Placeholder<N>),
}

impl<N: TreeNode> StringPart<N> {
    /// Unwraps the string part into text.
    ///
    /// # Panics
    ///
    /// Panics if the string part is not text.
    pub fn unwrap_text(self) -> StringText<N::Token> {
        match self {
            Self::Text(text) => text,
            _ => panic!("not string text"),
        }
    }

    /// Unwraps the string part into a placeholder.
    ///
    /// # Panics
    ///
    /// Panics if the string part is not a placeholder.
    pub fn unwrap_placeholder(self) -> Placeholder<N> {
        match self {
            Self::Placeholder(p) => p,
            _ => panic!("not a placeholder"),
        }
    }

    /// Casts the given syntax element to a string part.
    fn cast(element: NodeOrToken<N, N::Token>) -> Option<Self> {
        match element {
            NodeOrToken::Node(n) => Some(Self::Placeholder(Placeholder::cast(n)?)),
            NodeOrToken::Token(t) => Some(Self::Text(StringText::cast(t)?)),
        }
    }
}

/// Represents a textual part of a string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StringText<T: TreeToken = SyntaxToken>(T);

impl<T: TreeToken> StringText<T> {
    /// Unescapes the string text to the given buffer.
    ///
    /// If the string text contains invalid escape sequences, they are left
    /// as-is.
    pub fn unescape_to(&self, buffer: &mut String) {
        let text = self.0.text();
        let lexer = EscapeToken::lexer(text).spanned();
        for (token, span) in lexer {
            match token.expect("should lex") {
                EscapeToken::Valid => {
                    match &text[span] {
                        r"\\" => buffer.push('\\'),
                        r"\n" => buffer.push('\n'),
                        r"\r" => buffer.push('\r'),
                        r"\t" => buffer.push('\t'),
                        r"\'" => buffer.push('\''),
                        r#"\""# => buffer.push('"'),
                        r"\~" => buffer.push('~'),
                        r"\$" => buffer.push('$'),
                        _ => unreachable!("unexpected escape token"),
                    }
                    continue;
                }
                EscapeToken::ValidOctal => {
                    if let Some(c) = char::from_u32(
                        u32::from_str_radix(&text[span.start + 1..span.end], 8)
                            .expect("should be a valid octal number"),
                    ) {
                        buffer.push(c);
                        continue;
                    }
                }
                EscapeToken::ValidHex => {
                    buffer.push(
                        u8::from_str_radix(&text[span.start + 2..span.end], 16)
                            .expect("should be a valid hex number") as char,
                    );
                    continue;
                }
                EscapeToken::ValidUnicode => {
                    if let Some(c) = char::from_u32(
                        u32::from_str_radix(&text[span.start + 2..span.end], 16)
                            .expect("should be a valid hex number"),
                    ) {
                        buffer.push(c);
                        continue;
                    }
                }
                _ => {
                    // Write the token to the buffer below
                }
            }

            buffer.push_str(&text[span]);
        }
    }
}

impl<T: TreeToken> AstToken<T> for StringText<T> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LiteralStringText
    }

    fn cast(inner: T) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::LiteralStringText => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &T {
        &self.0
    }
}

/// Represents a placeholder in a string or command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placeholder<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> Placeholder<N> {
    /// Returns whether or not placeholder has a tilde (`~`) opening.
    ///
    /// If this method returns false, the opening was a dollar sign (`$`).
    pub fn has_tilde(&self) -> bool {
        self.0
            .children_with_tokens()
            .find_map(|c| {
                c.into_token().and_then(|t| match t.kind() {
                    SyntaxKind::PlaceholderOpen => Some(t.text().starts_with('~')),
                    _ => None,
                })
            })
            .expect("should have a placeholder open token")
    }

    /// Gets the option for the placeholder.
    pub fn option(&self) -> Option<PlaceholderOption<N>> {
        self.child()
    }

    /// Gets the placeholder expression.
    pub fn expr(&self) -> Expr<N> {
        Expr::child(&self.0).expect("placeholder should have an expression")
    }
}

impl<N: TreeNode> AstNode<N> for Placeholder<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PlaceholderNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::PlaceholderNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a placeholder option.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaceholderOption<N: TreeNode = SyntaxNode> {
    /// A `sep` option for specifying a delimiter for formatting arrays.
    Sep(SepOption<N>),
    /// A `default` option for substituting a default value for an undefined
    /// expression.
    Default(DefaultOption<N>),
    /// A `true/false` option for substituting a value depending on whether a
    /// boolean expression is true or false.
    TrueFalse(TrueFalseOption<N>),
}

impl<N: TreeNode> PlaceholderOption<N> {
    /// Attempts to get a reference to the inner [`SepOption`].
    ///
    /// * If `self` is a [`PlaceholderOption::Sep`], then a reference to the
    ///   inner [`SepOption`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_sep(&self) -> Option<&SepOption<N>> {
        match self {
            Self::Sep(o) => Some(o),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`SepOption`].
    ///
    /// * If `self` is a [`PlaceholderOption::Sep`], then the inner
    ///   [`SepOption`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_sep(self) -> Option<SepOption<N>> {
        match self {
            Self::Sep(o) => Some(o),
            _ => None,
        }
    }

    /// Unwraps the option into a separator option.
    ///
    /// # Panics
    ///
    /// Panics if the option is not a separator option.
    pub fn unwrap_sep(self) -> SepOption<N> {
        match self {
            Self::Sep(o) => o,
            _ => panic!("not a separator option"),
        }
    }

    /// Attempts to get a reference to the inner [`DefaultOption`].
    ///
    /// * If `self` is a [`PlaceholderOption::Default`], then a reference to the
    ///   inner [`DefaultOption`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_default(&self) -> Option<&DefaultOption<N>> {
        match self {
            Self::Default(o) => Some(o),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`DefaultOption`].
    ///
    /// * If `self` is a [`PlaceholderOption::Default`], then the inner
    ///   [`DefaultOption`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_default(self) -> Option<DefaultOption<N>> {
        match self {
            Self::Default(o) => Some(o),
            _ => None,
        }
    }

    /// Unwraps the option into a default option.
    ///
    /// # Panics
    ///
    /// Panics if the option is not a default option.
    pub fn unwrap_default(self) -> DefaultOption<N> {
        match self {
            Self::Default(o) => o,
            _ => panic!("not a default option"),
        }
    }

    /// Attempts to get a reference to the inner [`TrueFalseOption`].
    ///
    /// * If `self` is a [`PlaceholderOption::TrueFalse`], then a reference to
    ///   the inner [`TrueFalseOption`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_true_false(&self) -> Option<&TrueFalseOption<N>> {
        match self {
            Self::TrueFalse(o) => Some(o),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`TrueFalseOption`].
    ///
    /// * If `self` is a [`PlaceholderOption::TrueFalse`], then the inner
    ///   [`TrueFalseOption`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_true_false(self) -> Option<TrueFalseOption<N>> {
        match self {
            Self::TrueFalse(o) => Some(o),
            _ => None,
        }
    }

    /// Unwraps the option into a true/false option.
    ///
    /// # Panics
    ///
    /// Panics if the option is not a true/false option.
    pub fn unwrap_true_false(self) -> TrueFalseOption<N> {
        match self {
            Self::TrueFalse(o) => o,
            _ => panic!("not a true/false option"),
        }
    }

    /// Finds the first child that can be cast to a [`PlaceholderOption`].
    pub fn child(node: &N) -> Option<Self> {
        node.children().find_map(Self::cast)
    }

    /// Finds all children that can be cast to a [`PlaceholderOption`].
    pub fn children(node: &N) -> impl Iterator<Item = Self> + use<'_, N> {
        node.children().filter_map(Self::cast)
    }
}

impl<N: TreeNode> AstNode<N> for PlaceholderOption<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::PlaceholderSepOptionNode
                | SyntaxKind::PlaceholderDefaultOptionNode
                | SyntaxKind::PlaceholderTrueFalseOptionNode
        )
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::PlaceholderSepOptionNode => Some(Self::Sep(SepOption(inner))),
            SyntaxKind::PlaceholderDefaultOptionNode => Some(Self::Default(DefaultOption(inner))),
            SyntaxKind::PlaceholderTrueFalseOptionNode => {
                Some(Self::TrueFalse(TrueFalseOption(inner)))
            }
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        match self {
            Self::Sep(s) => &s.0,
            Self::Default(d) => &d.0,
            Self::TrueFalse(tf) => &tf.0,
        }
    }
}

/// Represents a `sep` option for a placeholder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SepOption<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> SepOption<N> {
    /// Gets the separator to use for formatting an array.
    pub fn separator(&self) -> LiteralString<N> {
        self.child()
            .expect("sep option should have a string literal")
    }
}

impl<N: TreeNode> AstNode<N> for SepOption<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PlaceholderSepOptionNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::PlaceholderSepOptionNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a `default` option for a placeholder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefaultOption<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> DefaultOption<N> {
    /// Gets the value to use for an undefined expression.
    pub fn value(&self) -> LiteralString<N> {
        self.child()
            .expect("default option should have a string literal")
    }
}

impl<N: TreeNode> AstNode<N> for DefaultOption<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PlaceholderDefaultOptionNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::PlaceholderDefaultOptionNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a `true/false` option for a placeholder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrueFalseOption<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> TrueFalseOption<N> {
    /// Gets the `true` and `false` values to use for a placeholder
    /// expression that evaluates to a boolean.
    ///
    /// The first value returned is the `true` value and the second
    /// value is the `false` value.
    pub fn values(&self) -> (LiteralString<N>, LiteralString<N>) {
        let mut true_value = None;
        let mut false_value = None;
        let mut found = None;
        let mut children = self.0.children_with_tokens();
        for child in children.by_ref() {
            match child {
                NodeOrToken::Token(t) if t.kind() == SyntaxKind::TrueKeyword => {
                    found = Some(true);
                }
                NodeOrToken::Token(t) if t.kind() == SyntaxKind::FalseKeyword => {
                    found = Some(false);
                }
                NodeOrToken::Node(n) if LiteralString::<N>::can_cast(n.kind()) => {
                    if found.expect("should have found true or false") {
                        assert!(true_value.is_none(), "multiple true values present");
                        true_value = Some(LiteralString(n));
                    } else {
                        assert!(false_value.is_none(), "multiple false values present");
                        false_value = Some(LiteralString(n));
                    }

                    if true_value.is_some() && false_value.is_some() {
                        break;
                    }
                }
                _ => continue,
            }
        }

        (
            true_value.expect("expected a true value to be present"),
            false_value.expect("expected a false value to be present`"),
        )
    }
}

impl<N: TreeNode> AstNode<N> for TrueFalseOption<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PlaceholderTrueFalseOptionNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::PlaceholderTrueFalseOptionNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a literal array.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiteralArray<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> LiteralArray<N> {
    /// Gets the elements of the literal array.
    pub fn elements(&self) -> impl Iterator<Item = Expr<N>> + use<'_, N> {
        Expr::children(&self.0)
    }
}

impl<N: TreeNode> AstNode<N> for LiteralArray<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LiteralArrayNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::LiteralArrayNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a literal pair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiteralPair<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> LiteralPair<N> {
    /// Gets the first and second expressions in the literal pair.
    pub fn exprs(&self) -> (Expr<N>, Expr<N>) {
        let mut children = self.0.children().filter_map(Expr::cast);
        let left = children.next().expect("pair should have a left expression");
        let right = children
            .next()
            .expect("pair should have a right expression");
        (left, right)
    }
}

impl<N: TreeNode> AstNode<N> for LiteralPair<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LiteralPairNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::LiteralPairNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a literal map.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiteralMap<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> LiteralMap<N> {
    /// Gets the items of the literal map.
    pub fn items(&self) -> impl Iterator<Item = LiteralMapItem<N>> + use<'_, N> {
        self.children()
    }
}

impl<N: TreeNode> AstNode<N> for LiteralMap<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LiteralMapNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::LiteralMapNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a literal map item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiteralMapItem<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> LiteralMapItem<N> {
    /// Gets the key and the value of the item.
    pub fn key_value(&self) -> (Expr<N>, Expr<N>) {
        let mut children = Expr::children(&self.0);
        let key = children.next().expect("expected a key expression");
        let value = children.next().expect("expected a value expression");
        (key, value)
    }
}

impl<N: TreeNode> AstNode<N> for LiteralMapItem<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LiteralMapItemNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::LiteralMapItemNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a literal object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiteralObject<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> LiteralObject<N> {
    /// Gets the items of the literal object.
    pub fn items(&self) -> impl Iterator<Item = LiteralObjectItem<N>> + use<'_, N> {
        self.children()
    }
}

impl<N: TreeNode> AstNode<N> for LiteralObject<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LiteralObjectNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::LiteralObjectNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Gets the name and value of a object or struct literal item.
fn name_value<N: TreeNode, T: AstNode<N>>(parent: &T) -> (Ident<N::Token>, Expr<N>) {
    let key = parent.token().expect("expected a key token");
    let value = Expr::child(parent.inner()).expect("expected a value expression");
    (key, value)
}

/// Represents a literal object item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiteralObjectItem<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> LiteralObjectItem<N> {
    /// Gets the name and the value of the item.
    pub fn name_value(&self) -> (Ident<N::Token>, Expr<N>) {
        name_value(self)
    }
}

impl<N: TreeNode> AstNode<N> for LiteralObjectItem<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LiteralObjectItemNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::LiteralObjectItemNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a literal struct.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiteralStruct<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> LiteralStruct<N> {
    /// Gets the name of the struct.
    pub fn name(&self) -> Ident<N::Token> {
        self.token().expect("expected the struct to have a name")
    }

    /// Gets the items of the literal struct.
    pub fn items(&self) -> impl Iterator<Item = LiteralStructItem<N>> + use<'_, N> {
        self.children()
    }
}

impl<N: TreeNode> AstNode<N> for LiteralStruct<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LiteralStructNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::LiteralStructNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a literal struct item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiteralStructItem<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> LiteralStructItem<N> {
    /// Gets the name and the value of the item.
    pub fn name_value(&self) -> (Ident<N::Token>, Expr<N>) {
        name_value(self)
    }
}

impl<N: TreeNode> AstNode<N> for LiteralStructItem<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LiteralStructItemNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::LiteralStructItemNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a literal `None`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiteralNone<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> AstNode<N> for LiteralNone<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LiteralNoneNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::LiteralNoneNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a literal `hints`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiteralHints<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> LiteralHints<N> {
    /// Gets the items of the literal hints.
    pub fn items(&self) -> impl Iterator<Item = LiteralHintsItem<N>> + use<'_, N> {
        self.children()
    }
}

impl<N: TreeNode> AstNode<N> for LiteralHints<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LiteralHintsNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::LiteralHintsNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a literal hints item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiteralHintsItem<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> LiteralHintsItem<N> {
    /// Gets the name of the hints item.
    pub fn name(&self) -> Ident<N::Token> {
        self.token().expect("expected an item name")
    }

    /// Gets the expression of the hints item.
    pub fn expr(&self) -> Expr<N> {
        Expr::child(&self.0).expect("expected an item expression")
    }
}

impl<N: TreeNode> AstNode<N> for LiteralHintsItem<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LiteralHintsItemNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::LiteralHintsItemNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a literal `input`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiteralInput<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> LiteralInput<N> {
    /// Gets the items of the literal input.
    pub fn items(&self) -> impl Iterator<Item = LiteralInputItem<N>> + use<'_, N> {
        self.children()
    }
}

impl<N: TreeNode> AstNode<N> for LiteralInput<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LiteralInputNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::LiteralInputNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a literal input item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiteralInputItem<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> LiteralInputItem<N> {
    /// Gets the names of the input item.
    ///
    /// More than one name indicates a struct member path.
    pub fn names(&self) -> impl Iterator<Item = Ident<N::Token>> + use<'_, N> {
        self.0
            .children_with_tokens()
            .filter_map(NodeOrToken::into_token)
            .filter_map(Ident::cast)
    }

    /// Gets the expression of the input item.
    pub fn expr(&self) -> Expr<N> {
        Expr::child(&self.0).expect("expected an item expression")
    }
}

impl<N: TreeNode> AstNode<N> for LiteralInputItem<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LiteralInputItemNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::LiteralInputItemNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a literal `output`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiteralOutput<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> LiteralOutput<N> {
    /// Gets the items of the literal output.
    pub fn items(&self) -> impl Iterator<Item = LiteralOutputItem<N>> + use<'_, N> {
        self.children()
    }
}

impl<N: TreeNode> AstNode<N> for LiteralOutput<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LiteralOutputNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::LiteralOutputNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a literal output item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiteralOutputItem<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> LiteralOutputItem<N> {
    /// Gets the names of the output item.
    ///
    /// More than one name indicates a struct member path.
    pub fn names(&self) -> impl Iterator<Item = Ident<N::Token>> + use<'_, N> {
        self.0
            .children_with_tokens()
            .filter_map(NodeOrToken::into_token)
            .filter_map(Ident::cast)
    }

    /// Gets the expression of the output item.
    pub fn expr(&self) -> Expr<N> {
        Expr::child(&self.0).expect("expected an item expression")
    }
}

impl<N: TreeNode> AstNode<N> for LiteralOutputItem<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LiteralOutputItemNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::LiteralOutputItemNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a name reference expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameRefExpr<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> NameRefExpr<N> {
    /// Gets the name being referenced.
    pub fn name(&self) -> Ident<N::Token> {
        self.token().expect("expected a name")
    }
}

impl<N: TreeNode> AstNode<N> for NameRefExpr<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::NameRefExprNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::NameRefExprNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a parenthesized expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParenthesizedExpr<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> ParenthesizedExpr<N> {
    /// Gets the inner expression.
    pub fn expr(&self) -> Expr<N> {
        Expr::child(&self.0).expect("expected an inner expression")
    }
}

impl<N: TreeNode> AstNode<N> for ParenthesizedExpr<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ParenthesizedExprNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::ParenthesizedExprNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents an `if` expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IfExpr<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> IfExpr<N> {
    /// Gets the three expressions of the `if` expression
    ///
    /// The first expression is the conditional.
    /// The second expression is the `true` expression.
    /// The third expression is the `false` expression.
    pub fn exprs(&self) -> (Expr<N>, Expr<N>, Expr<N>) {
        let mut children = Expr::children(&self.0);
        let conditional = children
            .next()
            .expect("should have a conditional expression");
        let true_expr = children.next().expect("should have a `true` expression");
        let false_expr = children.next().expect("should have a `false` expression");
        (conditional, true_expr, false_expr)
    }
}

impl<N: TreeNode> AstNode<N> for IfExpr<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::IfExprNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::IfExprNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Used to declare a prefix expression.
macro_rules! prefix_expression {
    ($name:ident, $kind:ident, $desc:literal) => {
        #[doc = concat!("Represents a ", $desc, " expression.")]
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct $name<N: TreeNode = SyntaxNode>(N);

        impl<N: TreeNode> $name<N> {
            /// Gets the operand expression.
            pub fn operand(&self) -> Expr<N> {
                Expr::child(&self.0).expect("expected an operand expression")
            }
        }

        impl<N: TreeNode> AstNode<N> for $name<N> {
            fn can_cast(kind: SyntaxKind) -> bool {
                kind == SyntaxKind::$kind
            }

            fn cast(inner: N) -> Option<Self> {
                match inner.kind() {
                    SyntaxKind::$kind => Some(Self(inner)),
                    _ => None,
                }
            }

            fn inner(&self) -> &N {
                &self.0
            }
        }
    };
}

/// Used to declare an infix expression.
macro_rules! infix_expression {
    ($name:ident, $kind:ident, $desc:literal) => {
        #[doc = concat!("Represents a ", $desc, " expression.")]
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct $name<N: TreeNode = SyntaxNode>(N);

        impl<N: TreeNode> $name<N> {
            /// Gets the operands of the expression.
            pub fn operands(&self) -> (Expr<N>, Expr<N>) {
                let mut children = Expr::children(&self.0);
                let lhs = children.next().expect("expected a lhs expression");
                let rhs = children.next().expect("expected a rhs expression");
                (lhs, rhs)
            }
        }

        impl<N: TreeNode> AstNode<N> for $name<N> {
            fn can_cast(kind: SyntaxKind) -> bool {
                kind == SyntaxKind::$kind
            }

            fn cast(inner: N) -> Option<Self> {
                match inner.kind() {
                    SyntaxKind::$kind => Some(Self(inner)),
                    _ => None,
                }
            }

            fn inner(&self) -> &N {
                &self.0
            }
        }
    };
}

prefix_expression!(LogicalNotExpr, LogicalNotExprNode, "logical `not`");
prefix_expression!(NegationExpr, NegationExprNode, "negation");
infix_expression!(LogicalOrExpr, LogicalOrExprNode, "logical `or`");
infix_expression!(LogicalAndExpr, LogicalAndExprNode, "logical `and`");
infix_expression!(EqualityExpr, EqualityExprNode, "equality");
infix_expression!(InequalityExpr, InequalityExprNode, "inequality");
infix_expression!(LessExpr, LessExprNode, "less than");
infix_expression!(LessEqualExpr, LessEqualExprNode, "less than or equal to");
infix_expression!(GreaterExpr, GreaterExprNode, "greater than");
infix_expression!(
    GreaterEqualExpr,
    GreaterEqualExprNode,
    "greater than or equal to"
);
infix_expression!(AdditionExpr, AdditionExprNode, "addition");
infix_expression!(SubtractionExpr, SubtractionExprNode, "substitution");
infix_expression!(MultiplicationExpr, MultiplicationExprNode, "multiplication");
infix_expression!(DivisionExpr, DivisionExprNode, "division");
infix_expression!(ModuloExpr, ModuloExprNode, "modulo");
infix_expression!(ExponentiationExpr, ExponentiationExprNode, "exponentiation");

/// Represents a call expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallExpr<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> CallExpr<N> {
    /// Gets the call target expression.
    pub fn target(&self) -> Ident<N::Token> {
        self.token().expect("expected a target identifier")
    }

    /// Gets the call arguments.
    pub fn arguments(&self) -> impl Iterator<Item = Expr<N>> + use<'_, N> {
        Expr::children(&self.0)
    }
}

impl<N: TreeNode> AstNode<N> for CallExpr<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::CallExprNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::CallExprNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents an index expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexExpr<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> IndexExpr<N> {
    /// Gets the operand and the index expressions.
    ///
    /// The first is the operand expression.
    /// The second is the index expression.
    pub fn operands(&self) -> (Expr<N>, Expr<N>) {
        let mut children = Expr::children(&self.0);
        let operand = children.next().expect("expected an operand expression");
        let index = children.next().expect("expected an index expression");
        (operand, index)
    }
}

impl<N: TreeNode> AstNode<N> for IndexExpr<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::IndexExprNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::IndexExprNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents an access expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessExpr<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> AccessExpr<N> {
    /// Gets the operand and the name of the access.
    ///
    /// The first is the operand expression.
    /// The second is the member name.
    pub fn operands(&self) -> (Expr<N>, Ident<N::Token>) {
        let operand = Expr::child(&self.0).expect("expected an operand expression");
        let name = Ident::cast(self.0.last_token().expect("expected a last token"))
            .expect("expected an ident token");
        (operand, name)
    }
}

impl<N: TreeNode> AstNode<N> for AccessExpr<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::AccessExprNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::AccessExprNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

#[cfg(test)]
mod test {
    use approx::assert_relative_eq;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::Document;

    #[test]
    fn literal_booleans() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Boolean a = true
    Boolean b = false
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 2);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Boolean");
        assert_eq!(decls[0].name().text(), "a");
        assert!(decls[0].expr().unwrap_literal().unwrap_boolean().value());

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "Boolean");
        assert_eq!(decls[1].name().text(), "b");
        assert!(!decls[1].expr().unwrap_literal().unwrap_boolean().value());
    }

    #[test]
    fn literal_integer() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Int a = 0
    Int b = 1234
    Int c = 01234
    Int d = 0x1234
    Int e = 0XF
    Int f = 9223372036854775807
    Int g = 9223372036854775808
    Int h = 9223372036854775809
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 8);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Int");
        assert_eq!(decls[0].name().text(), "a");
        assert_eq!(
            decls[0]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            0
        );

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "Int");
        assert_eq!(decls[1].name().text(), "b");
        assert_eq!(
            decls[1]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            1234
        );

        // Third declaration
        assert_eq!(decls[2].ty().to_string(), "Int");
        assert_eq!(decls[2].name().text(), "c");
        assert_eq!(
            decls[2]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            668
        );

        // Fourth declaration
        assert_eq!(decls[3].ty().to_string(), "Int");
        assert_eq!(decls[3].name().text(), "d");
        assert_eq!(
            decls[3]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            4660
        );

        // Fifth declaration
        assert_eq!(decls[4].ty().to_string(), "Int");
        assert_eq!(decls[4].name().text(), "e");
        assert_eq!(
            decls[4]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            15
        );

        // Sixth declaration
        assert_eq!(decls[5].ty().to_string(), "Int");
        assert_eq!(decls[5].name().text(), "f");
        assert_eq!(
            decls[5]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            9223372036854775807
        );

        // Seventh declaration
        assert_eq!(decls[6].ty().to_string(), "Int");
        assert_eq!(decls[6].name().text(), "g");
        assert!(
            decls[6]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .is_none(),
        );

        // Eighth declaration
        assert_eq!(decls[7].ty().to_string(), "Int");
        assert_eq!(decls[7].name().text(), "h");
        assert!(
            decls[7]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .is_none()
        );
    }

    #[test]
    fn literal_float() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Float a = 0.
    Float b = 0.0
    Float c = 1234.1234
    Float d = 123e123
    Float e = 0.1234
    Float f = 10.
    Float g = .2
    Float h = 1234.1234e1234
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 8);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Float");
        assert_eq!(decls[0].name().text(), "a");
        assert_relative_eq!(
            decls[0]
                .expr()
                .unwrap_literal()
                .unwrap_float()
                .value()
                .unwrap(),
            0.0
        );

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "Float");
        assert_eq!(decls[1].name().text(), "b");
        assert_relative_eq!(
            decls[1]
                .expr()
                .unwrap_literal()
                .unwrap_float()
                .value()
                .unwrap(),
            0.0
        );

        // Third declaration
        assert_eq!(decls[2].ty().to_string(), "Float");
        assert_eq!(decls[2].name().text(), "c");
        assert_relative_eq!(
            decls[2]
                .expr()
                .unwrap_literal()
                .unwrap_float()
                .value()
                .unwrap(),
            1234.1234
        );

        // Fourth declaration
        assert_eq!(decls[3].ty().to_string(), "Float");
        assert_eq!(decls[3].name().text(), "d");
        assert_relative_eq!(
            decls[3]
                .expr()
                .unwrap_literal()
                .unwrap_float()
                .value()
                .unwrap(),
            123e+123
        );

        // Fifth declaration
        assert_eq!(decls[4].ty().to_string(), "Float");
        assert_eq!(decls[4].name().text(), "e");
        assert_relative_eq!(
            decls[4]
                .expr()
                .unwrap_literal()
                .unwrap_float()
                .value()
                .unwrap(),
            0.1234
        );

        // Sixth declaration
        assert_eq!(decls[5].ty().to_string(), "Float");
        assert_eq!(decls[5].name().text(), "f");
        assert_relative_eq!(
            decls[5]
                .expr()
                .unwrap_literal()
                .unwrap_float()
                .value()
                .unwrap(),
            10.0
        );

        // Seventh declaration
        assert_eq!(decls[6].ty().to_string(), "Float");
        assert_eq!(decls[6].name().text(), "g");
        assert_relative_eq!(
            decls[6]
                .expr()
                .unwrap_literal()
                .unwrap_float()
                .value()
                .unwrap(),
            0.2
        );

        // Eighth declaration
        assert_eq!(decls[7].ty().to_string(), "Float");
        assert_eq!(decls[7].name().text(), "h");
        assert!(
            decls[7]
                .expr()
                .unwrap_literal()
                .unwrap_float()
                .value()
                .is_none()
        );
    }

    #[test]
    fn literal_string() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    String a = "hello"
    String b = 'world'
    String c = "Hello, ${name}!"
    String d = 'String~{'ception'}!'
    String e = <<< this is
    a multiline \
    string!
    ${first}
    ${second}
    >>>
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 5);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "String");
        assert_eq!(decls[0].name().text(), "a");
        let s = decls[0].expr().unwrap_literal().unwrap_string();
        assert_eq!(s.kind(), LiteralStringKind::DoubleQuoted);
        assert_eq!(s.text().unwrap().text(), "hello");

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "String");
        assert_eq!(decls[1].name().text(), "b");
        let s = decls[1].expr().unwrap_literal().unwrap_string();
        assert_eq!(s.kind(), LiteralStringKind::SingleQuoted);
        assert_eq!(s.text().unwrap().text(), "world");

        // Third declaration
        assert_eq!(decls[2].ty().to_string(), "String");
        assert_eq!(decls[2].name().text(), "c");
        let s = decls[2].expr().unwrap_literal().unwrap_string();
        assert_eq!(s.kind(), LiteralStringKind::DoubleQuoted);
        let parts: Vec<_> = s.parts().collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].clone().unwrap_text().text(), "Hello, ");
        let placeholder = parts[1].clone().unwrap_placeholder();
        assert!(!placeholder.has_tilde());
        assert_eq!(placeholder.expr().unwrap_name_ref().name().text(), "name");
        assert_eq!(parts[2].clone().unwrap_text().text(), "!");

        // Fourth declaration
        assert_eq!(decls[3].ty().to_string(), "String");
        assert_eq!(decls[3].name().text(), "d");
        let s = decls[3].expr().unwrap_literal().unwrap_string();
        assert_eq!(s.kind(), LiteralStringKind::SingleQuoted);
        let parts: Vec<_> = s.parts().collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].clone().unwrap_text().text(), "String");
        let placeholder = parts[1].clone().unwrap_placeholder();
        assert!(placeholder.has_tilde());
        assert_eq!(
            placeholder
                .expr()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "ception"
        );
        assert_eq!(parts[2].clone().unwrap_text().text(), "!");

        // Fifth declaration
        assert_eq!(decls[4].ty().to_string(), "String");
        assert_eq!(decls[4].name().text(), "e");
        let s = decls[4].expr().unwrap_literal().unwrap_string();
        assert_eq!(s.kind(), LiteralStringKind::Multiline);
        let parts: Vec<_> = s.parts().collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(
            parts[0].clone().unwrap_text().text(),
            " this is\n    a multiline \\\n    string!\n    "
        );
        let placeholder = parts[1].clone().unwrap_placeholder();
        assert!(!placeholder.has_tilde());
        assert_eq!(placeholder.expr().unwrap_name_ref().name().text(), "first");
        assert_eq!(parts[2].clone().unwrap_text().text(), "\n    ");
        let placeholder = parts[3].clone().unwrap_placeholder();
        assert!(!placeholder.has_tilde());
        assert_eq!(placeholder.expr().unwrap_name_ref().name().text(), "second");
        assert_eq!(parts[4].clone().unwrap_text().text(), "\n    ");
    }

    #[test]
    fn literal_array() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Array[Int] a = [1, 2, 3]
    Array[String] b = ["hello", "world", "!"]
    Array[Array[Int]] c = [[1, 2, 3], [4, 5, 6], [7, 8, 9]]
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 3);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Array[Int]");
        assert_eq!(decls[0].name().text(), "a");
        let a = decls[0].expr().unwrap_literal().unwrap_array();
        let elements: Vec<_> = a.elements().collect();
        assert_eq!(elements.len(), 3);
        assert_eq!(
            elements[0]
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            1
        );
        assert_eq!(
            elements[1]
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            2
        );
        assert_eq!(
            elements[2]
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            3
        );

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "Array[String]");
        assert_eq!(decls[1].name().text(), "b");
        let a = decls[1].expr().unwrap_literal().unwrap_array();
        let elements: Vec<_> = a.elements().collect();
        assert_eq!(elements.len(), 3);
        assert_eq!(
            elements[0]
                .clone()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "hello"
        );
        assert_eq!(
            elements[1]
                .clone()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "world"
        );
        assert_eq!(
            elements[2]
                .clone()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "!"
        );

        // Third declaration
        assert_eq!(decls[2].ty().to_string(), "Array[Array[Int]]");
        assert_eq!(decls[2].name().text(), "c");
        let a = decls[2].expr().unwrap_literal().unwrap_array();
        let elements: Vec<_> = a.elements().collect();
        assert_eq!(elements.len(), 3);
        let sub: Vec<_> = elements[0]
            .clone()
            .unwrap_literal()
            .unwrap_array()
            .elements()
            .collect();
        assert_eq!(sub.len(), 3);
        assert_eq!(
            sub[0]
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            1
        );
        assert_eq!(
            sub[1]
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            2
        );
        assert_eq!(
            sub[2]
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            3
        );
        let sub: Vec<_> = elements[1]
            .clone()
            .unwrap_literal()
            .unwrap_array()
            .elements()
            .collect();
        assert_eq!(sub.len(), 3);
        assert_eq!(
            sub[0]
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            4
        );
        assert_eq!(
            sub[1]
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            5
        );
        assert_eq!(
            sub[2]
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            6
        );
        let sub: Vec<_> = elements[2]
            .clone()
            .unwrap_literal()
            .unwrap_array()
            .elements()
            .collect();
        assert_eq!(sub.len(), 3);
        assert_eq!(
            sub[0]
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            7
        );
        assert_eq!(
            sub[1]
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            8
        );
        assert_eq!(
            sub[2]
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            9
        );
    }

    #[test]
    fn literal_pair() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Pair[Int, Int] a = (1000, 0x1000)
    Pair[String, Int] b = ("0x1000", 1000)
    Array[Pair[Int, String]] c = [(1, "hello"), (2, 'world'), (3, "!")]
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 3);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Pair[Int, Int]");
        assert_eq!(decls[0].name().text(), "a");
        let p = decls[0].expr().unwrap_literal().unwrap_pair();
        let (left, right) = p.exprs();
        assert_eq!(
            left.clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            1000
        );
        assert_eq!(
            right
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            0x1000
        );

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "Pair[String, Int]");
        assert_eq!(decls[1].name().text(), "b");
        let p = decls[1].expr().unwrap_literal().unwrap_pair();
        let (left, right) = p.exprs();
        assert_eq!(
            left.clone()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "0x1000"
        );
        assert_eq!(
            right
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            1000
        );

        // Third declaration
        assert_eq!(decls[2].ty().to_string(), "Array[Pair[Int, String]]");
        assert_eq!(decls[2].name().text(), "c");
        let a = decls[2].expr().unwrap_literal().unwrap_array();
        let elements: Vec<_> = a.elements().collect();
        assert_eq!(elements.len(), 3);
        let p = elements[0].clone().unwrap_literal().unwrap_pair();
        let (left, right) = p.exprs();
        assert_eq!(
            left.clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            1
        );
        assert_eq!(
            right
                .clone()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "hello"
        );
        let p = elements[1].clone().unwrap_literal().unwrap_pair();
        let (left, right) = p.exprs();
        assert_eq!(
            left.clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            2
        );
        assert_eq!(
            right
                .clone()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "world"
        );
        let p = elements[2].clone().unwrap_literal().unwrap_pair();
        let (left, right) = p.exprs();
        assert_eq!(
            left.clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            3
        );
        assert_eq!(
            right
                .clone()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "!"
        );
    }

    #[test]
    fn literal_map() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Map[Int, Int] a = {}
    Map[String, String] b = { "foo": "bar", "bar": "baz" }
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 2);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Map[Int, Int]");
        assert_eq!(decls[0].name().text(), "a");
        let m = decls[0].expr().unwrap_literal().unwrap_map();
        let items: Vec<_> = m.items().collect();
        assert_eq!(items.len(), 0);

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "Map[String, String]");
        assert_eq!(decls[1].name().text(), "b");
        let m = decls[1].expr().unwrap_literal().unwrap_map();
        let items: Vec<_> = m.items().collect();
        assert_eq!(items.len(), 2);
        let (key, value) = items[0].key_value();
        assert_eq!(
            key.unwrap_literal().unwrap_string().text().unwrap().text(),
            "foo"
        );
        assert_eq!(
            value
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "bar"
        );
        let (key, value) = items[1].key_value();
        assert_eq!(
            key.unwrap_literal().unwrap_string().text().unwrap().text(),
            "bar"
        );
        assert_eq!(
            value
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "baz"
        );
    }

    #[test]
    fn literal_object() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Object a = object {}
    Object b = object { foo: "bar", bar: 1, baz: [1, 2, 3] }
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 2);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Object");
        assert_eq!(decls[0].name().text(), "a");
        let o = decls[0].expr().unwrap_literal().unwrap_object();
        let items: Vec<_> = o.items().collect();
        assert_eq!(items.len(), 0);

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "Object");
        assert_eq!(decls[1].name().text(), "b");
        let o = decls[1].expr().unwrap_literal().unwrap_object();
        let items: Vec<_> = o.items().collect();
        assert_eq!(items.len(), 3);
        let (name, value) = items[0].name_value();
        assert_eq!(name.text(), "foo");
        assert_eq!(
            value
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "bar"
        );
        let (name, value) = items[1].name_value();
        assert_eq!(name.text(), "bar");
        assert_eq!(value.unwrap_literal().unwrap_integer().value().unwrap(), 1);
        let (name, value) = items[2].name_value();
        assert_eq!(name.text(), "baz");
        let elements: Vec<_> = value.unwrap_literal().unwrap_array().elements().collect();
        assert_eq!(elements.len(), 3);
        assert_eq!(
            elements[0]
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            1
        );
        assert_eq!(
            elements[1]
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            2
        );
        assert_eq!(
            elements[2]
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            3
        );
    }

    #[test]
    fn literal_struct() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Foo a = Foo { foo: "bar" }
    Bar b = Bar { bar: 1, baz: [1, 2, 3] }
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 2);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Foo");
        assert_eq!(decls[0].name().text(), "a");
        let s = decls[0].expr().unwrap_literal().unwrap_struct();
        assert_eq!(s.name().text(), "Foo");
        let items: Vec<_> = s.items().collect();
        assert_eq!(items.len(), 1);
        let (name, value) = items[0].name_value();
        assert_eq!(name.text(), "foo");
        assert_eq!(
            value
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "bar"
        );

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "Bar");
        assert_eq!(decls[1].name().text(), "b");
        let s = decls[1].expr().unwrap_literal().unwrap_struct();
        assert_eq!(s.name().text(), "Bar");
        let items: Vec<_> = s.items().collect();
        assert_eq!(items.len(), 2);
        let (name, value) = items[0].name_value();
        assert_eq!(name.text(), "bar");
        assert_eq!(value.unwrap_literal().unwrap_integer().value().unwrap(), 1);
        let (name, value) = items[1].name_value();
        assert_eq!(name.text(), "baz");
        let elements: Vec<_> = value.unwrap_literal().unwrap_array().elements().collect();
        assert_eq!(elements.len(), 3);
        assert_eq!(
            elements[0]
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            1
        );
        assert_eq!(
            elements[1]
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            2
        );
        assert_eq!(
            elements[2]
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            3
        );
    }

    #[test]
    fn literal_none() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Int? a = None
    Boolean b = a == None
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 2);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Int?");
        assert_eq!(decls[0].name().text(), "a");
        decls[0].expr().unwrap_literal().unwrap_none();

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "Boolean");
        assert_eq!(decls[1].name().text(), "b");
        let (lhs, rhs) = decls[1].expr().unwrap_equality().operands();
        assert_eq!(lhs.unwrap_name_ref().name().text(), "a");
        rhs.unwrap_literal().unwrap_none();
    }

    #[test]
    fn literal_hints() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.2

task test {
    hints {
        foo: hints {
            bar: "bar",
            baz: "baz"
        }
        bar: "bar"
        baz: hints {
            a: 1,
            b: 10.0,
            c: {
                "foo": "bar",
            }
        }
    }
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task hints
        let hints = tasks[0].hints().expect("should have a hints section");
        let items: Vec<_> = hints.items().collect();
        assert_eq!(items.len(), 3);

        // First hints item
        assert_eq!(items[0].name().text(), "foo");
        let inner: Vec<_> = items[0]
            .expr()
            .unwrap_literal()
            .unwrap_hints()
            .items()
            .collect();
        assert_eq!(inner.len(), 2);
        assert_eq!(inner[0].name().text(), "bar");
        assert_eq!(
            inner[0]
                .expr()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "bar"
        );
        assert_eq!(inner[1].name().text(), "baz");
        assert_eq!(
            inner[1]
                .expr()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "baz"
        );

        // Second hints item
        assert_eq!(items[1].name().text(), "bar");
        assert_eq!(
            items[1]
                .expr()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "bar"
        );

        // Third hints item
        assert_eq!(items[2].name().text(), "baz");
        let inner: Vec<_> = items[2]
            .expr()
            .unwrap_literal()
            .unwrap_hints()
            .items()
            .collect();
        assert_eq!(inner.len(), 3);
        assert_eq!(inner[0].name().text(), "a");
        assert_eq!(
            inner[0]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            1
        );
        assert_eq!(inner[1].name().text(), "b");
        assert_relative_eq!(
            inner[1]
                .expr()
                .unwrap_literal()
                .unwrap_float()
                .value()
                .unwrap(),
            10.0
        );
        assert_eq!(inner[2].name().text(), "c");
        let map: Vec<_> = inner[2]
            .expr()
            .unwrap_literal()
            .unwrap_map()
            .items()
            .collect();
        assert_eq!(map.len(), 1);
        let (k, v) = map[0].key_value();
        assert_eq!(
            k.unwrap_literal().unwrap_string().text().unwrap().text(),
            "foo"
        );
        assert_eq!(
            v.unwrap_literal().unwrap_string().text().unwrap().text(),
            "bar"
        );
    }

    #[test]
    fn literal_input() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.2

task test {
    hints {
        inputs: input {
            a: hints {
                foo: "bar"
            },
            b.c.d: hints {
                bar: "baz"
            }
        }
    }
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task hints
        let hints = tasks[0].hints().expect("task should have hints section");
        let items: Vec<_> = hints.items().collect();
        assert_eq!(items.len(), 1);

        // First hints item
        assert_eq!(items[0].name().text(), "inputs");
        let input: Vec<_> = items[0]
            .expr()
            .unwrap_literal()
            .unwrap_input()
            .items()
            .collect();
        assert_eq!(input.len(), 2);
        assert_eq!(
            input[0]
                .names()
                .map(|i| i.text().to_string())
                .collect::<Vec<_>>(),
            ["a"]
        );
        let inner: Vec<_> = input[0]
            .expr()
            .unwrap_literal()
            .unwrap_hints()
            .items()
            .collect();
        assert_eq!(inner.len(), 1);
        assert_eq!(inner[0].name().text(), "foo");
        assert_eq!(
            inner[0]
                .expr()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "bar"
        );
        assert_eq!(
            input[1]
                .names()
                .map(|i| i.text().to_string())
                .collect::<Vec<_>>(),
            ["b", "c", "d"]
        );
        let inner: Vec<_> = input[1]
            .expr()
            .unwrap_literal()
            .unwrap_hints()
            .items()
            .collect();
        assert_eq!(inner.len(), 1);
        assert_eq!(inner[0].name().text(), "bar");
        assert_eq!(
            inner[0]
                .expr()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "baz"
        );
    }

    #[test]
    fn literal_output() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.2

task test {
    hints {
        outputs: output {
            a: hints {
                foo: "bar"
            },
            b.c.d: hints {
                bar: "baz"
            }
        }
    }
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task hints
        let hints = tasks[0].hints().expect("task should have a hints section");
        let items: Vec<_> = hints.items().collect();
        assert_eq!(items.len(), 1);

        // First hints item
        assert_eq!(items[0].name().text(), "outputs");
        let output: Vec<_> = items[0]
            .expr()
            .unwrap_literal()
            .unwrap_output()
            .items()
            .collect();
        assert_eq!(output.len(), 2);
        assert_eq!(
            output[0]
                .names()
                .map(|i| i.text().to_string())
                .collect::<Vec<_>>(),
            ["a"]
        );
        let inner: Vec<_> = output[0]
            .expr()
            .unwrap_literal()
            .unwrap_hints()
            .items()
            .collect();
        assert_eq!(inner.len(), 1);
        assert_eq!(inner[0].name().text(), "foo");
        assert_eq!(
            inner[0]
                .expr()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "bar"
        );
        assert_eq!(
            output[1]
                .names()
                .map(|i| i.text().to_string())
                .collect::<Vec<_>>(),
            ["b", "c", "d"]
        );
        let inner: Vec<_> = output[1]
            .expr()
            .unwrap_literal()
            .unwrap_hints()
            .items()
            .collect();
        assert_eq!(inner.len(), 1);
        assert_eq!(inner[0].name().text(), "bar");
        assert_eq!(
            inner[0]
                .expr()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "baz"
        );
    }

    #[test]
    fn name_ref() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Int a = 0
    Int b = a
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 2);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Int");
        assert_eq!(decls[0].name().text(), "a");
        assert_eq!(
            decls[0]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            0
        );

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "Int");
        assert_eq!(decls[1].name().text(), "b");
        assert_eq!(decls[1].expr().unwrap_name_ref().name().text(), "a");
    }

    #[test]
    fn parenthesized() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Int a = (0)
    Int b = (10 - (5 + 5))
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 2);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Int");
        assert_eq!(decls[0].name().text(), "a");
        assert_eq!(
            decls[0]
                .expr()
                .unwrap_parenthesized()
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            0
        );

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "Int");
        assert_eq!(decls[1].name().text(), "b");
        let (lhs, rhs) = decls[1]
            .expr()
            .unwrap_parenthesized()
            .expr()
            .unwrap_subtraction()
            .operands();
        assert_eq!(lhs.unwrap_literal().unwrap_integer().value().unwrap(), 10);
        let (lhs, rhs) = rhs
            .unwrap_parenthesized()
            .expr()
            .unwrap_addition()
            .operands();
        assert_eq!(lhs.unwrap_literal().unwrap_integer().value().unwrap(), 5);
        assert_eq!(rhs.unwrap_literal().unwrap_integer().value().unwrap(), 5);
    }

    #[test]
    fn if_expr() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Int a = if true then 1 else 0
    String b = if a > 0 then "yes" else "no"
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 2);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Int");
        assert_eq!(decls[0].name().text(), "a");
        let (c, t, f) = decls[0].expr().unwrap_if().exprs();
        assert!(c.unwrap_literal().unwrap_boolean().value());
        assert_eq!(t.unwrap_literal().unwrap_integer().value().unwrap(), 1);
        assert_eq!(f.unwrap_literal().unwrap_integer().value().unwrap(), 0);

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "String");
        assert_eq!(decls[1].name().text(), "b");
        let (c, t, f) = decls[1].expr().unwrap_if().exprs();
        let (lhs, rhs) = c.unwrap_greater().operands();
        assert_eq!(lhs.unwrap_name_ref().name().text(), "a");
        assert_eq!(rhs.unwrap_literal().unwrap_integer().value().unwrap(), 0);
        assert_eq!(
            t.unwrap_literal().unwrap_string().text().unwrap().text(),
            "yes"
        );
        assert_eq!(
            f.unwrap_literal().unwrap_string().text().unwrap().text(),
            "no"
        );
    }

    #[test]
    fn logical_not() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Boolean a = !true
    Boolean b = !!!a
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 2);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Boolean");
        assert_eq!(decls[0].name().text(), "a");
        assert!(
            decls[0]
                .expr()
                .unwrap_logical_not()
                .operand()
                .unwrap_literal()
                .unwrap_boolean()
                .value()
        );

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "Boolean");
        assert_eq!(decls[1].name().text(), "b");
        assert_eq!(
            decls[1]
                .expr()
                .unwrap_logical_not()
                .operand()
                .unwrap_logical_not()
                .operand()
                .unwrap_logical_not()
                .operand()
                .unwrap_name_ref()
                .name()
                .text(),
            "a"
        );
    }

    #[test]
    fn negation() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Int a = -1
    Int b = ---a
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 2);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Int");
        assert_eq!(decls[0].name().text(), "a");
        assert_eq!(
            decls[0]
                .expr()
                .unwrap_negation()
                .operand()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            1
        );

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "Int");
        assert_eq!(decls[1].name().text(), "b");
        assert_eq!(
            decls[1]
                .expr()
                .unwrap_negation()
                .operand()
                .unwrap_negation()
                .operand()
                .unwrap_negation()
                .operand()
                .unwrap_name_ref()
                .name()
                .text(),
            "a"
        );
    }

    #[test]
    fn logical_or() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Boolean a = false
    Boolean b = true
    Boolean c = a || b
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 3);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Boolean");
        assert_eq!(decls[0].name().text(), "a");
        assert!(!decls[0].expr().unwrap_literal().unwrap_boolean().value());

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "Boolean");
        assert_eq!(decls[1].name().text(), "b");
        assert!(decls[1].expr().unwrap_literal().unwrap_boolean().value());

        // Third declaration
        assert_eq!(decls[2].ty().to_string(), "Boolean");
        assert_eq!(decls[2].name().text(), "c");
        let (lhs, rhs) = decls[2].expr().unwrap_logical_or().operands();
        assert_eq!(lhs.unwrap_name_ref().name().text(), "a");
        assert_eq!(rhs.unwrap_name_ref().name().text(), "b");
    }

    #[test]
    fn logical_and() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Boolean a = true
    Boolean b = true
    Boolean c = a && b
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 3);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Boolean");
        assert_eq!(decls[0].name().text(), "a");
        assert!(decls[0].expr().unwrap_literal().unwrap_boolean().value());

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "Boolean");
        assert_eq!(decls[1].name().text(), "b");
        assert!(decls[1].expr().unwrap_literal().unwrap_boolean().value());

        // Third declaration
        assert_eq!(decls[2].ty().to_string(), "Boolean");
        assert_eq!(decls[2].name().text(), "c");
        let (lhs, rhs) = decls[2].expr().unwrap_logical_and().operands();
        assert_eq!(lhs.unwrap_name_ref().name().text(), "a");
        assert_eq!(rhs.unwrap_name_ref().name().text(), "b");
    }

    #[test]
    fn equality() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Boolean a = true
    Boolean b = false
    Boolean c = a == b
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 3);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Boolean");
        assert_eq!(decls[0].name().text(), "a");
        assert!(decls[0].expr().unwrap_literal().unwrap_boolean().value());

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "Boolean");
        assert_eq!(decls[1].name().text(), "b");
        assert!(!decls[1].expr().unwrap_literal().unwrap_boolean().value());

        // Third declaration
        assert_eq!(decls[2].ty().to_string(), "Boolean");
        assert_eq!(decls[2].name().text(), "c");
        let (lhs, rhs) = decls[2].expr().unwrap_equality().operands();
        assert_eq!(lhs.unwrap_name_ref().name().text(), "a");
        assert_eq!(rhs.unwrap_name_ref().name().text(), "b");
    }

    #[test]
    fn inequality() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Boolean a = true
    Boolean b = false
    Boolean c = a != b
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 3);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Boolean");
        assert_eq!(decls[0].name().text(), "a");
        assert!(decls[0].expr().unwrap_literal().unwrap_boolean().value());

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "Boolean");
        assert_eq!(decls[1].name().text(), "b");
        assert!(!decls[1].expr().unwrap_literal().unwrap_boolean().value());

        // Third declaration
        assert_eq!(decls[2].ty().to_string(), "Boolean");
        assert_eq!(decls[2].name().text(), "c");
        let (lhs, rhs) = decls[2].expr().unwrap_inequality().operands();
        assert_eq!(lhs.unwrap_name_ref().name().text(), "a");
        assert_eq!(rhs.unwrap_name_ref().name().text(), "b");
    }

    #[test]
    fn less() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Int a = 1
    Int b = 2
    Boolean c = a < b
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 3);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Int");
        assert_eq!(decls[0].name().text(), "a");
        assert_eq!(
            decls[0]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            1
        );

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "Int");
        assert_eq!(decls[1].name().text(), "b");
        assert_eq!(
            decls[1]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            2
        );

        // Third declaration
        assert_eq!(decls[2].ty().to_string(), "Boolean");
        assert_eq!(decls[2].name().text(), "c");
        let (lhs, rhs) = decls[2].expr().unwrap_less().operands();
        assert_eq!(lhs.unwrap_name_ref().name().text(), "a");
        assert_eq!(rhs.unwrap_name_ref().name().text(), "b");
    }

    #[test]
    fn less_equal() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Int a = 1
    Int b = 2
    Boolean c = a <= b
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 3);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Int");
        assert_eq!(decls[0].name().text(), "a");
        assert_eq!(
            decls[0]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            1
        );

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "Int");
        assert_eq!(decls[1].name().text(), "b");
        assert_eq!(
            decls[1]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            2
        );

        // Third declaration
        assert_eq!(decls[2].ty().to_string(), "Boolean");
        assert_eq!(decls[2].name().text(), "c");
        let (lhs, rhs) = decls[2].expr().unwrap_less_equal().operands();
        assert_eq!(lhs.unwrap_name_ref().name().text(), "a");
        assert_eq!(rhs.unwrap_name_ref().name().text(), "b");
    }

    #[test]
    fn greater() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Int a = 1
    Int b = 2
    Boolean c = a > b
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 3);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Int");
        assert_eq!(decls[0].name().text(), "a");
        assert_eq!(
            decls[0]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            1
        );

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "Int");
        assert_eq!(decls[1].name().text(), "b");
        assert_eq!(
            decls[1]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            2
        );

        // Third declaration
        assert_eq!(decls[2].ty().to_string(), "Boolean");
        assert_eq!(decls[2].name().text(), "c");
        let (lhs, rhs) = decls[2].expr().unwrap_greater().operands();
        assert_eq!(lhs.unwrap_name_ref().name().text(), "a");
        assert_eq!(rhs.unwrap_name_ref().name().text(), "b");
    }

    #[test]
    fn greater_equal() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Int a = 1
    Int b = 2
    Boolean c = a >= b
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 3);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Int");
        assert_eq!(decls[0].name().text(), "a");
        assert_eq!(
            decls[0]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            1
        );

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "Int");
        assert_eq!(decls[1].name().text(), "b");
        assert_eq!(
            decls[1]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            2
        );

        // Third declaration
        assert_eq!(decls[2].ty().to_string(), "Boolean");
        assert_eq!(decls[2].name().text(), "c");
        let (lhs, rhs) = decls[2].expr().unwrap_greater_equal().operands();
        assert_eq!(lhs.unwrap_name_ref().name().text(), "a");
        assert_eq!(rhs.unwrap_name_ref().name().text(), "b");
    }

    #[test]
    fn addition() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Int a = 1
    Int b = 2
    Int c = a + b
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 3);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Int");
        assert_eq!(decls[0].name().text(), "a");
        assert_eq!(
            decls[0]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            1
        );

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "Int");
        assert_eq!(decls[1].name().text(), "b");
        assert_eq!(
            decls[1]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            2
        );

        // Third declaration
        assert_eq!(decls[2].ty().to_string(), "Int");
        assert_eq!(decls[2].name().text(), "c");
        let (lhs, rhs) = decls[2].expr().unwrap_addition().operands();
        assert_eq!(lhs.unwrap_name_ref().name().text(), "a");
        assert_eq!(rhs.unwrap_name_ref().name().text(), "b");
    }

    #[test]
    fn subtraction() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Int a = 1
    Int b = 2
    Int c = a - b
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 3);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Int");
        assert_eq!(decls[0].name().text(), "a");
        assert_eq!(
            decls[0]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            1
        );

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "Int");
        assert_eq!(decls[1].name().text(), "b");
        assert_eq!(
            decls[1]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            2
        );

        // Third declaration
        assert_eq!(decls[2].ty().to_string(), "Int");
        assert_eq!(decls[2].name().text(), "c");
        let (lhs, rhs) = decls[2].expr().unwrap_subtraction().operands();
        assert_eq!(lhs.unwrap_name_ref().name().text(), "a");
        assert_eq!(rhs.unwrap_name_ref().name().text(), "b");
    }

    #[test]
    fn multiplication() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Int a = 1
    Int b = 2
    Int c = a * b
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 3);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Int");
        assert_eq!(decls[0].name().text(), "a");
        assert_eq!(
            decls[0]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            1
        );

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "Int");
        assert_eq!(decls[1].name().text(), "b");
        assert_eq!(
            decls[1]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            2
        );

        // Third declaration
        assert_eq!(decls[2].ty().to_string(), "Int");
        assert_eq!(decls[2].name().text(), "c");
        let (lhs, rhs) = decls[2].expr().unwrap_multiplication().operands();
        assert_eq!(lhs.unwrap_name_ref().name().text(), "a");
        assert_eq!(rhs.unwrap_name_ref().name().text(), "b");
    }

    #[test]
    fn division() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Int a = 1
    Int b = 2
    Int c = a / b
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 3);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Int");
        assert_eq!(decls[0].name().text(), "a");
        assert_eq!(
            decls[0]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            1
        );

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "Int");
        assert_eq!(decls[1].name().text(), "b");
        assert_eq!(
            decls[1]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            2
        );

        // Third declaration
        assert_eq!(decls[2].ty().to_string(), "Int");
        assert_eq!(decls[2].name().text(), "c");
        let (lhs, rhs) = decls[2].expr().unwrap_division().operands();
        assert_eq!(lhs.unwrap_name_ref().name().text(), "a");
        assert_eq!(rhs.unwrap_name_ref().name().text(), "b");
    }

    #[test]
    fn modulo() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Int a = 1
    Int b = 2
    Int c = a % b
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 3);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Int");
        assert_eq!(decls[0].name().text(), "a");
        assert_eq!(
            decls[0]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            1
        );

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "Int");
        assert_eq!(decls[1].name().text(), "b");
        assert_eq!(
            decls[1]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            2
        );

        // Third declaration
        assert_eq!(decls[2].ty().to_string(), "Int");
        assert_eq!(decls[2].name().text(), "c");
        let (lhs, rhs) = decls[2].expr().unwrap_modulo().operands();
        assert_eq!(lhs.unwrap_name_ref().name().text(), "a");
        assert_eq!(rhs.unwrap_name_ref().name().text(), "b");
    }

    #[test]
    fn exponentiation() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.2

task test {
    Int a = 2
    Int b = 8
    Int c = a ** b
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 3);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Int");
        assert_eq!(decls[0].name().text(), "a");
        assert_eq!(
            decls[0]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            2
        );

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "Int");
        assert_eq!(decls[1].name().text(), "b");
        assert_eq!(
            decls[1]
                .expr()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            8
        );

        // Third declaration
        assert_eq!(decls[2].ty().to_string(), "Int");
        assert_eq!(decls[2].name().text(), "c");
        let (lhs, rhs) = decls[2].expr().unwrap_exponentiation().operands();
        assert_eq!(lhs.unwrap_name_ref().name().text(), "a");
        assert_eq!(rhs.unwrap_name_ref().name().text(), "b");
    }

    #[test]
    fn call() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Array[Int] a = [1, 2, 3]
    String b = sep(" ", a)
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 2);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Array[Int]");
        assert_eq!(decls[0].name().text(), "a");
        let elements: Vec<_> = decls[0]
            .expr()
            .unwrap_literal()
            .unwrap_array()
            .elements()
            .collect();
        assert_eq!(elements.len(), 3);
        assert_eq!(
            elements[0]
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            1
        );
        assert_eq!(
            elements[1]
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            2
        );
        assert_eq!(
            elements[2]
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            3
        );

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "String");
        assert_eq!(decls[1].name().text(), "b");
        let call = decls[1].expr().unwrap_call();
        assert_eq!(call.target().text(), "sep");
        let args: Vec<_> = call.arguments().collect();
        assert_eq!(args.len(), 2);
        assert_eq!(
            args[0]
                .clone()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            " "
        );
        assert_eq!(args[1].clone().unwrap_name_ref().name().text(), "a");
    }

    #[test]
    fn index() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Array[Int] a = [1, 2, 3]
    Int b = a[1]
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 2);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Array[Int]");
        assert_eq!(decls[0].name().text(), "a");
        let elements: Vec<_> = decls[0]
            .expr()
            .unwrap_literal()
            .unwrap_array()
            .elements()
            .collect();
        assert_eq!(elements.len(), 3);
        assert_eq!(
            elements[0]
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            1
        );
        assert_eq!(
            elements[1]
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            2
        );
        assert_eq!(
            elements[2]
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            3
        );

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "Int");
        assert_eq!(decls[1].name().text(), "b");
        let (expr, index) = decls[1].expr().unwrap_index().operands();
        assert_eq!(expr.unwrap_name_ref().name().text(), "a");
        assert_eq!(index.unwrap_literal().unwrap_integer().value().unwrap(), 1);
    }

    #[test]
    fn access() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    Object a = object { foo: "bar" }
    String b = a.foo
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 2);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "Object");
        assert_eq!(decls[0].name().text(), "a");
        let items: Vec<_> = decls[0]
            .expr()
            .unwrap_literal()
            .unwrap_object()
            .items()
            .collect();
        assert_eq!(items.len(), 1);
        let (name, value) = items[0].name_value();
        assert_eq!(name.text(), "foo");
        assert_eq!(
            value
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "bar"
        );

        // Second declaration
        assert_eq!(decls[1].ty().to_string(), "String");
        assert_eq!(decls[1].name().text(), "b");
        let (expr, index) = decls[1].expr().unwrap_access().operands();
        assert_eq!(expr.unwrap_name_ref().name().text(), "a");
        assert_eq!(index.text(), "foo");
    }

    #[test]
    fn strip_whitespace_on_single_line_string() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

task test {
    String a = "  foo  "
}"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");

        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);

        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 1);

        let expr = decls[0].expr().unwrap_literal().unwrap_string();
        assert_eq!(expr.text().unwrap().text(), "  foo  ");

        let stripped = expr.strip_whitespace();
        assert!(stripped.is_none());
    }

    #[test]
    fn strip_whitespace_on_multi_line_string_no_interpolation() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.2

task test {
    # all of these strings evaluate to "hello  world"
    String hw1 = <<<hello  world>>>
    String hw2 = <<<   hello  world   >>>
    String hw3 = <<<   
        hello  world>>>
    String hw4 = <<<   
        hello  world
        >>>
    String hw5 = <<<   
        hello  world
    >>>
    # The line continuation causes the newline and all whitespace preceding 'world' to be 
    # removed - to put two spaces between 'hello' and world' we need to put them before 
    # the line continuation.
    String hw6 = <<<
        hello  \
            world
    >>>
}"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");

        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);

        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 6);

        let expr = decls[0].expr().unwrap_literal().unwrap_string();
        let stripped = expr.strip_whitespace().unwrap();
        assert_eq!(stripped.len(), 1);
        match &stripped[0] {
            StrippedStringPart::Text(text) => assert_eq!(text.as_str(), "hello  world"),
            _ => panic!("expected text part"),
        }

        let expr = decls[1].expr().unwrap_literal().unwrap_string();
        let stripped = expr.strip_whitespace().unwrap();
        assert_eq!(stripped.len(), 1);
        match &stripped[0] {
            StrippedStringPart::Text(text) => assert_eq!(text.as_str(), "hello  world"),
            _ => panic!("expected text part"),
        }

        let expr = decls[2].expr().unwrap_literal().unwrap_string();
        let stripped = expr.strip_whitespace().unwrap();
        assert_eq!(stripped.len(), 1);
        match &stripped[0] {
            StrippedStringPart::Text(text) => assert_eq!(text.as_str(), "hello  world"),
            _ => panic!("expected text part"),
        }

        let expr = decls[3].expr().unwrap_literal().unwrap_string();
        let stripped = expr.strip_whitespace().unwrap();
        assert_eq!(stripped.len(), 1);
        match &stripped[0] {
            StrippedStringPart::Text(text) => assert_eq!(text.as_str(), "hello  world"),
            _ => panic!("expected text part"),
        }

        let expr = decls[4].expr().unwrap_literal().unwrap_string();
        let stripped = expr.strip_whitespace().unwrap();
        assert_eq!(stripped.len(), 1);
        match &stripped[0] {
            StrippedStringPart::Text(text) => assert_eq!(text.as_str(), "hello  world"),
            _ => panic!("expected text part"),
        }

        let expr = decls[5].expr().unwrap_literal().unwrap_string();
        let stripped = expr.strip_whitespace().unwrap();
        assert_eq!(stripped.len(), 1);
        match &stripped[0] {
            StrippedStringPart::Text(text) => assert_eq!(text.as_str(), "hello  world"),
            _ => panic!("expected text part"),
        }
    }

    #[test]
    fn strip_whitespace_on_multi_line_string_with_interpolation() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.2

task test {
    String hw1 = <<<
        hello  ${"world"}
    >>>
    String hw2 = <<<
        hello  ${
            "world"
        }
        my name
        is \
            Jerry\
    !
    >>>
}"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");

        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);

        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 2);

        let expr = decls[0].expr().unwrap_literal().unwrap_string();
        let stripped = expr.strip_whitespace().unwrap();
        assert_eq!(stripped.len(), 3);
        match &stripped[0] {
            StrippedStringPart::Text(text) => assert_eq!(text.as_str(), "hello  "),
            _ => panic!("expected text part"),
        }
        match &stripped[1] {
            StrippedStringPart::Placeholder(_) => {}
            _ => panic!("expected interpolated part"),
        }
        match &stripped[2] {
            StrippedStringPart::Text(text) => assert_eq!(text.as_str(), ""),
            _ => panic!("expected text part"),
        }

        let expr = decls[1].expr().unwrap_literal().unwrap_string();
        let stripped = expr.strip_whitespace().unwrap();
        assert_eq!(stripped.len(), 3);
        match &stripped[0] {
            StrippedStringPart::Text(text) => assert_eq!(text.as_str(), "hello  "),
            _ => panic!("expected text part"),
        }
        match &stripped[1] {
            StrippedStringPart::Placeholder(_) => {}
            _ => panic!("expected interpolated part"),
        }
        match &stripped[2] {
            StrippedStringPart::Text(text) => assert_eq!(text.as_str(), "\nmy name\nis Jerry!"),
            _ => panic!("expected text part"),
        }
    }

    #[test]
    fn remove_multiple_line_continuations() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.2

task test {
    String hw = <<<
    hello world \
    \
    \
    my name is Jeff.
    >>>
}"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");

        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);

        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 1);

        let expr = decls[0].expr().unwrap_literal().unwrap_string();
        let stripped = expr.strip_whitespace().unwrap();
        assert_eq!(stripped.len(), 1);
        match &stripped[0] {
            StrippedStringPart::Text(text) => {
                assert_eq!(text.as_str(), "hello world my name is Jeff.")
            }
            _ => panic!("expected text part"),
        }
    }

    #[test]
    fn strip_whitespace_with_content_on_first_line() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.2

task test {
    String hw = <<<    hello world
    my name is Jeff.
    >>>
}"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");

        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);

        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 1);

        let expr = decls[0].expr().unwrap_literal().unwrap_string();
        let stripped = expr.strip_whitespace().unwrap();
        assert_eq!(stripped.len(), 1);
        match &stripped[0] {
            StrippedStringPart::Text(text) => {
                assert_eq!(text.as_str(), "hello world\n    my name is Jeff.")
            }
            _ => panic!("expected text part"),
        }
    }

    #[test]
    fn whitespace_stripping_on_windows() {
        let (document, diagnostics) = Document::parse(
            "version 1.2\r\ntask test {\r\n    String s = <<<\r\n        hello\r\n    >>>\r\n}\r\n",
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");

        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);

        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 1);

        let expr = decls[0].expr().unwrap_literal().unwrap_string();
        let stripped = expr.strip_whitespace().unwrap();
        assert_eq!(stripped.len(), 1);
        match &stripped[0] {
            StrippedStringPart::Text(text) => {
                assert_eq!(text.as_str(), "hello")
            }
            _ => panic!("expected text part"),
        }
    }
}
