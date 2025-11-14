//! Implements the `baseline` function from the WDL standard library.

use std::path::Path;

use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::PrimitiveValue;
use crate::Value;
use crate::path;

/// Returns the "basename" of a file or directory - the name after the last
/// directory separator in the path.
///
/// The optional second parameter specifies a literal suffix to remove from the
/// file name. If the file name does not end with the specified suffix then it
/// is ignored.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#basename
fn basename(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    fn remove_suffix<'a>(context: CallContext<'_>, base: &'a str) -> &'a str {
        if context.arguments.len() == 2 {
            base.strip_suffix(
                context
                    .coerce_argument(1, PrimitiveType::String)
                    .unwrap_string()
                    .as_str(),
            )
            .unwrap_or(base)
        } else {
            base
        }
    }

    debug_assert!(!context.arguments.is_empty() && context.arguments.len() < 3);
    debug_assert!(context.return_type_eq(PrimitiveType::String));

    let path = context
        .coerce_argument(0, PrimitiveType::String)
        .unwrap_string();

    if let Some(url) = path::parse_supported_url(&path) {
        let base = url
            .path_segments()
            .and_then(|mut segments| segments.next_back())
            .unwrap_or("");

        return Ok(PrimitiveValue::new_string(remove_suffix(context, base)).into());
    }

    let base = Path::new(path.as_str())
        .file_name()
        .map(|f| f.to_str().expect("should be UTF-8"))
        .unwrap_or("");
    Ok(PrimitiveValue::new_string(remove_suffix(context, base)).into())
}

/// Gets the function describing `basename`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[
                Signature::new(
                    "(path: File, <suffix: String>) -> String",
                    Callback::Sync(basename),
                ),
                Signature::new(
                    "(path: String, <suffix: String>) -> String",
                    Callback::Sync(basename),
                ),
                Signature::new(
                    "(path: Directory, <suffix: String>) -> String",
                    Callback::Sync(basename),
                ),
            ]
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
    async fn basename() {
        let env = TestEnv::default();
        let value = eval_v1_expr(&env, V1::Two, "basename('/path/to/file.txt')")
            .await
            .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "file.txt");

        let value = eval_v1_expr(&env, V1::Two, "basename('/path/to/file.txt', '.txt')")
            .await
            .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "file");

        let value = eval_v1_expr(&env, V1::Two, "basename('/path/to/dir')")
            .await
            .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "dir");

        let value = eval_v1_expr(&env, V1::Two, "basename('file.txt')")
            .await
            .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "file.txt");

        let value = eval_v1_expr(&env, V1::Two, "basename('file.txt', '.txt')")
            .await
            .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "file");

        let value = eval_v1_expr(&env, V1::Two, "basename('file.txt', '.jpg')")
            .await
            .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "file.txt");

        let value = eval_v1_expr(&env, V1::Two, "basename('https://example.com')")
            .await
            .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "");

        let value = eval_v1_expr(&env, V1::Two, "basename('https://example.com/foo')")
            .await
            .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "foo");

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "basename('https://example.com/foo/bar/baz.txt')",
        )
        .await
        .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "baz.txt");

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "basename('https://example.com/foo/bar/baz.txt?foo=baz', '.txt')",
        )
        .await
        .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "baz");

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "basename('https://example.com/foo/bar/baz.txt#hmm', '.jpg')",
        )
        .await
        .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "baz.txt");
    }
}
