//! Implements the `as_map` function from the WDL standard library.

use std::fmt;

use indexmap::IndexMap;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::Map;
use crate::PrimitiveValue;
use crate::Value;
use crate::diagnostics::function_call_failed;

/// The name of the function defined in this file for use in diagnostics.
const FUNCTION_NAME: &str = "as_map";

/// Used for displaying duplicate key errors.
struct DuplicateKeyError(Option<PrimitiveValue>);

impl fmt::Display for DuplicateKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Some(v) => write!(
                f,
                "array contains a duplicate entry for map key `{v}`",
                v = v.raw(None)
            ),
            None => write!(f, "array contains a duplicate entry for map key `None`"),
        }
    }
}

/// Converts an Array of Pairs into a Map in which the left elements of the
/// Pairs are the keys and the right elements the values.
///
/// All the keys must be unique, or an error is raised.
///
/// The order of the key/value pairs in the output Map is the same as the order
/// of the Pairs in the Array.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#as_map
fn as_map(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 1);
    debug_assert!(
        context.return_type.as_map().is_some(),
        "return type should be a map"
    );

    let array = context.arguments[0]
        .value
        .as_array()
        .expect("argument should be an array");

    let mut elements = IndexMap::with_capacity(array.len());
    for e in array.as_slice() {
        let pair = e.as_pair().expect("element should be a pair");
        let key = match pair.left() {
            Value::None(_) => None,
            Value::Primitive(v) => Some(v.clone()),
            _ => unreachable!("expected a primitive type for the left value"),
        };

        if elements.insert(key.clone(), pair.right().clone()).is_some() {
            return Err(function_call_failed(
                FUNCTION_NAME,
                DuplicateKeyError(key),
                context.arguments[0].span,
            ));
        }
    }

    Ok(Map::new_unchecked(context.return_type, elements).into())
}

/// Gets the function describing `as_map`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(Array[Pair[K, V]]) -> Map[K, V] where `K`: any primitive type",
                Callback::Sync(as_map),
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
    async fn as_map() {
        let env = TestEnv::default();

        let value = eval_v1_expr(&env, V1::One, "as_map([])").await.unwrap();
        assert_eq!(value.unwrap_map().len(), 0);

        let value = eval_v1_expr(&env, V1::One, "as_map([('foo', 'bar'), ('bar', 'baz')])")
            .await
            .unwrap();
        let elements: Vec<_> = value
            .as_map()
            .unwrap()
            .iter()
            .map(|(k, v)| {
                (
                    k.as_ref().and_then(|k| k.as_string()).unwrap().as_str(),
                    v.as_string().unwrap().as_str(),
                )
            })
            .collect();
        assert_eq!(elements, [("foo", "bar"), ("bar", "baz")]);

        let value = eval_v1_expr(&env, V1::One, "as_map([('a', 1), ('c', 3), ('b', 2)])")
            .await
            .unwrap();
        let elements: Vec<_> = value
            .as_map()
            .unwrap()
            .iter()
            .map(|(k, v)| {
                (
                    k.as_ref().and_then(|k| k.as_string()).unwrap().as_str(),
                    v.as_integer().unwrap(),
                )
            })
            .collect();
        assert_eq!(elements, [("a", 1), ("c", 3), ("b", 2)]);

        let value = eval_v1_expr(&env, V1::One, "as_map([('a', 1), (None, 3), ('b', 2)])")
            .await
            .unwrap();
        let elements: Vec<_> = value
            .as_map()
            .unwrap()
            .iter()
            .map(|(k, v)| {
                (
                    k.as_ref().map(|k| k.as_string().unwrap().as_str()),
                    v.as_integer().unwrap(),
                )
            })
            .collect();
        assert_eq!(elements, [(Some("a"), 1), (None, 3), (Some("b"), 2)]);

        let value = eval_v1_expr(&env, V1::One, "as_map(as_pairs({'a': 1, 'c': 3, 'b': 2}))")
            .await
            .unwrap();
        let elements: Vec<_> = value
            .as_map()
            .unwrap()
            .iter()
            .map(|(k, v)| {
                (
                    k.as_ref().and_then(|k| k.as_string()).unwrap().as_str(),
                    v.as_integer().unwrap(),
                )
            })
            .collect();
        assert_eq!(elements, [("a", 1), ("c", 3), ("b", 2)]);

        let diagnostic = eval_v1_expr(&env, V1::One, "as_map([('a', 1), ('c', 3), ('a', 2)])")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `as_map` failed: array contains a duplicate entry for map key `a`"
        );

        let diagnostic = eval_v1_expr(&env, V1::One, "as_map([(None, 1), ('c', 3), (None, 2)])")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `as_map` failed: array contains a duplicate entry for map key `None`"
        );
    }
}
