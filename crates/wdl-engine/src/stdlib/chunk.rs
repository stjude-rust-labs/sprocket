//! Implements the `chunk` function from the WDL standard library.

use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::Array;
use crate::Value;
use crate::diagnostics::function_call_failed;

/// The name of the function defined in this file for use in diagnostics.
const FUNCTION_NAME: &str = "chunk";

/// Given an array and a length `n`, splits the array into consecutive,
/// non-overlapping arrays of n elements.
///
/// If the length of the array is not a multiple `n` then the final sub-array
/// will have length(array) % `n` elements.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#-chunk
fn chunk(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 2);

    let array = context.arguments[0]
        .value
        .as_array()
        .expect("argument should be an array");

    let size = context.arguments[1]
        .value
        .as_integer()
        .expect("argument should be an integer");

    if size < 0 {
        return Err(function_call_failed(
            FUNCTION_NAME,
            "chunk size cannot be negative",
            context.arguments[1].span,
        ));
    }

    let element_ty = context
        .return_type
        .as_array()
        .expect("type should be an array")
        .element_type();

    let elements = array
        .as_slice()
        .chunks(size as usize)
        .map(|chunk| {
            Array::new_unchecked(element_ty.clone(), Vec::from_iter(chunk.iter().cloned())).into()
        })
        .collect();

    Ok(Array::new_unchecked(context.return_type, elements).into())
}

/// Gets the function describing `chunk`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(array: Array[X], size: Int) -> Array[Array[X]]",
                Callback::Sync(chunk),
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
    async fn chunk() {
        let env = TestEnv::default();

        let value = eval_v1_expr(&env, V1::Two, "chunk([], 10)").await.unwrap();
        assert_eq!(value.as_array().unwrap().len(), 0);

        let value = eval_v1_expr(&env, V1::Two, "chunk([1, 2, 3, 4, 5], 1)")
            .await
            .unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| {
                v.as_array()
                    .unwrap()
                    .as_slice()
                    .iter()
                    .map(|v| v.as_integer().unwrap())
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(elements, [[1], [2], [3], [4], [5]]);

        let value = eval_v1_expr(&env, V1::Two, "chunk([1, 2, 3, 4, 5], 2)")
            .await
            .unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| {
                v.as_array()
                    .unwrap()
                    .as_slice()
                    .iter()
                    .map(|v| v.as_integer().unwrap())
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(elements, [[1, 2].as_slice(), &[3, 4], &[5]]);

        let value = eval_v1_expr(&env, V1::Two, "chunk([1, 2, 3, 4, 5], 3)")
            .await
            .unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| {
                v.as_array()
                    .unwrap()
                    .as_slice()
                    .iter()
                    .map(|v| v.as_integer().unwrap())
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(elements, [[1, 2, 3].as_slice(), &[4, 5]]);

        let value = eval_v1_expr(&env, V1::Two, "chunk([1, 2, 3, 4, 5], 4)")
            .await
            .unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| {
                v.as_array()
                    .unwrap()
                    .as_slice()
                    .iter()
                    .map(|v| v.as_integer().unwrap())
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(elements, [[1, 2, 3, 4].as_slice(), &[5]]);

        let value = eval_v1_expr(&env, V1::Two, "chunk([1, 2, 3, 4, 5], 5)")
            .await
            .unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| {
                v.as_array()
                    .unwrap()
                    .as_slice()
                    .iter()
                    .map(|v| v.as_integer().unwrap())
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(elements, [[1, 2, 3, 4, 5]]);

        let value = eval_v1_expr(&env, V1::Two, "chunk([1, 2, 3, 4, 5], 10)")
            .await
            .unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| {
                v.as_array()
                    .unwrap()
                    .as_slice()
                    .iter()
                    .map(|v| v.as_integer().unwrap())
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(elements, [[1, 2, 3, 4, 5]]);

        let diagnostic = eval_v1_expr(&env, V1::Two, "chunk([1, 2, 3], -10)")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `chunk` failed: chunk size cannot be negative"
        );
    }
}
