//! Implements the `split` function from the WDL standard library.

use regex::Regex;
use wdl_analysis::stdlib::STDLIB as ANALYSIS_STDLIB;
use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::Array;
use crate::PrimitiveValue;
use crate::Value;
use crate::diagnostics::function_call_failed;

/// The name of the function defined in this file for use in diagnostics.
const FUNCTION_NAME: &str = "split";

/// Given the two `String` parameters `input` and `delimiter`, this function
/// splits the input string on the provided delimiter and stores the results in
/// a `Array[String]`.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.3/SPEC.md#split
fn split(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 2);
    debug_assert!(context.return_type_eq(ANALYSIS_STDLIB.array_string_type().clone()));

    let input = context
        .coerce_argument(0, PrimitiveType::String)
        .unwrap_string();
    let delimiter = context
        .coerce_argument(1, PrimitiveType::String)
        .unwrap_string();

    let regex = Regex::new(delimiter.as_str())
        .map_err(|e| function_call_failed(FUNCTION_NAME, &e, context.arguments[1].span))?;

    let elements = regex
        .split(input.as_str())
        .map(|s| PrimitiveValue::new_string(s).into())
        .collect::<Vec<_>>();
    Ok(Array::new_unchecked(context.return_type, elements).into())
}

/// Gets the function describing `split`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(input: String, delimiter: String) -> Array[String]",
                Callback::Sync(split),
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
    async fn split() {
        let env = TestEnv::default();
        let diagnostic = eval_v1_expr(&env, V1::Three, "split('foo bar baz', '?')")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `split` failed: regex parse error:\n    ?\n    ^\nerror: repetition \
             operator missing expression"
        );

        let value = eval_v1_expr(&env, V1::Three, "split('hello there world', '.t...e.')")
            .await
            .unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v.as_string().unwrap().as_str())
            .collect();
        assert_eq!(elements, ["hello", "world"]);

        let value = eval_v1_expr(&env, V1::Three, "split('hello world', 'goodbye')")
            .await
            .unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v.as_string().unwrap().as_str())
            .collect();
        assert_eq!(elements, ["hello world"]);

        let value = eval_v1_expr(&env, V1::Three, "split('hello\tBob', '\\t')")
            .await
            .unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v.as_string().unwrap().as_str())
            .collect();
        assert_eq!(elements, ["hello", "Bob"]);

        let value = eval_v1_expr(&env, V1::Three, "split('hello there\nworld', '\\s')")
            .await
            .unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v.as_string().unwrap().as_str())
            .collect();
        assert_eq!(elements, ["hello", "there", "world"]);
    }
}
