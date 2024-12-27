//! Implements the `as_pairs` function from the WDL standard library.

use wdl_ast::Diagnostic;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::Array;
use crate::Pair;
use crate::Value;

/// Converts a Map into an Array of Pairs.
///
/// Since Maps are ordered, the output array will always have elements in the
/// same order they were added to the Map.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#as_pairs
fn as_pairs(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 1);

    let map = context.arguments[0]
        .value
        .as_map()
        .expect("argument should be a map");

    let element_ty = context
        .return_type
        .as_array()
        .expect("type should be an array")
        .element_type();

    let elements = map
        .iter()
        .map(|(k, v)| Pair::new_unchecked(element_ty.clone(), k.clone().into(), v.clone()).into())
        .collect();

    Ok(Array::new_unchecked(context.return_type, elements).into())
}

/// Gets the function describing `as_pairs`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(Map[K, V]) -> Array[Pair[K, V]] where `K`: any primitive type",
                as_pairs,
            )]
        },
    )
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use wdl_ast::version::V1;

    use crate::PrimitiveValue;
    use crate::Value;
    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[test]
    fn as_pairs() {
        let mut env = TestEnv::default();

        let value = eval_v1_expr(&mut env, V1::One, "as_pairs({})").unwrap();
        assert_eq!(value.unwrap_array().len(), 0);

        let value = eval_v1_expr(
            &mut env,
            V1::One,
            "as_pairs({ 'foo': 'bar', 'bar': 'baz' })",
        )
        .unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| {
                let pair = v.as_pair().unwrap();
                (
                    pair.left().as_string().unwrap().as_str(),
                    pair.right().as_string().unwrap().as_str(),
                )
            })
            .collect();
        assert_eq!(elements, [("foo", "bar"), ("bar", "baz")]);

        let value = eval_v1_expr(&mut env, V1::One, "as_pairs({'a': 1, 'c': 3, 'b': 2})").unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| {
                let pair = v.as_pair().unwrap();
                (
                    pair.left().as_string().unwrap().as_str(),
                    pair.right().as_integer().unwrap(),
                )
            })
            .collect();
        assert_eq!(elements, [("a", 1), ("c", 3), ("b", 2)]);

        let value = eval_v1_expr(&mut env, V1::One, "as_pairs({'a': 1, None: 3, 'b': 2})").unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| {
                let pair = v.as_pair().unwrap();
                (
                    match pair.left() {
                        Value::None => None,
                        Value::Primitive(PrimitiveValue::String(s)) => Some(s.as_str()),
                        _ => panic!("expected a String?"),
                    },
                    pair.right().as_integer().unwrap(),
                )
            })
            .collect();
        assert_eq!(elements, [(Some("a"), 1), (None, 3), (Some("b"), 2)]);
    }
}
