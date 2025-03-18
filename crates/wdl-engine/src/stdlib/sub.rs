//! Implements the `sub` function from the WDL standard library.

use std::borrow::Cow;

use regex::Regex;
use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::PrimitiveValue;
use crate::Value;
use crate::diagnostics::invalid_regex;

/// Given three String parameters `input`, `pattern`, and `replace`, this
/// function replaces all non-overlapping occurrences of `pattern` in `input`
/// with `replace`.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#sub
fn sub(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 3);
    debug_assert!(context.return_type_eq(PrimitiveType::String));

    let input = context
        .coerce_argument(0, PrimitiveType::String)
        .unwrap_string();
    let pattern = context
        .coerce_argument(1, PrimitiveType::String)
        .unwrap_string();
    let replacement = context
        .coerce_argument(2, PrimitiveType::String)
        .unwrap_string();

    let regex =
        Regex::new(pattern.as_str()).map_err(|e| invalid_regex(&e, context.arguments[1].span))?;
    match regex.replace(input.as_str(), replacement.as_str()) {
        Cow::Borrowed(_) => {
            // No replacements, just return the input
            Ok(PrimitiveValue::String(input).into())
        }
        Cow::Owned(s) => {
            // A replacement occurred, allocate a new string
            Ok(PrimitiveValue::new_string(s).into())
        }
    }
}

/// Gets the function describing `sub`.
pub const fn descriptor() -> Function {
    Function::new(const { &[Signature::new("(String, String, String) -> String", sub)] })
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use wdl_ast::version::V1;

    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[tokio::test]
    async fn sub() {
        let env = TestEnv::default();
        let diagnostic = eval_v1_expr(&env, V1::Two, "sub('foo bar baz', '?', 'nope')")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "regex parse error:\n    ?\n    ^\nerror: repetition operator missing expression"
        );

        let value = eval_v1_expr(&env, V1::Two, "sub('hello world', 'e..o', 'ey there')")
            .await
            .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "hey there world");

        let value = eval_v1_expr(&env, V1::Two, "sub('hello world', 'goodbye', 'nope')")
            .await
            .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "hello world");

        let value = eval_v1_expr(&env, V1::Two, "sub('hello\tBob', '\\t', ' ')")
            .await
            .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "hello Bob");
    }
}
