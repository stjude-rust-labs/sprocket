//! Implements the `read_string` function from the WDL standard library.

use futures::FutureExt;
use futures::future::BoxFuture;
use tokio::fs;
use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::PrimitiveValue;
use crate::Value;
use crate::diagnostics::function_call_failed;
use crate::stdlib::download_file;

/// The name of the function defined in this file for use in diagnostics.
const FUNCTION_NAME: &str = "read_string";

/// Reads an entire file as a String, with any trailing end-of-line characters
/// (\r and \n) stripped off.
///
/// If the file is empty, an empty string is returned.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#read_string
fn read_string(context: CallContext<'_>) -> BoxFuture<'_, Result<Value, Diagnostic>> {
    async move {
        debug_assert!(context.arguments.len() == 1);
        debug_assert!(context.return_type_eq(PrimitiveType::String));

        let path = context
            .coerce_argument(0, PrimitiveType::File)
            .unwrap_file();

        let file_path = download_file(
            context.context.downloader(),
            context.work_dir(),
            path.as_str(),
        )
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

        let mut contents = fs::read_to_string(&file_path).await.map_err(read_error)?;
        let trimmed = contents.trim_end_matches(['\r', '\n']);
        contents.truncate(trimmed.len());
        Ok(PrimitiveValue::new_string(contents).into())
    }
    .boxed()
}

/// Gets the function describing `read_string`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(File) -> String",
                Callback::Async(read_string),
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
    async fn read_string() {
        let mut env = TestEnv::default();
        env.write_file("foo", "hello\nworld!\n\r\n");
        env.insert_name(
            "file",
            PrimitiveValue::new_file(
                env.work_dir()
                    .unwrap()
                    .join("foo")
                    .unwrap()
                    .unwrap_local()
                    .to_str()
                    .expect("should be UTF-8"),
            ),
        );

        let diagnostic = eval_v1_expr(&env, V1::Two, "read_string('does-not-exist')")
            .await
            .unwrap_err();
        assert!(
            diagnostic
                .message()
                .starts_with("call to function `read_string` failed: failed to read file")
        );

        for file in ["foo", "https://example.com/foo"] {
            let value = eval_v1_expr(&env, V1::Two, &format!("read_string('{file}')"))
                .await
                .unwrap();
            assert_eq!(value.unwrap_string().as_str(), "hello\nworld!");
        }

        let value = eval_v1_expr(&env, V1::Two, "read_string(file)")
            .await
            .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "hello\nworld!");
    }
}
