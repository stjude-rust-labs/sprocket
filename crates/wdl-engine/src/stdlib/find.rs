//! Implements the `find` function from the WDL standard library.

use regex::Regex;
use wdl_analysis::types::Optional;
use wdl_analysis::types::PrimitiveType;
use wdl_analysis::types::Type;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::PrimitiveValue;
use crate::Value;
use crate::diagnostics::function_call_failed;

/// The name of the function defined in this file for use in diagnostics.
const FUNCTION_NAME: &str = "find";

/// Given two String parameters `input` and `pattern`, searches for the
/// occurrence of `pattern` within `input` and returns the first match or `None`
/// if there are no matches.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#-find
fn find(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 2);
    debug_assert!(context.return_type_eq(Type::from(PrimitiveType::String).optional()));

    let input = context
        .coerce_argument(0, PrimitiveType::String)
        .unwrap_string();
    let pattern = context
        .coerce_argument(1, PrimitiveType::String)
        .unwrap_string();

    let regex = Regex::new(pattern.as_str())
        .map_err(|e| function_call_failed(FUNCTION_NAME, &e, context.arguments[1].span))?;

    match regex.find(input.as_str()) {
        Some(m) => Ok(PrimitiveValue::new_string(m.as_str()).into()),
        None => Ok(Value::None),
    }
}

/// Gets the function describing `find`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(String, String) -> String?",
                Callback::Sync(find),
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
    async fn find() {
        let env = TestEnv::default();
        let diagnostic = eval_v1_expr(&env, V1::Two, "find('foo bar baz', '?')")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `find` failed: regex parse error:\n    ?\n    ^\nerror: repetition \
             operator missing expression"
        );

        let value = eval_v1_expr(&env, V1::Two, "find('hello world', 'e..o')")
            .await
            .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "ello");

        let value = eval_v1_expr(&env, V1::Two, "find('hello world', 'goodbye')")
            .await
            .unwrap();
        assert!(value.is_none());

        let value = eval_v1_expr(&env, V1::Two, "find('hello\tBob', '\\t')")
            .await
            .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "\t");
    }
}
