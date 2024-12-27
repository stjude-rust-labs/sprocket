//! Implements the `round` function from the WDL standard library.

use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::Value;

/// Rounds a floating point number to the nearest integer based on standard
/// rounding rules ("round half up").
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#round
fn round(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 1);
    debug_assert!(context.return_type_eq(PrimitiveType::Integer));

    let arg = context
        .coerce_argument(0, PrimitiveType::Float)
        .unwrap_float();
    Ok((arg.round() as i64).into())
}

/// Gets the function describing `round`.
pub const fn descriptor() -> Function {
    Function::new(const { &[Signature::new("(Float) -> Int", round)] })
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use wdl_ast::version::V1;

    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[test]
    fn round() {
        let mut env = TestEnv::default();
        let value = eval_v1_expr(&mut env, V1::Zero, "round(10.5)").unwrap();
        assert_eq!(value.unwrap_integer(), 11);

        let value = eval_v1_expr(&mut env, V1::Zero, "round(10.3)").unwrap();
        assert_eq!(value.unwrap_integer(), 10);

        let value = eval_v1_expr(&mut env, V1::Zero, "round(10)").unwrap();
        assert_eq!(value.unwrap_integer(), 10);

        let value = eval_v1_expr(&mut env, V1::Zero, "round(9.9999)").unwrap();
        assert_eq!(value.unwrap_integer(), 10);

        let value = eval_v1_expr(&mut env, V1::Zero, "round(9.12345)").unwrap();
        assert_eq!(value.unwrap_integer(), 9);

        let value = eval_v1_expr(&mut env, V1::Zero, "round(0)").unwrap();
        assert_eq!(value.unwrap_integer(), 0);

        let value = eval_v1_expr(&mut env, V1::Zero, "round(-5.1)").unwrap();
        assert_eq!(value.unwrap_integer(), -5);

        let value = eval_v1_expr(&mut env, V1::Zero, "round(-5.5)").unwrap();
        assert_eq!(value.unwrap_integer(), -6);
    }
}
