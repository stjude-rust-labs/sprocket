//! Implements the `range` function from the WDL standard library.

use std::sync::Arc;

use wdl_analysis::stdlib::STDLIB as ANALYSIS_STDLIB;
use wdl_analysis::types::PrimitiveTypeKind;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::Array;
use crate::Value;
use crate::diagnostics::function_call_failed;

/// Creates an array of the given length containing sequential integers starting
/// from 0.
///
/// The length must be >= 0. If the length is 0, an empty array is returned.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#range
fn range(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 1);
    debug_assert!(context.return_type_eq(ANALYSIS_STDLIB.array_int_type()));

    let n = context
        .coerce_argument(0, PrimitiveTypeKind::Integer)
        .unwrap_integer();

    if n < 0 {
        return Err(function_call_failed(
            "range",
            "array length cannot be negative",
            context.arguments[0].span,
        ));
    }

    Ok(Array::new_unchecked(
        context.return_type,
        Arc::new((0..n).map(Into::into).collect()),
    )
    .into())
}

/// Gets the function describing `range`.
pub const fn descriptor() -> Function {
    Function::new(const { &[Signature::new("(Int) -> Array[Int]", range)] })
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use wdl_ast::version::V1;

    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[test]
    fn range() {
        let mut env = TestEnv::default();
        let value = eval_v1_expr(&mut env, V1::One, "range(0)").unwrap();
        assert_eq!(value.unwrap_array().len(), 0);

        let value = eval_v1_expr(&mut env, V1::One, "range(10)").unwrap();
        assert_eq!(
            value
                .unwrap_array()
                .elements()
                .iter()
                .cloned()
                .map(|v| v.unwrap_integer())
                .collect::<Vec<_>>(),
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
        );

        let diagnostic = eval_v1_expr(&mut env, V1::One, "range(-10)").unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `range` failed: array length cannot be negative"
        );
    }
}
