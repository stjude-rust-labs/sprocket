//! Implements the `select_all` function from the WDL standard library.

use std::sync::Arc;

use wdl_ast::Diagnostic;

use super::CallContext;
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
        context
            .types()
            .type_definition(
                context
                    .return_type
                    .as_compound()
                    .expect("type should be compound")
                    .definition(),
            )
            .as_array()
            .is_some(),
        "return type should be an array"
    );
    let array = context.arguments[0]
        .value
        .as_array()
        .expect("argument should be an array");

    let elements = array
        .elements()
        .iter()
        .filter(|v| !v.is_none())
        .cloned()
        .collect();
    Ok(Array::new_unchecked(context.return_type, Arc::new(elements)).into())
}

/// Gets the function describing `select_all`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(Array[X]) -> Array[X] where `X`: any optional type",
                select_all,
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
    fn select_all() {
        let mut env = TestEnv::default();

        let value = eval_v1_expr(&mut env, V1::One, "select_all([])").unwrap();
        assert_eq!(value.unwrap_array().len(), 0);

        let value = eval_v1_expr(&mut env, V1::One, "select_all([None, None, None])").unwrap();
        assert_eq!(value.unwrap_array().len(), 0);

        let value = eval_v1_expr(&mut env, V1::One, "select_all([None, 2, None])").unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .elements()
            .iter()
            .map(|v| v.as_integer().unwrap())
            .collect();
        assert_eq!(elements, [2]);

        let value = eval_v1_expr(&mut env, V1::One, "select_all([1, 2, None])").unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .elements()
            .iter()
            .map(|v| v.as_integer().unwrap())
            .collect();
        assert_eq!(elements, [1, 2]);

        let value = eval_v1_expr(&mut env, V1::One, "select_all([1, 2, 3, None])").unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .elements()
            .iter()
            .map(|v| v.as_integer().unwrap())
            .collect();
        assert_eq!(elements, [1, 2, 3]);
    }
}
