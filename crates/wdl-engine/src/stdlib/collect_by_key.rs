//! Implements the `collect_by_key` function from the WDL standard library.

use indexmap::IndexMap;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::Array;
use crate::Map;
use crate::Value;

/// Given an Array of Pairs, creates a Map in which the right elements of the
/// Pairs are grouped by the left elements.
///
/// In other words, the input Array may have multiple Pairs with the same key.
///
/// Rather than causing an error (as would happen with as_map), all the values
/// with the same key are grouped together into an Array.
///
/// The order of the keys in the output Map is the same as the order of their
/// first occurrence in the input Array.
///
/// The order of the elements in the Map values is the same as their order of
/// occurrence in the input Array.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#collect_by_key
fn collect_by_key(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 1);

    let array = context.arguments[0]
        .value
        .as_array()
        .expect("value should be an array");

    let map_ty = context
        .return_type
        .as_map()
        .expect("return type should be a map");
    debug_assert!(
        map_ty.value_type().as_array().is_some(),
        "return type's value type should be an array"
    );

    // Start by collecting duplicate keys into a `Vec<Value>`
    let mut map: IndexMap<_, Vec<_>> = IndexMap::new();
    for v in array.as_slice() {
        let pair = v.as_pair().expect("value should be a pair");
        map.entry(match pair.left() {
            Value::None(_) => None,
            Value::Primitive(v) => Some(v.clone()),
            _ => unreachable!("value should be primitive"),
        })
        .or_default()
        .push(pair.right().clone());
    }

    // Transform each `Vec<Value>` into an array value
    let elements = map
        .into_iter()
        .map(|(k, v)| {
            (
                k,
                Array::new_unchecked(map_ty.value_type().clone(), v).into(),
            )
        })
        .collect();

    Ok(Map::new_unchecked(context.return_type, elements).into())
}

/// Gets the function describing `collect_by_key`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(Array[Pair[K, V]]) -> Map[K, Array[V]] where `K`: any primitive type",
                Callback::Sync(collect_by_key),
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
    async fn collect_by_key() {
        let env = TestEnv::default();

        let value = eval_v1_expr(&env, V1::Two, "collect_by_key([])")
            .await
            .unwrap();
        assert_eq!(value.unwrap_map().len(), 0);

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "collect_by_key([('a', 1), ('b', 2), ('a', 3)])",
        )
        .await
        .unwrap();
        assert_eq!(value.to_string(), r#"{"a": [1, 3], "b": [2]}"#);

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "collect_by_key([('a', 1), (None, 2), ('a', 3), (None, 4), ('b', 5), ('c', 6), ('b', \
             7)])",
        )
        .await
        .unwrap();
        assert_eq!(
            value.to_string(),
            r#"{"a": [1, 3], None: [2, 4], "b": [5, 7], "c": [6]}"#
        );
    }
}
