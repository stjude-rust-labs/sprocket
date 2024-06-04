//! Validation of number literals in a V1 AST.

use miette::Diagnostic;
use miette::SourceSpan;
use rowan::ast::AstNode;
use wdl_grammar::experimental::tree::SyntaxKind;

use crate::experimental::to_source_span;
use crate::experimental::v1::Expr;
use crate::experimental::v1::LiteralExpr;
use crate::experimental::v1::Visitor;
use crate::experimental::AstToken;
use crate::experimental::Diagnostics;
use crate::experimental::VisitReason;

/// Represents a number validation error.
#[derive(thiserror::Error, Diagnostic, Debug, Clone, PartialEq, Eq)]
enum Error {
    /// An integer value is out of bounds.
    #[error("literal integer exceeds the range for a 64-bit signed integer ({min}..={max})", min = i64::MIN, max = i64::MAX)]
    IntOutOfBounds {
        /// The span of the invalid literal integer.
        #[label(primary, "this literal integer is out of range")]
        span: SourceSpan,
    },
    /// A float value is out of bounds.
    #[error("literal float exceeds the range for a 64-bit float ({min:+e}..={max:+e})", min = f64::MIN, max = f64::MAX)]
    FloatOutOfBounds {
        /// The span of the invalid literal float.
        #[label(primary, "this literal float is out of range")]
        span: SourceSpan,
    },
}

/// A visitor of numbers within an AST.
///
/// Ensures that numbers are within their respective ranges.
#[derive(Debug, Default)]
pub struct NumberVisitor {
    /// Stores the start of a negation expression.
    negation_start: Option<usize>,
}

impl Visitor for NumberVisitor {
    type State = Diagnostics;

    fn expr(&mut self, state: &mut Self::State, reason: VisitReason, expr: &Expr) {
        if reason == VisitReason::Exit {
            self.negation_start = None;
            return;
        }

        // Check for either a literal integer, literal float, or a negation with an
        // immediate literal integer or literal float operand.
        //
        // In the case of floats, we can simply check if `value` returns `is_none`,
        // which will indicate that the literal is out of range.
        //
        // For integers, we check to see if the value is 9223372036854775808
        // (0x8000000000000000), which is normally out of range for a signed
        // 64-bit integer. However, if it is an immediate operand of a negation
        // expression, we accept the value, as it's actually -9223372036854775808.
        //
        // If a value is out of range and an operand to a negation expression, we start
        // the error span at the minus token.
        match expr {
            Expr::Literal(LiteralExpr::Integer(i)) => match i.value() {
                Some(0x8000000000000000) if self.negation_start.is_some() => {
                    // Value is in range for negation
                }
                Some(0x8000000000000000) | None => {
                    // Value is out of range
                    let range = i.token().syntax().text_range();
                    let span = match self.negation_start {
                        Some(start) => {
                            SourceSpan::new(start.into(), usize::from(range.end()) - start)
                        }
                        None => to_source_span(range),
                    };

                    state.add(Error::IntOutOfBounds { span });
                }
                Some(_) => {
                    // Value is in range
                }
            },
            Expr::Literal(LiteralExpr::Float(f)) => {
                if f.value().is_none() {
                    // Value is out of range
                    let range = f.token().syntax().text_range();
                    let span = match self.negation_start {
                        Some(start) => {
                            SourceSpan::new(start.into(), usize::from(range.end()) - start)
                        }
                        None => to_source_span(range),
                    };

                    state.add(Error::FloatOutOfBounds { span });
                }
            }
            Expr::Negation(negation) => {
                // Check to see if the very next expression is a literal integer or float
                if matches!(
                    negation.operand(),
                    Expr::Literal(LiteralExpr::Integer(_)) | Expr::Literal(LiteralExpr::Float(_))
                ) {
                    self.negation_start = Some(
                        negation
                            .syntax()
                            .children_with_tokens()
                            .find(|c| c.kind() == SyntaxKind::Minus)
                            .expect("should have minus token")
                            .text_range()
                            .start()
                            .into(),
                    );
                }
            }
            _ => {}
        }
    }
}
