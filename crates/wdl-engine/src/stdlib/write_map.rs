//! Implements the `write_map` function from the WDL standard library.

use std::io::BufWriter;
use std::io::Write;
use std::path::Path;

use tempfile::NamedTempFile;
use wdl_analysis::stdlib::STDLIB as ANALYSIS_STDLIB;
use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::PrimitiveValue;
use crate::Value;
use crate::diagnostics::function_call_failed;

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
fn write_map(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert!(context.arguments.len() == 1);
    debug_assert!(context.return_type_eq(PrimitiveType::File));

    // Helper for handling errors while writing to the file.
    let write_error = |e: std::io::Error| {
        function_call_failed(
            "write_map",
            format!("failed to write to temporary file: {e}"),
            context.call_site,
        )
    };

    let map = context
        .coerce_argument(0, ANALYSIS_STDLIB.map_string_string_type().clone())
        .unwrap_map();

    // Create a temporary file that will be persisted after writing the map
    let mut file = NamedTempFile::with_prefix_in("tmp", context.temp_dir()).map_err(|e| {
        function_call_failed(
            "write_map",
            format!("failed to create temporary file: {e}"),
            context.call_site,
        )
    })?;

    // Write the lines
    let mut writer = BufWriter::new(file.as_file_mut());
    for (key, value) in map.iter() {
        writeln!(
            &mut writer,
            "{key}\t{value}",
            key = key
                .as_ref()
                .expect("key should not be optional")
                .as_string()
                .unwrap(),
            value = value.as_string().unwrap()
        )
        .map_err(write_error)?;
    }

    // Consume the writer, flushing the buffer to disk.
    writer
        .into_inner()
        .map_err(|e| write_error(e.into_error()))?;

    let (_, path) = file.keep().map_err(|e| {
        function_call_failed(
            "write_map",
            format!("failed to keep temporary file: {e}"),
            context.call_site,
        )
    })?;

    Ok(
        PrimitiveValue::new_file(path.into_os_string().into_string().map_err(|path| {
            function_call_failed(
                "write_map",
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

/// Gets the function describing `write_map`.
pub const fn descriptor() -> Function {
    Function::new(const { &[Signature::new("(Map[String, String]) -> File", write_map)] })
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
