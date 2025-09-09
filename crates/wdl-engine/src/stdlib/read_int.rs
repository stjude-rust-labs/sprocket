//! Implements the `read_int` function from the WDL standard library.

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
const FUNCTION_NAME: &str = "read_int";

/// Reads a file that contains a single line containing only an integer and
/// (optional) whitespace.
///
/// If the line contains a valid integer, that value is returned as an Int. If
/// the file is empty or does not contain a single integer, an error is raised.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#read_int
fn read_int(context: CallContext<'_>) -> BoxFuture<'_, Result<Value, Diagnostic>> {
    async move {
        debug_assert!(context.arguments.len() == 1);
        debug_assert!(context.return_type_eq(PrimitiveType::Integer));

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
                format!("file `{path}` does not contain an integer value on a single line"),
                context.call_site,
            )
        };

        let mut lines =
            BufReader::new(fs::File::open(&file_path).await.map_err(read_error)?).lines();
        let line = lines
            .next_line()
            .await
            .map_err(read_error)?
            .ok_or_else(invalid_contents)?;

        if lines.next_line().await.map_err(read_error)?.is_some() {
            return Err(invalid_contents());
        }

        Ok(line
            .trim()
            .parse::<i64>()
            .map_err(|_| invalid_contents())?
            .into())
    }
    .boxed()
}

/// Gets the function describing `read_int`.
pub const fn descriptor() -> Function {
    Function::new(const { &[Signature::new("(File) -> Int", Callback::Async(read_int))] })
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use wdl_ast::version::V1;

    use crate::PrimitiveValue;
    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[tokio::test]
    async fn read_int() {
        let mut env = TestEnv::default();
        env.write_file("foo", "12345 hello world!");
        env.write_file("bar", "     \t   \t12345   \n");
        env.insert_name("file", PrimitiveValue::new_file("bar"));

        let diagnostic = eval_v1_expr(&env, V1::Two, "read_int('does-not-exist')")
            .await
            .unwrap_err();
        assert!(
            diagnostic
                .message()
                .starts_with("call to function `read_int` failed: failed to read file")
        );

        let diagnostic = eval_v1_expr(&env, V1::Two, "read_int('foo')")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `read_int` failed: file `foo` does not contain an integer value on \
             a single line"
        );

        for file in ["bar", "https://example.com/bar"] {
            let value = eval_v1_expr(&env, V1::Two, &format!("read_int('{file}')"))
                .await
                .unwrap();
            assert_eq!(value.unwrap_integer(), 12345);
        }

        let value = eval_v1_expr(&env, V1::Two, "read_int(file)").await.unwrap();
        assert_eq!(value.unwrap_integer(), 12345);
    }
}
