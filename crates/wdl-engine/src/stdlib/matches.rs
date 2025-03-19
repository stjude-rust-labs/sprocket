//! Implements the `matches` function from the WDL standard library.

use regex::Regex;
use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::Value;
use crate::diagnostics::invalid_regex;

/// Given two String parameters `input` and `pattern`, tests whether `pattern`
/// matches `input` at least once.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#-matches
fn matches(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 2);
    debug_assert!(context.return_type_eq(PrimitiveType::Boolean));

    let input = context
        .coerce_argument(0, PrimitiveType::String)
        .unwrap_string();
    let pattern = context
        .coerce_argument(1, PrimitiveType::String)
        .unwrap_string();

    let regex =
        Regex::new(pattern.as_str()).map_err(|e| invalid_regex(&e, context.arguments[1].span))?;
    Ok(regex.is_match(input.as_str()).into())
}

/// Gets the function describing `matches`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(String, String) -> Boolean",
                Callback::Sync(matches),
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
    async fn matches() {
        let env = TestEnv::default();
        let diagnostic = eval_v1_expr(&env, V1::Two, "matches('foo bar baz', '?')")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "regex parse error:\n    ?\n    ^\nerror: repetition operator missing expression"
        );

        let value = eval_v1_expr(&env, V1::Two, "matches('hello world', 'e..o')")
            .await
            .unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(&env, V1::Two, "matches('hello world', 'goodbye')")
            .await
            .unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(&env, V1::Two, "matches('hello\tBob', '\\t')")
            .await
            .unwrap();
        assert!(value.unwrap_boolean());
    }
}
