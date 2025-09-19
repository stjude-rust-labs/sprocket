//! Implements the `select_all` function from the WDL standard library.

use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::Array;
use crate::Value;

/// Filters the input Array of optional values by removing all None values.
///
/// The elements in the output Array are in the same order as the input Array.
///
/// If the input array is empty or contains only None values, an empty array is
/// returned.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#select_all
fn select_all(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 1);
    debug_assert!(
        context.return_type.as_array().is_some(),
        "return type should be an array"
    );
    let array = context.arguments[0]
        .value
        .as_array()
        .expect("argument should be an array");

    let elements = array
        .as_slice()
        .iter()
        .filter(|v| !v.is_none())
        .cloned()
        .collect();
    Ok(Array::new_unchecked(context.return_type, elements).into())
}

/// Gets the function describing `select_all`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(Array[X]) -> Array[X]",
                Callback::Sync(select_all),
            )]
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
    async fn select_all() {
        let env = TestEnv::default();

        let value = eval_v1_expr(&env, V1::One, "select_all([])").await.unwrap();
        assert_eq!(value.unwrap_array().len(), 0);

        let value = eval_v1_expr(&env, V1::One, "select_all([None, None, None])")
            .await
            .unwrap();
        assert_eq!(value.unwrap_array().len(), 0);

        let value = eval_v1_expr(&env, V1::One, "select_all([None, 2, None])")
            .await
            .unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v.as_integer().unwrap())
            .collect();
        assert_eq!(elements, [2]);

        let value = eval_v1_expr(&env, V1::One, "select_all([1, 2, None])")
            .await
            .unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v.as_integer().unwrap())
            .collect();
        assert_eq!(elements, [1, 2]);

        let value = eval_v1_expr(&env, V1::One, "select_all([1, 2, 3, None])")
            .await
            .unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v.as_integer().unwrap())
            .collect();
        assert_eq!(elements, [1, 2, 3]);

        let value = eval_v1_expr(&env, V1::One, "select_all([1, 2, 3])")
            .await
            .unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v.as_integer().unwrap())
            .collect();
        assert_eq!(elements, [1, 2, 3]);
    }
}
