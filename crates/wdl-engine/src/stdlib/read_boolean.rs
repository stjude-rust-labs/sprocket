//! Implements the `read_boolean` function from the WDL standard library.

use futures::FutureExt;
use futures::future::BoxFuture;
use tokio::fs;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::Value;
use crate::diagnostics::function_call_failed;
use crate::stdlib::download_file;

/// The name of the function defined in this file for use in diagnostics.
const FUNCTION_NAME: &str = "read_boolean";

/// Reads a file that contains a single line containing only a boolean value and
/// (optional) whitespace.
///
/// If the non-whitespace content of the line is "true" or "false", that value
/// is returned as a Boolean. If the file is empty or does not contain a single
/// boolean, an error is raised. The comparison is case-insensitive.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#read_boolean
fn read_boolean(context: CallContext<'_>) -> BoxFuture<'_, Result<Value, Diagnostic>> {
    async move {
        debug_assert!(context.arguments.len() == 1);
        debug_assert!(context.return_type_eq(PrimitiveType::Boolean));

        let path = context
            .coerce_argument(0, PrimitiveType::File)
            .unwrap_file();

        let file_path = download_file(context.downloader(), context.base_dir(), &path)
            .await
            .map_err(|e| function_call_failed(FUNCTION_NAME, e, context.arguments[0].span))?;

        let read_error = |e: std::io::Error| {
            function_call_failed(
                FUNCTION_NAME,
                format!(
                    "failed to read file `{path}`: {e}",
                    path = file_path.display()
                ),
                context.call_site,
            )
        };

        let invalid_contents = || {
            function_call_failed(
                FUNCTION_NAME,
                format!("file `{path}` does not contain a boolean value on a single line"),
                context.call_site,
            )
        };

        let mut lines =
            BufReader::new(fs::File::open(&file_path).await.map_err(read_error)?).lines();
        let mut line = lines
            .next_line()
            .await
            .map_err(read_error)?
            .ok_or_else(invalid_contents)?;

        if lines.next_line().await.map_err(read_error)?.is_some() {
            return Err(invalid_contents());
        }

        line.make_ascii_lowercase();
        Ok(line
            .trim()
            .parse::<bool>()
            .map_err(|_| invalid_contents())?
            .into())
    }
    .boxed()
}

/// Gets the function describing `read_boolean`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(File) -> Boolean",
                Callback::Async(read_boolean),
            )]
        },
    )
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use wdl_ast::version::V1;

    use crate::PrimitiveValue;
    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[tokio::test]
    async fn read_boolean() {
        let mut env = TestEnv::default();
        env.write_file("foo", "true false hello world!");
        env.write_file("bar", "\t\tTrUe   \n");
        env.write_file("baz", "\t\tfalse   \n");
        env.insert_name("t", PrimitiveValue::new_file("bar"));
        env.insert_name("f", PrimitiveValue::new_file("baz"));

        let diagnostic = eval_v1_expr(&env, V1::Two, "read_boolean('does-not-exist')")
            .await
            .unwrap_err();
        assert!(
            diagnostic
                .message()
                .starts_with("call to function `read_boolean` failed: failed to read file")
        );

        let diagnostic = eval_v1_expr(&env, V1::Two, "read_boolean('foo')")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `read_boolean` failed: file `foo` does not contain a boolean value \
             on a single line"
        );

        for file in ["bar", "https://example.com/bar"] {
            let value = eval_v1_expr(&env, V1::Two, &format!("read_boolean('{file}')"))
                .await
                .unwrap();
            assert!(value.unwrap_boolean());
        }

        let value = eval_v1_expr(&env, V1::Two, "read_boolean(t)")
            .await
            .unwrap();
        assert!(value.unwrap_boolean());

        for file in ["baz", "https://example.com/baz"] {
            let value = eval_v1_expr(&env, V1::Two, &format!("read_boolean('{file}')"))
                .await
                .unwrap();
            assert!(!value.unwrap_boolean());
        }

        let value = eval_v1_expr(&env, V1::Two, "read_boolean(f)")
            .await
            .unwrap();
        assert!(!value.unwrap_boolean());
    }
}
