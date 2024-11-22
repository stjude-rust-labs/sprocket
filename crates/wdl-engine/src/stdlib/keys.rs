//! Implements the `keys` function from the WDL standard library.

use std::sync::Arc;

use wdl_ast::Diagnostic;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::Array;
use crate::CompoundValue;
use crate::Object;
use crate::PrimitiveValue;
use crate::Struct;
use crate::Value;

/// Given a key-value type collection (Map, Struct, or Object), returns an Array
/// of the keys from the input collection, in the same order as the elements in
/// the collection.
///
/// When the argument is a Struct, the returned array will contain the keys in
/// the same order they appear in the struct definition.
///
/// When the argument is an Object, the returned array has no guaranteed order.
///
/// When the input Map or Object is empty, an empty array is returned.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#keys
fn keys(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 1);
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
        "return type should be an array"
    );

    let elements = match &context.arguments[0].value {
        Value::Compound(CompoundValue::Map(map)) => {
            map.elements().keys().map(|k| k.clone().into()).collect()
        }
        Value::Compound(CompoundValue::Object(Object { members, .. }))
        | Value::Compound(CompoundValue::Struct(Struct { members, .. })) => members
            .keys()
            .map(|k| PrimitiveValue::new_string(k).into())
            .collect(),
        _ => unreachable!("expected a map, object, or struct"),
    };

    Ok(Array::new_unchecked(context.return_type, Arc::new(elements)).into())
}

/// Gets the function describing `keys`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[
                Signature::new(
                    "(Map[K, V]) -> Array[K] where `K`: any primitive type",
                    keys,
                ),
                Signature::new("(S) -> Array[String] where `S`: any structure", keys),
                Signature::new("(Object) -> Array[String]", keys),
            ]
        },
    )
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use wdl_analysis::types::PrimitiveTypeKind;
    use wdl_analysis::types::StructType;
    use wdl_ast::version::V1;

    use crate::Value;
    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[test]
    fn keys() {
        let mut env = TestEnv::default();

        let ty = env.types_mut().add_struct(StructType::new("Foo", [
            ("foo", PrimitiveTypeKind::Float),
            ("bar", PrimitiveTypeKind::String),
            ("baz", PrimitiveTypeKind::Integer),
        ]));

        env.insert_struct("Foo", ty);

        let value = eval_v1_expr(&mut env, V1::One, "keys({})").unwrap();
        assert_eq!(value.unwrap_array().len(), 0);

        let value =
            eval_v1_expr(&mut env, V1::One, "keys({'foo': 1, 'bar': 2, 'baz': 3})").unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .elements()
            .iter()
            .map(|v| v.as_string().unwrap().as_str())
            .collect();
        assert_eq!(elements, ["foo", "bar", "baz"]);

        let value = eval_v1_expr(&mut env, V1::One, "keys({'foo': 1, None: 2, 'baz': 3})").unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .elements()
            .iter()
            .map(|v| match v {
                Value::None => None,
                Value::Primitive(v) => Some(v.as_string().unwrap().as_str()),
                _ => unreachable!("expected an optional primitive value"),
            })
            .collect();
        assert_eq!(elements, [Some("foo"), None, Some("baz")]);

        let value =
            eval_v1_expr(&mut env, V1::Two, "keys(object { foo: 1, bar: 2, baz: 3})").unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .elements()
            .iter()
            .map(|v| v.as_string().unwrap().as_str())
            .collect();
        assert_eq!(elements, ["foo", "bar", "baz"]);

        let value =
            eval_v1_expr(&mut env, V1::Two, "keys(Foo { foo: 1.0, bar: '2', baz: 3})").unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .elements()
            .iter()
            .map(|v| v.as_string().unwrap().as_str())
            .collect();
        assert_eq!(elements, ["foo", "bar", "baz"]);
    }
}
