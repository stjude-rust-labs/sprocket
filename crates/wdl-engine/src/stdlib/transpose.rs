//! Implements the `transpose` function from the WDL standard library.

use wdl_analysis::types::Type;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::Array;
use crate::Value;
use crate::diagnostics::function_call_failed;

/// Transposes a two-dimensional array according to the standard matrix
/// transposition rules, i.e. each row of the input array becomes a column of
/// the output array.
///
/// The input array must be square - i.e., every row must have the same number
/// of elements - or an error is raised.
///
/// If either the inner or the outer array is empty, an empty array is returned.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#transpose
fn transpose(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 1);
    debug_assert!(
        context.return_type.as_array().is_some(),
        "type should be an array"
    );

    let outer = context.arguments[0]
        .value
        .as_array()
        .expect("argument should be an array");

    let rows = outer.len();
    let (columns, ty) = outer
        .as_slice()
        .first()
        .map(|v| {
            (
                v.as_array()
                    .expect("element should be an array")
                    .as_slice()
                    .len(),
                v.ty(),
            )
        })
        .unwrap_or((0, Type::Union));

    let mut transposed_outer: Vec<Value> = Vec::with_capacity(columns);
    for i in 0..columns {
        let mut transposed_inner: Vec<Value> = Vec::with_capacity(rows);
        for j in 0..rows {
            let inner = outer.as_slice()[j]
                .as_array()
                .expect("element should be an array");
            if inner.len() != columns {
                return Err(function_call_failed(
                    "transpose",
                    format!("expected array at index {j} to have a length of {columns}"),
                    context.call_site,
                ));
            }

            transposed_inner.push(inner.as_slice()[i].clone())
        }

        transposed_outer.push(Array::new_unchecked(ty.clone(), transposed_inner).into());
    }

    Ok(Array::new_unchecked(context.return_type, transposed_outer).into())
}

/// Gets the function describing `transpose`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(Array[Array[X]]) -> Array[Array[X]]",
                transpose,
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

    #[test]
    fn transpose() {
        let mut env = TestEnv::default();

        let value = eval_v1_expr(&mut env, V1::One, "transpose([])").unwrap();
        assert_eq!(value.as_array().unwrap().len(), 0);

        let value = eval_v1_expr(&mut env, V1::One, "transpose([[], [], []])").unwrap();
        assert_eq!(value.as_array().unwrap().len(), 0);

        let value = eval_v1_expr(&mut env, V1::One, "transpose([[0, 1, 2], [3, 4, 5]])").unwrap();
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
        assert_eq!(elements, [[0, 3], [1, 4], [2, 5]]);

        let value = eval_v1_expr(
            &mut env,
            V1::One,
            "transpose([['a', 'b', 'c'], ['d', 'e', 'f'], ['g', 'h', 'i']])",
        )
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
                    .map(|v| v.as_string().unwrap().as_str())
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(elements, [["a", "d", "g"], ["b", "e", "h"], [
            "c", "f", "i"
        ]]);

        let diagnostic =
            eval_v1_expr(&mut env, V1::One, "transpose([['foo', 'bar'], ['baz']])").unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `transpose` failed: expected array at index 1 to have a length of 2"
        );
    }
}
