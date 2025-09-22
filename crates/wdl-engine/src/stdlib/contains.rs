//! Implements the `contains` function from the WDL standard library.

use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::Value;

/// Tests whether the given array contains at least one occurrence of the given
/// value.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#-contains
fn contains(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 2);
    debug_assert!(context.return_type_eq(PrimitiveType::Boolean));

    let array = context.arguments[0]
        .value
        .as_array()
        .expect("argument should be an array");

    let item = &context.arguments[1].value;

    Ok(array
        .as_slice()
        .iter()
        .any(|e| Value::equals(e, item).unwrap_or(false))
        .into())
}

/// Gets the function describing `contains`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(Array[P], P) -> Boolean where `P`: any primitive type",
                Callback::Sync(contains),
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
    async fn contains() {
        let env = TestEnv::default();

        assert!(
            !eval_v1_expr(&env, V1::Two, "contains([], 1)")
                .await
                .unwrap()
                .unwrap_boolean()
        );
        assert!(
            !eval_v1_expr(&env, V1::Two, "contains([], None)")
                .await
                .unwrap()
                .unwrap_boolean()
        );
        assert!(
            eval_v1_expr(&env, V1::Two, "contains([1, None, 3], None)")
                .await
                .unwrap()
                .unwrap_boolean()
        );
        assert!(
            eval_v1_expr(&env, V1::Two, "contains([None], None)")
                .await
                .unwrap()
                .unwrap_boolean()
        );
        assert!(
            eval_v1_expr(&env, V1::Two, "contains([1, 2, 3, 4, 5], 2)")
                .await
                .unwrap()
                .unwrap_boolean()
        );
        assert!(
            !eval_v1_expr(&env, V1::Two, "contains([1, 2, 3, 4, 5], 100)")
                .await
                .unwrap()
                .unwrap_boolean()
        );
        assert!(
            eval_v1_expr(&env, V1::Two, "contains(['foo', 'bar', 'baz'], 'foo')")
                .await
                .unwrap()
                .unwrap_boolean()
        );
        assert!(
            !eval_v1_expr(&env, V1::Two, "contains(['foo', 'bar', 'baz'], 'qux')")
                .await
                .unwrap()
                .unwrap_boolean()
        );
        assert!(
            !eval_v1_expr(&env, V1::Two, "contains(['foo', None, 'baz'], 'bar')")
                .await
                .unwrap()
                .unwrap_boolean()
        );
        assert!(
            eval_v1_expr(&env, V1::Two, "contains(['foo', None, 'baz'], None)")
                .await
                .unwrap()
                .unwrap_boolean()
        );
    }
}
