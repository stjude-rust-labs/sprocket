//! Implements the `write_lines` function from the WDL standard library.

use std::path::Path;

use futures::FutureExt;
use futures::future::BoxFuture;
use tempfile::NamedTempFile;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::io::BufWriter;
use wdl_analysis::stdlib::STDLIB as ANALYSIS_STDLIB;
use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::PrimitiveValue;
use crate::Value;
use crate::diagnostics::function_call_failed;

/// Writes a file with one line for each element in a Array[String].
///
/// All lines are terminated by the newline (\n) character (following the POSIX
/// standard).
///
/// If the Array is empty, an empty file is written.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#write_lines
fn write_lines(context: CallContext<'_>) -> BoxFuture<'_, Result<Value, Diagnostic>> {
    async move {
        debug_assert!(context.arguments.len() == 1);
        debug_assert!(context.return_type_eq(PrimitiveType::File));

        // Helper for handling errors while writing to the file.
        let write_error = |e: std::io::Error| {
            function_call_failed(
                "write_lines",
                format!("failed to write to temporary file: {e}"),
                context.call_site,
            )
        };

        let lines = context
            .coerce_argument(0, ANALYSIS_STDLIB.array_string_type().clone())
            .unwrap_array();

        // Create a temporary file that will be persisted after writing the lines
        let path = NamedTempFile::with_prefix_in("tmp", context.temp_dir())
            .map_err(|e| {
                function_call_failed(
                    "write_lines",
                    format!("failed to create temporary file: {e}"),
                    context.call_site,
                )
            })?
            .into_temp_path();

        // Re-open the file for asynchronous write
        let file = fs::File::create(&path).await.map_err(|e| {
            function_call_failed(
                "write_lines",
                format!(
                    "failed to open temporary file `{path}`: {e}",
                    path = path.display()
                ),
                context.call_site,
            )
        })?;

        // Write the lines
        let mut writer = BufWriter::new(file);
        for line in lines.as_slice() {
            writer
                .write_all(line.as_string().unwrap().as_bytes())
                .await
                .map_err(write_error)?;
            writer.write_all(b"\n").await.map_err(write_error)?;
        }

        // Flush the writer and drop it
        writer.flush().await.map_err(write_error)?;
        drop(writer);

        let path = path.keep().map_err(|e| {
            function_call_failed(
                "write_lines",
                format!("failed to keep temporary file: {e}"),
                context.call_site,
            )
        })?;

        Ok(
            PrimitiveValue::new_file(path.into_os_string().into_string().map_err(|path| {
                function_call_failed(
                    "write_lines",
                    format!(
                        "path `{path}` cannot be represented as UTF-8",
                        path = Path::new(&path).display()
                    ),
                    context.call_site,
                )
            })?)
            .into(),
        )
    }
    .boxed()
}

/// Gets the function describing `write_lines`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(Array[String]) -> File",
                Callback::Async(write_lines),
            )]
        },
    )
}

#[cfg(test)]
mod test {
    use std::fs;

    use pretty_assertions::assert_eq;
    use wdl_ast::version::V1;

    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[tokio::test]
    async fn write_lines() {
        let env = TestEnv::default();

        let value = eval_v1_expr(&env, V1::Two, "write_lines([])")
            .await
            .unwrap();
        assert!(
            value
                .as_file()
                .expect("should be file")
                .as_str()
                .starts_with(env.temp_dir().to_str().expect("should be UTF-8")),
            "file should be in temp directory"
        );
        assert_eq!(
            fs::read_to_string(value.unwrap_file().as_str()).expect("failed to read file"),
            "",
        );

        let value = eval_v1_expr(&env, V1::Two, "write_lines(['hello', 'world', '!\n', '!'])")
            .await
            .unwrap();
        assert!(
            value
                .as_file()
                .expect("should be file")
                .as_str()
                .starts_with(env.temp_dir().to_str().expect("should be UTF-8")),
            "file should be in temp directory"
        );
        assert_eq!(
            fs::read_to_string(value.unwrap_file().as_str()).expect("failed to read file"),
            "hello\nworld\n!\n\n!\n"
        );
    }
}
