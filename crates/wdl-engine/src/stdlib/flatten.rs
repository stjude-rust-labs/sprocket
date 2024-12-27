//! Implements the `flatten` function from the WDL standard library.

use wdl_ast::Diagnostic;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::Array;
use crate::Value;

/// Flattens a nested Array[Array[X]] by concatenating all of the element
/// arrays, in order, into a single array.
///
/// The function is not recursive - e.g. if the input is
/// Array[Array[Array[Int]]] then the output will be Array[Array[Int]].
///
/// The elements in the concatenated array are not deduplicated.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#flatten
fn flatten(context: CallContext<'_>) -> Result<Value, Diagnostic> {
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
        .flat_map(|v| {
            v.as_array()
                .expect("array element should be an array")
                .as_slice()
                .iter()
                .cloned()
        })
        .collect();

    Ok(Array::new_unchecked(context.return_type, elements).into())
}

/// Gets the function describing `flatten`.
pub const fn descriptor() -> Function {
    Function::new(const { &[Signature::new("(Array[Array[X]]) -> Array[X]", flatten)] })
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use wdl_ast::version::V1;

    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[test]
    fn flatten() {
        let mut env = TestEnv::default();

        let value = eval_v1_expr(&mut env, V1::One, "flatten([])").unwrap();
        assert_eq!(value.as_array().unwrap().len(), 0);

        let value = eval_v1_expr(&mut env, V1::One, "flatten([[], [], []])").unwrap();
        assert_eq!(value.as_array().unwrap().len(), 0);

        let value = eval_v1_expr(
            &mut env,
            V1::One,
            "flatten([[1, 2, 3], [4, 5, 6, 7], [8, 9]])",
        )
        .unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v.as_integer().unwrap())
            .collect();
        assert_eq!(elements, [1, 2, 3, 4, 5, 6, 7, 8, 9]);

        let value = eval_v1_expr(
            &mut env,
            V1::Two,
            "flatten(chunk([1, 2, 3, 4, 5, 6, 7, 8, 9], 1))",
        )
        .unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v.as_integer().unwrap())
            .collect();
        assert_eq!(elements, [1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }
}
