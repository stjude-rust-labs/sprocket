//! Type evaluation utilities for LSP handlers.

use wdl_ast::AstToken;
use wdl_ast::v1;

use crate::Document;
use crate::document::ScopeRef;
use crate::handlers::TypeEvalContext;
use crate::types::Type;
use crate::types::v1::ExprTypeEvaluator;

/// Evaluates the type of an expression, with special handling for enum type
/// names.
///
/// When the expression is a name reference to an enum type (e.g., `Status` in
/// `Status.Active`), this function returns the enum type directly instead of
/// evaluating it as a variable reference. For all other cases, it performs
/// standard type evaluation.
///
/// Returns the evaluated type, or `Type::Union` if evaluation fails.
pub fn evaluate_expr_type(expr: &v1::Expr, scope: ScopeRef<'_>, document: &Document) -> Type {
    match expr {
        v1::Expr::NameRef(name_ref) => {
            let name = name_ref.name().text().to_string();
            document
                .enum_by_name(&name)
                .and_then(|enum_info| enum_info.ty().cloned())
                .unwrap_or_else(|| {
                    let mut ctx = TypeEvalContext { scope, document };
                    let mut evaluator = ExprTypeEvaluator::new(&mut ctx);
                    evaluator.evaluate_expr(expr).unwrap_or(Type::Union)
                })
        }
        _ => {
            let mut ctx = TypeEvalContext { scope, document };
            let mut evaluator = ExprTypeEvaluator::new(&mut ctx);
            evaluator.evaluate_expr(expr).unwrap_or(Type::Union)
        }
    }
}
