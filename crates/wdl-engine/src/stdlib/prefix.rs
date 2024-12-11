//! Implements the `prefix` function from the WDL standard library.

use wdl_analysis::stdlib::STDLIB as ANALYSIS_STDLIB;
use wdl_analysis::types::PrimitiveTypeKind;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::Array;
use crate::PrimitiveValue;
use crate::Value;

/// Adds a prefix to each element of the input array of primitive values.
///
/// Equivalent to evaluating "~{prefix}~{array[i]}" for each i in
/// range(length(array)).
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#prefix
fn prefix(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 2);
    debug_assert!(context.return_type_eq(ANALYSIS_STDLIB.array_string_type()));

    let prefix = context
        .coerce_argument(0, PrimitiveTypeKind::String)
        .unwrap_string();

    let array = context.arguments[1]
        .value
        .as_array()
        .expect("value should be an array");

    let elements = array
        .as_slice()
        .iter()
        .map(|v| match v {
            Value::None => PrimitiveValue::String(prefix.clone()).into(),
            Value::Primitive(v) => {
                PrimitiveValue::new_string(format!("{prefix}{v}", v = v.raw())).into()
            }
            _ => panic!("expected an array of primitive values"),
        })
        .collect();

    Ok(Array::new_unchecked(context.return_type, elements).into())
}

/// Gets the function describing `prefix`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(String, Array[P]) -> Array[String] where `P`: any primitive type",
                prefix,
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
    fn prefix() {
        let mut env = TestEnv::default();
        let value = eval_v1_expr(&mut env, V1::Zero, "prefix('foo', [1, 2, 3])").unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v.as_string().unwrap().as_str())
            .collect();
        assert_eq!(elements, ["foo1", "foo2", "foo3"]);

        let value = eval_v1_expr(&mut env, V1::Zero, "prefix('foo', [1.0, 1.1, 1.2])").unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v.as_string().unwrap().as_str())
            .collect();
        assert_eq!(elements, ["foo1.000000", "foo1.100000", "foo1.200000"]);

        let value =
            eval_v1_expr(&mut env, V1::Zero, "prefix('foo', ['bar', 'baz', 'qux'])").unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v.as_string().unwrap().as_str())
            .collect();
        assert_eq!(elements, ["foobar", "foobaz", "fooqux"]);

        let value = eval_v1_expr(&mut env, V1::Zero, "prefix('foo', [1, None, 3])").unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v.as_string().unwrap().as_str())
            .collect();
        assert_eq!(elements, ["foo1", "foo", "foo3"]);

        let value = eval_v1_expr(&mut env, V1::One, "prefix('foo', [])").unwrap();
        assert!(value.unwrap_array().is_empty());
    }
}
