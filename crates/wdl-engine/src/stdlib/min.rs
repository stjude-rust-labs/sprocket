//! Implements the `min` function from the WDL standard library.

use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::Value;

/// Returns the smaller of two integer values.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#min
fn int_min(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 2);
    debug_assert!(context.return_type_eq(PrimitiveType::Integer));

    let first = context
        .coerce_argument(0, PrimitiveType::Integer)
        .unwrap_integer();
    let second = context
        .coerce_argument(1, PrimitiveType::Integer)
        .unwrap_integer();
    Ok(first.min(second).into())
}

/// Returns the smaller of two float values.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#min
fn float_min(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 2);
    debug_assert!(context.return_type_eq(PrimitiveType::Float));

    let first = context
        .coerce_argument(0, PrimitiveType::Float)
        .unwrap_float();
    let second = context
        .coerce_argument(1, PrimitiveType::Float)
        .unwrap_float();
    Ok(first.min(second).into())
}

/// Gets the function describing `min`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[
                Signature::new("(Int, Int) -> Int", Callback::Sync(int_min)),
                Signature::new("(Int, Float) -> Float", Callback::Sync(float_min)),
                Signature::new("(Float, Int) -> Float", Callback::Sync(float_min)),
                Signature::new("(Float, Float) -> Float", Callback::Sync(float_min)),
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

    #[tokio::test]
    async fn min() {
        let env = TestEnv::default();
        let value = eval_v1_expr(&env, V1::One, "min(7, 42)").await.unwrap();
        assert_eq!(value.unwrap_integer(), 7);

        let value = eval_v1_expr(&env, V1::One, "min(42, 7)").await.unwrap();
        assert_eq!(value.unwrap_integer(), 7);

        let value = eval_v1_expr(&env, V1::One, "min(-42, 7)").await.unwrap();
        assert_eq!(value.unwrap_integer(), -42);

        let value = eval_v1_expr(&env, V1::One, "min(0, -42)").await.unwrap();
        assert_eq!(value.unwrap_integer(), -42);

        let value = eval_v1_expr(&env, V1::One, "min(0, 42)").await.unwrap();
        assert_eq!(value.unwrap_integer(), 0);

        let value = eval_v1_expr(&env, V1::One, "min(7.0, 42)").await.unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 7.0);

        let value = eval_v1_expr(&env, V1::One, "min(42.0, 7)").await.unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 7.0);

        let value = eval_v1_expr(&env, V1::One, "min(-42.0, 7)").await.unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -42.0);

        let value = eval_v1_expr(&env, V1::One, "min(0.0, -42)").await.unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -42.0);

        let value = eval_v1_expr(&env, V1::One, "min(0.0, 42)").await.unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -0.0);

        let value = eval_v1_expr(&env, V1::One, "min(7, 42.0)").await.unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 7.0);

        let value = eval_v1_expr(&env, V1::One, "min(42, 7.0)").await.unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 7.0);

        let value = eval_v1_expr(&env, V1::One, "min(-42, 7.0)").await.unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -42.0);

        let value = eval_v1_expr(&env, V1::One, "min(0, -42.0)").await.unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -42.0);

        let value = eval_v1_expr(&env, V1::One, "min(0, 42.0)").await.unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -0.0);

        let value = eval_v1_expr(&env, V1::One, "min(7.0, 42.0)").await.unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 7.0);

        let value = eval_v1_expr(&env, V1::One, "min(42.0, 7.0)").await.unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 7.0);

        let value = eval_v1_expr(&env, V1::One, "min(-42.0, 7.0)")
            .await
            .unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -42.0);

        let value = eval_v1_expr(&env, V1::One, "min(0.0, -42.0)")
            .await
            .unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -42.0);

        let value = eval_v1_expr(&env, V1::One, "min(0.0, 42.0)").await.unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -0.0);

        let value = eval_v1_expr(
            &env,
            V1::One,
            "min(12345, min(-100, min(54321, 1234.5678)))",
        )
        .await
        .unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -100.0);
    }
}
