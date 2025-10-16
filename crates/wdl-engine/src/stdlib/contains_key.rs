//! Implements the `contains_key` function from the WDL standard library.

use std::sync::Arc;

use wdl_analysis::stdlib::STDLIB as ANALYSIS_STDLIB;
use wdl_analysis::types::PrimitiveType;
use wdl_analysis::types::Type;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::CompoundValue;
use crate::PrimitiveValue;
use crate::Struct;
use crate::Value;

/// Given a Map and a key, tests whether the collection contains an entry with
/// the given key.
///
/// `Boolean contains_key(Map[P, Y], P)`: Tests whether the Map has an entry
/// with the given key. If P is an optional type (e.g., String?), then the
/// second argument may be None.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#-contains_key
fn contains_key_map(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 2);
    debug_assert!(context.return_type_eq(PrimitiveType::Boolean));

    let map = context.arguments[0]
        .value
        .as_map()
        .expect("first argument should be a map");

    let key = match &context.arguments[1].value {
        Value::None(_) => None,
        Value::Primitive(v) => Some(v.clone()),
        _ => unreachable!("expected a primitive value for second argument"),
    };

    Ok(map.contains_key(&key).into())
}

/// Given an object and a key, tests whether the object contains an entry with
/// the given key.
///
/// `Boolean contains_key(Object, String)`: Tests whether the Object has an
/// entry with the given name.`
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#-contains_key
fn contains_key_object(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 2);
    debug_assert!(context.return_type_eq(PrimitiveType::Boolean));

    // As `Map[String, X]` coerces to `Object`, dispatch to the map overload if
    // passed a map
    if context.arguments[0].value.as_map().is_some() {
        return contains_key_map(context);
    }

    let object = context.coerce_argument(0, Type::Object).unwrap_object();
    let key = context.coerce_argument(1, PrimitiveType::String);
    Ok(object.contains_key(key.unwrap_string().as_str()).into())
}

/// Given a key-value type collection (Map, Struct, or Object) and a key, tests
/// whether the collection contains an entry with the given key.
///
/// `Boolean contains_key(Map[String, Y]|Struct|Object, Array[String])`: Tests
/// recursively for the presence of a compound key within a nested collection.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#-contains_key
fn contains_key_recursive(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 2);
    debug_assert!(context.return_type_eq(PrimitiveType::Boolean));

    /// Helper for looking up a value in a map, object, or struct by the given
    /// key.
    fn get(value: &Value, key: &Arc<String>) -> Option<Value> {
        match value {
            Value::Compound(CompoundValue::Map(map)) => {
                map.get(&Some(PrimitiveValue::String(key.clone()))).cloned()
            }
            Value::Compound(CompoundValue::Object(object)) => object.get(key.as_str()).cloned(),
            Value::Compound(CompoundValue::Struct(Struct { members, .. })) => {
                members.get(key.as_str()).cloned()
            }
            _ => None,
        }
    }

    let mut value = context.arguments[0].value.clone();
    let keys = context
        .coerce_argument(1, ANALYSIS_STDLIB.array_string_type().clone())
        .unwrap_array();

    for key in keys
        .as_slice()
        .iter()
        .map(|v| v.as_string().expect("element should be a string"))
    {
        match get(&value, key) {
            Some(v) => value = v,
            None => return Ok(false.into()),
        }
    }

    Ok(true.into())
}

/// Gets the function describing `contains_key`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[
                Signature::new(
                    "(map: Map[K, V], key: K) -> Boolean where `K`: any primitive type",
                    Callback::Sync(contains_key_map),
                ),
                Signature::new(
                    "(object: Object, key: String) -> Boolean",
                    Callback::Sync(contains_key_object),
                ),
                Signature::new(
                    "(map: Map[String, V], keys: Array[String]) -> Boolean",
                    Callback::Sync(contains_key_recursive),
                ),
                Signature::new(
                    "(struct: S, keys: Array[String]) -> Boolean where `S`: any structure",
                    Callback::Sync(contains_key_recursive),
                ),
                Signature::new(
                    "(object: Object, keys: Array[String]) -> Boolean",
                    Callback::Sync(contains_key_recursive),
                ),
            ]
        },
    )
}

#[cfg(test)]
mod test {
    use wdl_analysis::types::PrimitiveType;
    use wdl_analysis::types::StructType;
    use wdl_analysis::types::Type;
    use wdl_ast::version::V1;

    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[tokio::test]
    async fn contains_key() {
        let mut env = TestEnv::default();

        let bar_ty: Type = StructType::new("Bar", [("baz", PrimitiveType::String)]).into();
        env.insert_struct("Bar", bar_ty.clone());

        let foo_ty = StructType::new("Foo", [("bar", bar_ty)]);
        env.insert_struct("Foo", foo_ty);

        let value = eval_v1_expr(&env, V1::Two, "contains_key({}, 1)")
            .await
            .unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(&env, V1::Two, "contains_key({ 1: 2, None: 3}, None)")
            .await
            .unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(&env, V1::Two, "contains_key({ 1: 2 }, 1)")
            .await
            .unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "contains_key({ 'foo': 1, 'bar': 2, 'baz': 3 }, 'qux')",
        )
        .await
        .unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "contains_key({ 'foo': 1, 'bar': 2, 'baz': 3 }, 'baz')",
        )
        .await
        .unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "contains_key(object { foo: 1, bar: 2, baz: 3 }, 'qux')",
        )
        .await
        .unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "contains_key(object { foo: 1, bar: 2, baz: 3 }, 'baz')",
        )
        .await
        .unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "contains_key({ 'foo': 1, 'bar': 2, 'baz': 3 }, ['qux'])",
        )
        .await
        .unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "contains_key({ 'foo': 1, 'bar': 2, 'baz': 3 }, ['baz'])",
        )
        .await
        .unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "contains_key(object { foo: 1, bar: 2, baz: 3 }, ['qux'])",
        )
        .await
        .unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "contains_key(object { foo: 1, bar: 2, baz: 3 }, ['baz'])",
        )
        .await
        .unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "contains_key(Foo { bar: Bar { baz: 'qux' } }, ['qux'])",
        )
        .await
        .unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "contains_key(Foo { bar: Bar { baz: 'qux' } }, ['bar'])",
        )
        .await
        .unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "contains_key({ 'foo': 1, 'bar': 2, 'baz': 3 }, ['qux', 'nope'])",
        )
        .await
        .unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "contains_key({ 'foo': 1, 'bar': 2, 'baz': 3 }, ['baz', 'nope'])",
        )
        .await
        .unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "contains_key(object { foo: 1, bar: 2, baz: 3 }, ['qux', 'nope'])",
        )
        .await
        .unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "contains_key(object { foo: 1, bar: 2, baz: 3 }, ['baz', 'nope'])",
        )
        .await
        .unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "contains_key(Foo { bar: Bar { baz: 'qux' } }, ['qux', 'nope'])",
        )
        .await
        .unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "contains_key(Foo { bar: Bar { baz: 'qux' } }, ['bar', 'nope'])",
        )
        .await
        .unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "contains_key({ 'foo': { 'qux': 1 }, 'bar': { 'qux': 2 }, 'baz': { 'qux': 3 } }, \
             ['baz', 'qux'])",
        )
        .await
        .unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "contains_key({ 'foo': { 'qux': 1 }, 'bar': { 'qux': 2 }, 'baz': { 'qux': 3 } }, \
             ['baz', 'qux', 'nope'])",
        )
        .await
        .unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "contains_key(object { foo: 1, bar: 2, baz: object { qux: 3 } }, ['baz', 'qux'])",
        )
        .await
        .unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "contains_key(object { foo: 1, bar: 2, baz: object { qux: 3 } }, ['baz', 'qux', \
             'nope'])",
        )
        .await
        .unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "contains_key(Foo { bar: Bar { baz: 'qux' } }, ['bar', 'baz'])",
        )
        .await
        .unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "contains_key(Foo { bar: Bar { baz: 'qux' } }, ['bar', 'baz', 'nope'])",
        )
        .await
        .unwrap();
        assert!(!value.unwrap_boolean());
    }
}
