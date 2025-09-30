//! Implements the `read_lines` function from the WDL standard library.

use futures::FutureExt;
use futures::future::BoxFuture;
use tokio::fs;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
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
use crate::stdlib::download_file;

/// The name of the function defined in this file for use in diagnostics.
const FUNCTION_NAME: &str = "read_lines";

/// Reads each line of a file as a String, and returns all lines in the file as
/// an Array[String].
///
/// Trailing end-of-line characters (\r and \n) are removed from each line.
///
/// The order of the lines in the returned Array[String] is the order in which
/// the lines appear in the file.
///
/// If the file is empty, an empty array is returned.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#read_lines
fn read_lines(context: CallContext<'_>) -> BoxFuture<'_, Result<Value, Diagnostic>> {
    async move {
        debug_assert!(context.arguments.len() == 1);
        debug_assert!(context.return_type_eq(ANALYSIS_STDLIB.array_string_type().clone()));

        let path = context
            .coerce_argument(0, PrimitiveType::File)
            .unwrap_file();

        let file_path = download_file(context.transferer(), context.base_dir(), &path)
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

        let file = fs::File::open(&file_path).await.map_err(read_error)?;
        let mut lines = BufReader::new(file).lines();
        let mut elements = Vec::new();
        while let Some(line) = lines.next_line().await.map_err(read_error)? {
            elements.push(PrimitiveValue::new_string(line).into());
        }

        Ok(Array::new_unchecked(context.return_type, elements).into())
    }
    .boxed()
}

/// Gets the function describing `read_lines`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(File) -> Array[String]",
                Callback::Async(read_lines),
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
    async fn read_lines() {
        let mut env = TestEnv::default();
        env.write_file("foo", "\nhello!\nworld!\n\r\nhi!\r\nthere!");
        env.write_file("empty", "");
        env.insert_name("file", PrimitiveValue::new_file("foo"));

        let diagnostic = eval_v1_expr(&env, V1::Two, "read_lines('does-not-exist')")
            .await
            .unwrap_err();
        assert!(
            diagnostic
                .message()
                .starts_with("call to function `read_lines` failed: failed to read file")
        );

        for file in ["foo", "https://example.com/foo"] {
            let value = eval_v1_expr(&env, V1::Two, &format!("read_lines('{file}')"))
                .await
                .unwrap();
            let elements: Vec<_> = value
                .as_array()
                .unwrap()
                .as_slice()
                .iter()
                .map(|v| v.as_string().unwrap().as_str())
                .collect();
            assert_eq!(elements, ["", "hello!", "world!", "", "hi!", "there!"]);
        }

        let value = eval_v1_expr(&env, V1::Two, "read_lines(file)")
            .await
            .unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v.as_string().unwrap().as_str())
            .collect();
        assert_eq!(elements, ["", "hello!", "world!", "", "hi!", "there!"]);

        let value = eval_v1_expr(&env, V1::Two, "read_lines('empty')")
            .await
            .unwrap();
        assert!(value.unwrap_array().is_empty());
    }
}
