//! Implements the `values` function from the WDL standard library.

use wdl_ast::Diagnostic;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::Array;
use crate::Value;

/// Returns an Array of the values from the input Map, in the same order as the
/// elements in the map.
///
/// If the map is empty, an empty array is returned.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#-values
fn values(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 1);
    debug_assert!(
        context.return_type.as_array().is_some(),
        "return type should be an array"
    );

    let elements = context.arguments[0]
        .value
        .as_map()
        .expect("value should be a map")
        .values()
        .cloned()
        .collect();
    Ok(Array::new_unchecked(context.return_type, elements).into())
}

/// Gets the function describing `values`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(Map[K, V]) -> Array[V] where `K`: any primitive type",
                values,
            )]
        },
    )
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use wdl_analysis::types::PrimitiveType;
    use wdl_analysis::types::StructType;
    use wdl_ast::version::V1;

    use crate::Value;
    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[test]
    fn values() {
        let mut env = TestEnv::default();

        let ty = StructType::new("Foo", [
            ("foo", PrimitiveType::Float),
            ("bar", PrimitiveType::String),
            ("baz", PrimitiveType::Integer),
        ]);

        env.insert_struct("Foo", ty);

        let value = eval_v1_expr(&mut env, V1::Two, "values({})").unwrap();
        assert_eq!(value.unwrap_array().len(), 0);

        let value =
            eval_v1_expr(&mut env, V1::Two, "values({'foo': 1, 'bar': 2, 'baz': 3})").unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v.as_integer().unwrap())
            .collect();
        assert_eq!(elements, [1, 2, 3]);

        let value = eval_v1_expr(
            &mut env,
            V1::Two,
            "values({'foo': 1, 'bar': None, 'baz': 3})",
        )
        .unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| match v {
                Value::None => None,
                Value::Primitive(v) => Some(v.as_integer().unwrap()),
                _ => unreachable!("expected an optional primitive value"),
            })
            .collect();
        assert_eq!(elements, [Some(1), None, Some(3)]);
    }
}
