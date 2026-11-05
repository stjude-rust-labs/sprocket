//! Implementation of evaluation for input and output validation blocks.

use wdl_analysis::diagnostics::Io;
use wdl_analysis::types::PrimitiveType;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::v1::Expr;
use wdl_ast::v1::ValidationBlock;

use super::ExprEvaluator;
use crate::Coercible;
use crate::EvaluationContext;
use crate::diagnostics::ValidationFailure;
use crate::diagnostics::validation_failed;
use crate::tree::SyntaxNode;

/// Evaluates the clauses of an input or output validation block.
///
/// Every clause is evaluated in source order; a clause's message is only
/// evaluated when its condition evaluates to `false`.
///
/// An evaluation error for a clause's condition or message is recorded as a
/// failure for that clause and evaluation continues with the next clause.
///
/// Returns an error diagnostic reporting every failure, in source order, if
/// any clause failed.
pub(crate) async fn evaluate_validation<C: EvaluationContext>(
    evaluator: &mut ExprEvaluator<C>,
    block: &ValidationBlock<SyntaxNode>,
    io: Io,
    task: bool,
    name: &str,
) -> Result<(), Diagnostic> {
    let mut failures = Vec::new();
    for clause in block.clauses() {
        // Access the expressions without panicking; a clause that failed to
        // parse never reaches evaluation as documents with errors are not
        // evaluated
        let mut exprs = Expr::children(clause.inner());
        let (Some(condition), Some(message)) = (exprs.next(), exprs.next()) else {
            continue;
        };

        match evaluate_condition(evaluator, &condition).await {
            Ok(true) => continue,
            Ok(false) => {}
            Err(e) => {
                failures.push(ValidationFailure {
                    description: format!("failed to evaluate condition: {e}", e = e.message()),
                    label: "this condition failed to evaluate",
                    span: condition.span(),
                });
                continue;
            }
        }

        match evaluate_message(evaluator, &message).await {
            Ok(description) => failures.push(ValidationFailure {
                description,
                label: "this condition evaluated to `false`",
                span: condition.span(),
            }),
            Err(e) => failures.push(ValidationFailure {
                description: format!("failed to evaluate message: {e}", e = e.message()),
                label: "this message failed to evaluate",
                span: message.span(),
            }),
        }
    }

    if failures.is_empty() {
        return Ok(());
    }

    Err(validation_failed(io, task, name, &failures))
}

/// Evaluates the condition of a validation clause.
async fn evaluate_condition<C: EvaluationContext>(
    evaluator: &mut ExprEvaluator<C>,
    expr: &Expr<SyntaxNode>,
) -> Result<bool, Diagnostic> {
    let value = evaluator.evaluate_expr(expr).await?;
    let ty = value.ty();
    value
        .coerce(None, &PrimitiveType::Boolean.into())
        .map(|v| v.unwrap_boolean())
        .map_err(|e| {
            Diagnostic::error(format!(
                "type mismatch: expected type `Boolean`, but found {ty:#}: {e:#}"
            ))
            .with_highlight(expr.span())
        })
}

/// Evaluates the message of a validation clause.
async fn evaluate_message<C: EvaluationContext>(
    evaluator: &mut ExprEvaluator<C>,
    expr: &Expr<SyntaxNode>,
) -> Result<String, Diagnostic> {
    let value = evaluator.evaluate_expr(expr).await?;
    let ty = value.ty();
    value
        .coerce(None, &PrimitiveType::String.into())
        .map(|v| v.unwrap_string().to_string())
        .map_err(|e| {
            Diagnostic::error(format!(
                "type mismatch: expected type `String`, but found {ty:#}: {e:#}"
            ))
            .with_highlight(expr.span())
        })
}
