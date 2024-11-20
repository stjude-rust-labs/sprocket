//! Implements the `squote` function from the WDL standard library.

use std::sync::Arc;

use wdl_analysis::stdlib::STDLIB as ANALYSIS_STDLIB;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::Array;
use crate::PrimitiveValue;
use crate::Value;

/// Adds single-quotes (') around each element of the input array of primitive
/// values.
///
/// Equivalent to evaluating "'~{array[i]}'" for each i in range(length(array)).
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#squote
fn squote(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 1);
    debug_assert!(context.return_type_eq(ANALYSIS_STDLIB.array_string_type()));

    let array = context.arguments[0]
        .value
        .as_array()
        .expect("value should be an array");

    let elements = array
        .elements()
        .iter()
        .map(|v| match v {
            Value::Primitive(v) => PrimitiveValue::new_string(format!("'{v}'", v = v.raw())).into(),
            _ => panic!("expected an array of primitive values"),
        })
        .collect();

    Ok(Array::new_unchecked(context.return_type, Arc::new(elements)).into())
}

/// Gets the function describing `squote`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(Array[P]) -> Array[String] where `P`: any required primitive type",
                squote,
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
    fn squote() {
        let mut env = TestEnv::default();
        let value = eval_v1_expr(&mut env, V1::One, "squote([1, 2, 3])").unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .elements()
            .iter()
            .map(|v| v.as_string().unwrap().as_str())
            .collect();
        assert_eq!(elements, ["'1'", "'2'", "'3'"]);

        let value = eval_v1_expr(&mut env, V1::One, "squote([1.0, 1.1, 1.2])").unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .elements()
            .iter()
            .map(|v| v.as_string().unwrap().as_str())
            .collect();
        assert_eq!(elements, ["'1.0'", "'1.1'", "'1.2'"]);

        let value = eval_v1_expr(&mut env, V1::One, "squote(['bar', 'baz', 'qux'])").unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .elements()
            .iter()
            .map(|v| v.as_string().unwrap().as_str())
            .collect();
        assert_eq!(elements, ["'bar'", "'baz'", "'qux'"]);

        let value = eval_v1_expr(&mut env, V1::One, "squote([])").unwrap();
        assert!(value.unwrap_array().elements().is_empty());
    }
}
