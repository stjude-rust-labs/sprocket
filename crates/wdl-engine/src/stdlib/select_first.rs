//! Implements the `select_first` function from the WDL standard library.

use wdl_ast::Diagnostic;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::Value;
use crate::diagnostics::function_call_failed;

/// Selects the first - i.e., left-most - non-None value from an Array of
/// optional values.
///
/// The optional second parameter provides a default value that is returned if
/// the array is empty or contains only None values.
///
/// If the default value is not provided and the array is empty or contains only
/// None values, then an error is raised.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#select_first
fn select_first(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert!(!context.arguments.is_empty() && context.arguments.len() < 3);

    let array = context.arguments[0]
        .value
        .as_array()
        .expect("argument should be an array");

    if array.is_empty() {
        return Err(function_call_failed(
            "select_first",
            "array is empty",
            context.arguments[0].span,
        ));
    }

    match array.as_slice().iter().find(|v| !v.is_none()) {
        Some(v) => Ok(v.clone_as_required()),
        None => {
            if context.arguments.len() < 2 {
                return Err(function_call_failed(
                    "select_first",
                    "array contains only `None` values",
                    context.arguments[0].span,
                ));
            }

            Ok(context.arguments[1].value.clone_as_required())
        }
    }
}

/// Gets the function describing `select_first`.
pub const fn descriptor() -> Function {
    Function::new(const { &[Signature::new("(Array[X], <X>) -> X", select_first)] })
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use wdl_ast::version::V1;

    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[tokio::test]
    async fn select_first() {
        let env = TestEnv::default();

        let diagnostic = eval_v1_expr(&env, V1::One, "select_first([])")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `select_first` failed: array is empty"
        );

        let diagnostic = eval_v1_expr(&env, V1::One, "select_first([], 1)")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `select_first` failed: array is empty"
        );

        let diagnostic = eval_v1_expr(&env, V1::One, "select_first([None, None, None])")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `select_first` failed: array contains only `None` values"
        );

        let value = eval_v1_expr(&env, V1::One, "select_first([None, None, None], 12345)")
            .await
            .unwrap();
        assert_eq!(value.unwrap_integer(), 12345);

        let value = eval_v1_expr(&env, V1::One, "select_first([1, None, 3])")
            .await
            .unwrap();
        assert_eq!(value.unwrap_integer(), 1);

        let value = eval_v1_expr(&env, V1::One, "select_first([None, 2, 3])")
            .await
            .unwrap();
        assert_eq!(value.unwrap_integer(), 2);

        let value = eval_v1_expr(&env, V1::One, "select_first([None, None, 3])")
            .await
            .unwrap();
        assert_eq!(value.unwrap_integer(), 3);

        let value = eval_v1_expr(&env, V1::One, "select_first([None, 2, None], 12345)")
            .await
            .unwrap();
        assert_eq!(value.unwrap_integer(), 2);

        let value = eval_v1_expr(&env, V1::One, "select_first([1, 2, 3])")
            .await
            .unwrap();
        assert_eq!(value.unwrap_integer(), 1);
    }
}
