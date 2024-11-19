//! Implements the `min` function from the WDL standard library.

use wdl_analysis::types::PrimitiveTypeKind;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::Value;

/// Returns the smaller of two integer values.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#min
fn int_min(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 2);
    debug_assert!(context.return_type_eq(PrimitiveTypeKind::Integer));

    let first = context
        .coerce_argument(0, PrimitiveTypeKind::Integer)
        .unwrap_integer();
    let second = context
        .coerce_argument(1, PrimitiveTypeKind::Integer)
        .unwrap_integer();
    Ok(first.min(second).into())
}

/// Returns the smaller of two float values.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#min
fn float_min(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 2);
    debug_assert!(context.return_type_eq(PrimitiveTypeKind::Float));

    let first = context
        .coerce_argument(0, PrimitiveTypeKind::Float)
        .unwrap_float();
    let second = context
        .coerce_argument(1, PrimitiveTypeKind::Float)
        .unwrap_float();
    Ok(first.min(second).into())
}

/// Gets the function describing `min`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[
                Signature::new("(Int, Int) -> Int", int_min),
                Signature::new("(Int, Float) -> Float", float_min),
                Signature::new("(Float, Int) -> Float", float_min),
                Signature::new("(Float, Float) -> Float", float_min),
            ]
        },
    )
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use wdl_ast::version::V1;

    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[test]
    fn min() {
        let mut env = TestEnv::default();
        let value = eval_v1_expr(&mut env, V1::One, "min(7, 42)").unwrap();
        assert_eq!(value.unwrap_integer(), 7);

        let value = eval_v1_expr(&mut env, V1::One, "min(42, 7)").unwrap();
        assert_eq!(value.unwrap_integer(), 7);

        let value = eval_v1_expr(&mut env, V1::One, "min(-42, 7)").unwrap();
        assert_eq!(value.unwrap_integer(), -42);

        let value = eval_v1_expr(&mut env, V1::One, "min(0, -42)").unwrap();
        assert_eq!(value.unwrap_integer(), -42);

        let value = eval_v1_expr(&mut env, V1::One, "min(0, 42)").unwrap();
        assert_eq!(value.unwrap_integer(), 0);

        let value = eval_v1_expr(&mut env, V1::One, "min(7.0, 42)").unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 7.0);

        let value = eval_v1_expr(&mut env, V1::One, "min(42.0, 7)").unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 7.0);

        let value = eval_v1_expr(&mut env, V1::One, "min(-42.0, 7)").unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -42.0);

        let value = eval_v1_expr(&mut env, V1::One, "min(0.0, -42)").unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -42.0);

        let value = eval_v1_expr(&mut env, V1::One, "min(0.0, 42)").unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -0.0);

        let value = eval_v1_expr(&mut env, V1::One, "min(7, 42.0)").unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 7.0);

        let value = eval_v1_expr(&mut env, V1::One, "min(42, 7.0)").unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 7.0);

        let value = eval_v1_expr(&mut env, V1::One, "min(-42, 7.0)").unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -42.0);

        let value = eval_v1_expr(&mut env, V1::One, "min(0, -42.0)").unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -42.0);

        let value = eval_v1_expr(&mut env, V1::One, "min(0, 42.0)").unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -0.0);

        let value = eval_v1_expr(&mut env, V1::One, "min(7.0, 42.0)").unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 7.0);

        let value = eval_v1_expr(&mut env, V1::One, "min(42.0, 7.0)").unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 7.0);

        let value = eval_v1_expr(&mut env, V1::One, "min(-42.0, 7.0)").unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -42.0);

        let value = eval_v1_expr(&mut env, V1::One, "min(0.0, -42.0)").unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -42.0);

        let value = eval_v1_expr(&mut env, V1::One, "min(0.0, 42.0)").unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -0.0);

        let value = eval_v1_expr(
            &mut env,
            V1::One,
            "min(12345, min(-100, min(54321, 1234.5678)))",
        )
        .unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -100.0);
    }
}
