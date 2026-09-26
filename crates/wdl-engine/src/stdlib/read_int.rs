//! Implements the `read_int` function from the WDL standard library.

use futures::FutureExt;
use futures::future::BoxFuture;
use tokio::fs;
use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::Value;
use crate::diagnostics::function_call_failed;
use crate::value::parse_integer;

/// The name of the function defined in this file for use in diagnostics.
const FUNCTION_NAME: &str = "read_int";

/// Reads the contents of a file as a `String` and coerces it to an `Int`.
///
/// If the file is empty or its contents cannot be coerced to an `Int`, an
/// error is raised.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#read_int
fn read_int(context: CallContext<'_>) -> BoxFuture<'_, Result<Value, Diagnostic>> {
    async move {
        debug_assert_eq!(context.arguments.len(), 1);
        debug_assert!(context.return_type_eq(PrimitiveType::Integer));

        let path = context
            .coerce_argument(0, PrimitiveType::File)
            .unwrap_file();

        let file_path = context
            .download_file(&path)
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

        let contents = fs::read_to_string(&file_path).await.map_err(read_error)?;
        parse_integer(&contents).map(Into::into).ok_or_else(|| {
            function_call_failed(
                FUNCTION_NAME,
                format!("file `{path}` does not contain an integer value"),
                context.call_site,
            )
        })
    }
    .boxed()
}

/// Gets the function describing `read_int`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(file: File) -> Int",
                Callback::Async(read_int),
            )]
        },
    )
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use wdl_ast::version::V1;

    use crate::PrimitiveValue;
    use crate::v1::tests::TestEnv;
    use crate::v1::tests::eval_v1_expr;

    #[tokio::test]
    async fn read_int() {
        let mut env = TestEnv::default();
        env.write_file("foo", "12345 hello world!");
        env.write_file("bar", "     \t   \t12345   \n");
        env.write_file("baz", "\n12345\n\n");
        env.write_file("qux", "123\n45\n");
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
            "call to function `read_int` failed: file `foo` does not contain an integer value"
        );

        for file in ["bar", "https://example.com/bar"] {
            let value = eval_v1_expr(&env, V1::Two, &format!("read_int('{file}')"))
                .await
                .unwrap();
            assert_eq!(value.unwrap_integer(), 12345);
        }

        let value = eval_v1_expr(&env, V1::Two, "read_int(file)").await.unwrap();
        assert_eq!(value.unwrap_integer(), 12345);

        let value = eval_v1_expr(&env, V1::Two, "read_int('baz')")
            .await
            .unwrap();
        assert_eq!(value.unwrap_integer(), 12345);

        let diagnostic = eval_v1_expr(&env, V1::Two, "read_int('qux')")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `read_int` failed: file `qux` does not contain an integer value"
        );
    }
}
