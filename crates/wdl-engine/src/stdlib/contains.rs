//! Implements the `contains` function from the WDL standard library.

use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;

use super::CallContext;
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
                contains,
            )]
        },
    )
}

#[cfg(test)]
mod test {
    use wdl_ast::version::V1;

    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[test]
    fn contains() {
        let mut env = TestEnv::default();

        assert!(
            !eval_v1_expr(&mut env, V1::Two, "contains([], 1)")
                .unwrap()
                .unwrap_boolean()
        );
        assert!(
            !eval_v1_expr(&mut env, V1::Two, "contains([], None)")
                .unwrap()
                .unwrap_boolean()
        );
        assert!(
            eval_v1_expr(&mut env, V1::Two, "contains([1, None, 3], None)")
                .unwrap()
                .unwrap_boolean()
        );
        assert!(
            eval_v1_expr(&mut env, V1::Two, "contains([None], None)")
                .unwrap()
                .unwrap_boolean()
        );
        assert!(
            eval_v1_expr(&mut env, V1::Two, "contains([1, 2, 3, 4, 5], 2)")
                .unwrap()
                .unwrap_boolean()
        );
        assert!(
            !eval_v1_expr(&mut env, V1::Two, "contains([1, 2, 3, 4, 5], 100)")
                .unwrap()
                .unwrap_boolean()
        );
        assert!(
            eval_v1_expr(&mut env, V1::Two, "contains(['foo', 'bar', 'baz'], 'foo')")
                .unwrap()
                .unwrap_boolean()
        );
        assert!(
            !eval_v1_expr(&mut env, V1::Two, "contains(['foo', 'bar', 'baz'], 'qux')")
                .unwrap()
                .unwrap_boolean()
        );
        assert!(
            !eval_v1_expr(&mut env, V1::Two, "contains(['foo', None, 'baz'], 'bar')")
                .unwrap()
                .unwrap_boolean()
        );
        assert!(
            eval_v1_expr(&mut env, V1::Two, "contains(['foo', None, 'baz'], None)")
                .unwrap()
                .unwrap_boolean()
        );
    }
}
