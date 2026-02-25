//! Implements the `write_map` function from the WDL standard library.

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
use crate::Value;
use crate::diagnostics::function_call_failed;
use crate::stdlib::temp_path_to_value;

/// The name of the function defined in this file for use in diagnostics.
const FUNCTION_NAME: &str = "write_map";

/// Writes a tab-separated value (TSV) file with one line for each element in a
/// Map[String, String].
///
/// Each element is concatenated into a single tab-delimited string of the
/// format ~{key}\t~{value}.
///
/// Each line is terminated by the newline (\n) character.
///
/// If the Map is empty, an empty file is written.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#write_map
fn write_map(context: CallContext<'_>) -> BoxFuture<'_, Result<Value, Diagnostic>> {
    async move {
        debug_assert!(context.arguments.len() == 1);
        debug_assert!(context.return_type_eq(PrimitiveType::File));

        // Helper for handling errors while writing to the file.
        let write_error = |e: std::io::Error| {
            function_call_failed(
                FUNCTION_NAME,
                format!("failed to write to temporary file: {e}"),
                context.call_site,
            )
        };

        let map = context
            .coerce_argument(0, ANALYSIS_STDLIB.map_string_string_type().clone())
            .unwrap_map();

        // Create a temporary file that will be persisted after writing the map
        let (file, path) = NamedTempFile::with_prefix_in("tmp", context.temp_dir())
            .map_err(|e| {
                function_call_failed(
                    FUNCTION_NAME,
                    format!("failed to create temporary file: {e}"),
                    context.call_site,
                )
            })?
            .into_parts();

        // Write the lines
        let mut writer = BufWriter::new(fs::File::from(file));
        for (key, value) in map.iter() {
            writer
                .write_all(key.as_string().unwrap().as_bytes())
                .await
                .map_err(write_error)?;
            writer.write_all(b"\t").await.map_err(write_error)?;
            writer
                .write_all(value.as_string().unwrap().as_bytes())
                .await
                .map_err(write_error)?;
            writer.write_all(b"\n").await.map_err(write_error)?;
        }

        // Flush the writer and drop it
        writer.flush().await.map_err(write_error)?;
        drop(writer);

        temp_path_to_value(context, path, FUNCTION_NAME)
    }
    .boxed()
}

/// Gets the function describing `write_map`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(map: Map[String, String]) -> File",
                Callback::Async(write_map),
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
    async fn write_map() {
        let env = TestEnv::default();

        let value = eval_v1_expr(&env, V1::Two, "write_map({})").await.unwrap();
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

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "write_map({ 'foo': 'bar', 'bar': 'baz', 'qux': 'jam' })",
        )
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
            "foo\tbar\nbar\tbaz\nqux\tjam\n",
        );
    }
}
