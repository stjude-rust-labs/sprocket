//! Validation of number literals in an AST.

use crate::AstNode;
use crate::AstToken;
use crate::Diagnostic;
use crate::Diagnostics;
use crate::Document;
use crate::Span;
use crate::SupportedVersion;
use crate::VisitReason;
use crate::Visitor;
use crate::v1::Expr;
use crate::v1::LiteralExpr;
use crate::v1::Minus;

/// Creates an "integer not in range" diagnostic
fn integer_not_in_range(span: Span) -> Diagnostic {
    Diagnostic::error(format!(
        "literal integer exceeds the range for a 64-bit signed integer ({min}..={max})",
        min = i64::MIN,
        max = i64::MAX,
    ))
    .with_label("this literal integer is not in range", span)
}

/// Creates a "float not in range" diagnostic
fn float_not_in_range(span: Span) -> Diagnostic {
    Diagnostic::error(format!(
        "literal float exceeds the range for a 64-bit float ({min:+e}..={max:+e})",
        min = f64::MIN,
        max = f64::MAX,
    ))
    .with_label("this literal float is not in range", span)
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

    fn document(
        &mut self,
        _: &mut Self::State,
        reason: VisitReason,
        _: &Document,
        _: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Reset the visitor upon document entry
        *self = Default::default();
    }

    fn expr(&mut self, state: &mut Self::State, reason: VisitReason, expr: &Expr) {
        if reason == VisitReason::Exit {
            self.negation_start = None;
            return;
        }

        // Check for either a literal integer, literal float, or a negation with an
        // immediate literal integer or literal float operand.
        //
        // In the case of floats, we can simply check if `value` returns `is_none`,
        // which will indicate that the literal is not in range.
        //
        // For integers, we need to call the `negate` method if the literal is part
        // of a negation expression; otherwise, we use `value`.
        //
        // If a value is not in range and an operand to a negation expression, we start
        // the error span at the minus token.
        match expr {
            Expr::Literal(LiteralExpr::Integer(i)) => {
                let in_range = if self.negation_start.is_some() {
                    i.negate().is_some()
                } else {
                    i.value().is_some()
                };

                if in_range {
                    return;
                }

                let start = self
                    .negation_start
                    .or_else(|| i.minus().map(|t| t.span().start()));
                let span = i.integer().span();
                let span = match start {
                    Some(start) => Span::new(start, span.end() - start),
                    None => span,
                };

                state.add(integer_not_in_range(span));
            }
            Expr::Literal(LiteralExpr::Float(f)) => {
                if f.value().is_some() {
                    // Value is in range
                    return;
                }

                let start = self
                    .negation_start
                    .or_else(|| f.minus().map(|t| t.span().start()));
                let span = f.float().span();
                let span = match start {
                    Some(start) => Span::new(start, span.end() - start),
                    None => span,
                };

                state.add(float_not_in_range(span));
            }
            Expr::Negation(negation) => {
                // Check to see if the very next expression is a literal integer or float
                if matches!(
                    negation.operand(),
                    Expr::Literal(LiteralExpr::Integer(_)) | Expr::Literal(LiteralExpr::Float(_))
                ) {
                    self.negation_start = Some(
                        negation
                            .token::<Minus<_>>()
                            .expect("should have minus token")
                            .span()
                            .start(),
                    );
                }
            }
            _ => {}
        }
    }
}
