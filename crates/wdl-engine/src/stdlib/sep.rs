//! Implements the `sep` function from the WDL standard library.

use std::fmt::Write;

use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::PrimitiveValue;
use crate::Value;

/// Concatenates the elements of an array together into a string with the given
/// separator between consecutive elements.
///
/// There are always N-1 separators in the output string, where N is the length
/// of the input array.
///
/// A separator is never added after the last element.
///
/// Returns an empty string if the array is empty.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#sep-1
fn sep(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 2);
    debug_assert!(context.return_type_eq(PrimitiveType::String));

    let sep = context
        .coerce_argument(0, PrimitiveType::String)
        .unwrap_string();

    let array = context.arguments[1]
        .value
        .as_array()
        .expect("value should be an array");

    let s = array
        .as_slice()
        .iter()
        .enumerate()
        .fold(String::new(), |mut s, (i, v)| {
            if i > 0 {
                s.push_str(&sep);
            }

            match v {
                Value::None(_) => {}
                Value::Primitive(v) => write!(&mut s, "{v}", v = v.raw(Some(context.inner())))
                    .expect("failed to write to a string"),
                _ => panic!("expected an array of primitive values"),
            }

            s
        });

    Ok(PrimitiveValue::new_string(s).into())
}

/// Gets the function describing `sep`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(String, Array[P]) -> String where `P`: any primitive type",
                Callback::Sync(sep),
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
    async fn sep() {
        let env = TestEnv::default();
        let value = eval_v1_expr(
            &env,
            V1::One,
            "sep(' ', prefix('-i ', ['file_1', 'file_2']))",
        )
        .await
        .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "-i file_1 -i file_2");

        let value = eval_v1_expr(&env, V1::One, "sep('', ['a', 'b', 'c'])")
            .await
            .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "abc");

        let value = eval_v1_expr(&env, V1::One, "sep(' ', ['a', 'b', 'c'])")
            .await
            .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "a b c");

        let value = eval_v1_expr(&env, V1::One, "sep(' ', ['a', None, 'c'])")
            .await
            .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "a  c");

        let value = eval_v1_expr(&env, V1::One, "sep(',', [1])").await.unwrap();
        assert_eq!(value.unwrap_string().as_str(), "1");

        let value = eval_v1_expr(&env, V1::One, "sep(',', [])").await.unwrap();
        assert_eq!(value.unwrap_string().as_str(), "");
    }
}
