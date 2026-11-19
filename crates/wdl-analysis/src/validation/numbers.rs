//! Validation of number literals in an AST.

use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::v1::Expr;
use wdl_ast::v1::LiteralExpr;
use wdl_ast::v1::Minus;
use wdl_ast::version::V1;

use crate::Diagnostics;
use crate::VisitReason;
use crate::Visitor;
use crate::document::Document;

/// Creates an "integer not in range" diagnostic
fn integer_not_in_range(span: Span) -> Diagnostic {
    Diagnostic::error(format!(
        "literal integer exceeds the range for a 64-bit signed integer ({min}..={max})",
        min = i64::MIN,
        max = i64::MAX,
    ))
    .with_label("this literal integer is not in range", span)
}

/// Creates a "leading zero" diagnostic for a literal integer.
fn integer_leading_zero(text: &str, span: Span) -> Diagnostic {
    let digits = text.trim_start_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    Diagnostic::error("literal integer has a leading zero")
        .with_label("leading zeros are not permitted in literal integers", span)
        .with_fix(format!(
            "use `0o{digits}` for an octal integer or `{digits}` for a decimal integer"
        ))
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
/// Ensures that numbers are within their respective ranges and, starting with
/// WDL 1.1, that literal integers do not have leading zeros.
#[derive(Debug, Default)]
pub struct NumberVisitor {
    /// Stores the supported version of the WDL document we're visiting.
    version: Option<SupportedVersion>,
    /// Stores the start of a negation expression.
    negation_start: Option<usize>,
}

impl Visitor for NumberVisitor {
    fn reset(&mut self) {
        *self = Default::default();
    }

    fn document(
        &mut self,
        _: &mut Diagnostics,
        reason: VisitReason,
        _: &Document,
        version: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        self.version = Some(version);
    }

    fn expr(&mut self, diagnostics: &mut Diagnostics, reason: VisitReason, expr: &Expr) {
        if reason == VisitReason::Exit {
            self.negation_start = None;
            return;
        }

        // Check for either a literal integer, literal float, or a negation with
        // an immediate literal integer or literal float operand.
        //
        // In the case of floats, we can simply check if `value` returns
        // `is_none`, which will indicate that the literal is not in
        // range.
        //
        // For integers, we need to call the `negate` method if the literal is
        // part of a negation expression; otherwise, we use `value`.
        //
        // If a value is not in range and an operand to a negation expression,
        // we start the error span at the minus token.
        match expr {
            Expr::Literal(LiteralExpr::Integer(i)) => {
                // Legacy octal literals (e.g. `0755`) are only permitted in WDL
                // 1.0
                let token = i.integer();
                let text = token.text();
                if self
                    .version
                    .is_some_and(|v| v >= SupportedVersion::V1(V1::One))
                    && text.starts_with('0')
                    && text.len() > 1
                    && text.as_bytes()[1].is_ascii_digit()
                {
                    diagnostics.add(integer_leading_zero(text, token.span()));
                    return;
                }

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

                diagnostics.add(integer_not_in_range(span));
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

                diagnostics.add(float_not_in_range(span));
            }
            Expr::Negation(negation) => {
                // Check to see if the very next expression is a literal integer
                // or float
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
