//! Implements the `sub` function from the WDL standard library.

use std::borrow::Cow;

use regex::Regex;
use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::PrimitiveValue;
use crate::Value;
use crate::diagnostics::function_call_failed;

/// The name of the function defined in this file for use in diagnostics.
const FUNCTION_NAME: &str = "sub";

/// Converts a WDL replacement string to Rust regex replacement syntax.
///
/// WDL follows POSIX ERE/`sed` conventions for backreferences (`\1`-`\9`) while
/// the Rust regex crate uses `$1`-`$9`.
///
/// Literal `$` characters are escaped to `$$` to prevent unintended
/// backreference interpretation (e.g., `$100` in WDL should produce a literal
/// `$100`, not capture group 1 followed by `00`).
fn convert_replacement(s: &str) -> Cow<'_, str> {
    if !needs_conversion(s) {
        return Cow::Borrowed(s);
    }

    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.peek() {
                Some('1'..='9') => {
                    result.push('$');
                    result.push(chars.next().unwrap());
                }
                Some('\\') => {
                    result.push('\\');
                    chars.next();
                }
                _ => result.push('\\'),
            },
            '$' => result.push_str("$$"),
            _ => result.push(c),
        }
    }

    Cow::Owned(result)
}

/// Returns true if the string contains characters requiring conversion.
fn needs_conversion(s: &str) -> bool {
    let mut iter = s.bytes().peekable();
    while let Some(c) = iter.next() {
        match c {
            b'\\' => match iter.peek() {
                Some(b'1'..=b'9') => return true,
                _ => {
                    iter.next();
                    continue;
                }
            },
            b'$' => return true,
            _ => continue,
        }
    }

    false
}

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

    let regex = Regex::new(pattern.as_str())
        .map_err(|e| function_call_failed(FUNCTION_NAME, &e, context.arguments[1].span))?;
    let converted = convert_replacement(replacement.as_str());
    match regex.replace_all(input.as_str(), converted.as_ref()) {
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
    Function::new(
        const {
            &[Signature::new(
                "(input: String, pattern: String, replace: String) -> String",
                Callback::Sync(sub),
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
    async fn sub() {
        let env = TestEnv::default();
        let diagnostic = eval_v1_expr(&env, V1::Two, "sub('foo bar baz', '?', 'nope')")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `sub` failed: regex parse error:\n    ?\n    ^\nerror: repetition \
             operator missing expression"
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

        let value = eval_v1_expr(&env, V1::Two, "sub('hello there world', ' ', '_')")
            .await
            .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "hello_there_world");

        let value = eval_v1_expr(&env, V1::Two, "sub('ab', '(a)(b)', '\\2\\1')")
            .await
            .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "ba");

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "sub('when chocolate', '([^ ]+) ([^ ]+)', '\\2, \\1?')",
        )
        .await
        .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "chocolate, when?");

        let value = eval_v1_expr(&env, V1::Two, "sub('hello', 'hello', '\\\\world')")
            .await
            .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "\\world");

        let value = eval_v1_expr(&env, V1::Two, "sub('hello', 'hello', '$100')")
            .await
            .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "$100");

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "sub('price 50', '(\\w+) (\\d+)', '\\2 \\1 = $\\2')",
        )
        .await
        .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "50 price = $50");
    }
}
