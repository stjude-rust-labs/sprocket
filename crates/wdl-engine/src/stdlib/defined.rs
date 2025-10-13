//! Implements the `defined` function from the WDL standard library.

use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::Value;

/// Tests whether the given optional value is defined, i.e., has a non-None
/// value.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#defined
fn defined(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 1);
    debug_assert!(context.return_type_eq(PrimitiveType::Boolean));
    Ok((!context.arguments[0].value.is_none()).into())
}

/// Gets the function describing `defined`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(value: X) -> Boolean",
                Callback::Sync(defined),
            )]
        },
    )
}

#[cfg(test)]
mod test {
    use wdl_ast::version::V1;

    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[tokio::test]
    async fn defined() {
        let env = TestEnv::default();

        let value = eval_v1_expr(&env, V1::Zero, "defined('foo')")
            .await
            .unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(&env, V1::Zero, "defined(['foo'])")
            .await
            .unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(&env, V1::Zero, "defined(1)").await.unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(&env, V1::Zero, "defined({})").await.unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(&env, V1::Zero, "defined(None)").await.unwrap();
        assert!(!value.unwrap_boolean());
    }
}
