//! Implements the `cross` function from the WDL standard library.

use itertools::Itertools;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::Array;
use crate::Pair;
use crate::Value;

/// Creates an array of Pairs containing the cross product of two input arrays,
/// i.e., each element in the first array is paired with each element in the
/// second array.
///
/// Given Array[X] of length M, and Array[Y] of length N, the cross product is
/// Array[Pair[X, Y]] of length M*N with the following elements: [(X0, Y0), (X0,
/// Y1), ..., (X0, Yn-1), (X1, Y0), ..., (X1, Yn-1), ..., (Xm-1, Yn-1)].
///
/// If either of the input arrays is empty, an empty array is returned.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#cross
fn cross(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 2);
    debug_assert!(
        context
            .types()
            .type_definition(
                context
                    .return_type
                    .as_compound()
                    .expect("type should be compound")
                    .definition()
            )
            .as_array()
            .is_some(),
        "type should be an array"
    );

    let left = context.arguments[0]
        .value
        .as_array()
        .expect("argument should be an array");

    let right = context.arguments[1]
        .value
        .as_array()
        .expect("argument should be an array");

    let element_ty = context
        .types()
        .type_definition(
            context
                .return_type
                .as_compound()
                .expect("type should be compound")
                .definition(),
        )
        .as_array()
        .unwrap()
        .element_type();

    let elements = left
        .as_slice()
        .iter()
        .cartesian_product(right.as_slice().iter())
        .map(|(l, r)| Pair::new_unchecked(element_ty, l.clone(), r.clone()).into())
        .collect();
    Ok(Array::new_unchecked(context.return_type, elements).into())
}

/// Gets the function describing `cross`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(Array[X], Array[Y]) -> Array[Pair[X, Y]]",
                cross,
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
    fn cross() {
        let mut env = TestEnv::default();

        let value = eval_v1_expr(&mut env, V1::One, "cross([], [])").unwrap();
        assert_eq!(value.as_array().unwrap().len(), 0);

        let value = eval_v1_expr(&mut env, V1::One, "cross([1], [])").unwrap();
        assert_eq!(value.as_array().unwrap().len(), 0);

        let value = eval_v1_expr(&mut env, V1::One, "cross([], [1])").unwrap();
        assert_eq!(value.as_array().unwrap().len(), 0);

        let value = eval_v1_expr(&mut env, V1::One, "cross([1, 2, 3], ['a', 'b'])").unwrap();
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
        assert_eq!(elements, [
            (1, "a"),
            (1, "b"),
            (2, "a"),
            (2, "b"),
            (3, "a"),
            (3, "b")
        ]);
    }
}
