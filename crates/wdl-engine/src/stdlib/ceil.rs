//! Implements the `ceil` function from the WDL standard library.

use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::Value;

/// Rounds a floating point number up to the next higher integer.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#ceil
fn ceil(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 1);
    debug_assert!(context.return_type_eq(PrimitiveType::Integer));

    let arg = context
        .coerce_argument(0, PrimitiveType::Float)
        .unwrap_float();
    Ok((arg.ceil() as i64).into())
}

/// Gets the function describing `ceil`.
pub const fn descriptor() -> Function {
    Function::new(const { &[Signature::new("(Float) -> Int", ceil)] })
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use wdl_ast::version::V1;

    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[test]
    fn ceil() {
        let mut env = TestEnv::default();
        let value = eval_v1_expr(&mut env, V1::Zero, "ceil(10.5)").unwrap();
        assert_eq!(value.unwrap_integer(), 11);

        let value = eval_v1_expr(&mut env, V1::Zero, "ceil(10)").unwrap();
        assert_eq!(value.unwrap_integer(), 10);

        let value = eval_v1_expr(&mut env, V1::Zero, "ceil(9.9999)").unwrap();
        assert_eq!(value.unwrap_integer(), 10);

        let value = eval_v1_expr(&mut env, V1::Zero, "ceil(0)").unwrap();
        assert_eq!(value.unwrap_integer(), 0);

        let value = eval_v1_expr(&mut env, V1::Zero, "ceil(-5.1)").unwrap();
        assert_eq!(value.unwrap_integer(), -5);
    }
}
