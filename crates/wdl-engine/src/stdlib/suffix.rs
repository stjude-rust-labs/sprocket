//! Implements the `suffix` function from the WDL standard library.

use std::sync::Arc;

use wdl_analysis::stdlib::STDLIB as ANALYSIS_STDLIB;
use wdl_analysis::types::PrimitiveTypeKind;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::Array;
use crate::PrimitiveValue;
use crate::Value;

/// Adds a suffix to each element of the input array of primitive values.
///
/// Equivalent to evaluating "~{array[i]}~{suffix}" for each i in
/// range(length(array)).
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#suffix
fn suffix(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 2);
    debug_assert!(context.return_type_eq(ANALYSIS_STDLIB.array_string_type()));

    let suffix = context
        .coerce_argument(0, PrimitiveTypeKind::String)
        .unwrap_string();

    let array = context.arguments[1]
        .value
        .as_array()
        .expect("value should be an array");

    let elements = array
        .elements()
        .iter()
        .map(|v| match v {
            Value::Primitive(v) => {
                PrimitiveValue::new_string(format!("{v}{suffix}", v = v.raw())).into()
            }
            _ => panic!("expected an array of primitive values"),
        })
        .collect();

    Ok(Array::new_unchecked(context.return_type, Arc::new(elements)).into())
}

/// Gets the function describing `suffix`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(String, Array[P]) -> Array[String] where `P`: any required primitive type",
                suffix,
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
    fn suffix() {
        let mut env = TestEnv::default();
        let value = eval_v1_expr(&mut env, V1::One, "suffix('foo', [1, 2, 3])").unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .elements()
            .iter()
            .map(|v| v.as_string().unwrap().as_str())
            .collect();
        assert_eq!(elements, ["1foo", "2foo", "3foo"]);

        let value = eval_v1_expr(&mut env, V1::One, "suffix('foo', [1.0, 1.1, 1.2])").unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .elements()
            .iter()
            .map(|v| v.as_string().unwrap().as_str())
            .collect();
        assert_eq!(elements, ["1.0foo", "1.1foo", "1.2foo"]);

        let value =
            eval_v1_expr(&mut env, V1::One, "suffix('foo', ['bar', 'baz', 'qux'])").unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .elements()
            .iter()
            .map(|v| v.as_string().unwrap().as_str())
            .collect();
        assert_eq!(elements, ["barfoo", "bazfoo", "quxfoo"]);

        let value = eval_v1_expr(&mut env, V1::One, "suffix('foo', [])").unwrap();
        assert!(value.unwrap_array().elements().is_empty());
    }
}
