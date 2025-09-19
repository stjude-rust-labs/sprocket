//! Implements the `zip` function from the WDL standard library.

use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::Array;
use crate::Pair;
use crate::Value;
use crate::diagnostics::function_call_failed;

/// The name of the function defined in this file for use in diagnostics.
const FUNCTION_NAME: &str = "zip";

/// Creates an array of Pairs containing the dot product of two input arrays,
/// i.e., the elements at the same indices in each array X[i] and Y[i] are
/// combined together into (X[i], Y[i]) for each i in range(length(X)).
///
/// The input arrays must have the same lengths or an error is raised.
///
/// If the input arrays are empty, an empty array is returned.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#zip
fn zip(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 2);

    let left = context.arguments[0]
        .value
        .as_array()
        .expect("argument should be an array");

    let right = context.arguments[1]
        .value
        .as_array()
        .expect("argument should be an array");

    if left.len() != right.len() {
        return Err(function_call_failed(
            FUNCTION_NAME,
            format!(
                "expected an array of length {left}, but this is an array of length {right}",
                left = left.len(),
                right = right.len()
            ),
            context.arguments[1].span,
        ));
    }

    let element_ty = context
        .return_type
        .as_array()
        .expect("type should be an array")
        .element_type();

    debug_assert!(
        element_ty.as_pair().is_some(),
        "element type should be a pair"
    );

    let elements = left
        .as_slice()
        .iter()
        .zip(right.as_slice())
        .map(|(l, r)| Pair::new_unchecked(element_ty.clone(), l.clone(), r.clone()).into())
        .collect();

    Ok(Array::new_unchecked(context.return_type, elements).into())
}

/// Gets the function describing `zip`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(Array[X], Array[Y]) -> Array[Pair[X, Y]]",
                Callback::Sync(zip),
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
    async fn zip() {
        let env = TestEnv::default();

        let value = eval_v1_expr(&env, V1::One, "zip([], [])").await.unwrap();
        assert_eq!(value.as_array().unwrap().len(), 0);

        let value = eval_v1_expr(&env, V1::One, "zip([1, 2, 3], ['a', 'b', 'c'])")
            .await
            .unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| {
                let p = v.as_pair().unwrap();
                (
                    p.left().as_integer().unwrap(),
                    p.right().as_string().unwrap().as_str(),
                )
            })
            .collect();
        assert_eq!(elements, [(1, "a"), (2, "b"), (3, "c")]);

        let diagnostic = eval_v1_expr(&env, V1::One, "zip([1, 2, 3], ['a'])")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `zip` failed: expected an array of length 3, but this is an array \
             of length 1"
        );
    }
}
