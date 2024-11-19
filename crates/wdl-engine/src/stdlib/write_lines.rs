//! Implements the `write_lines` function from the WDL standard library.

use std::io::BufWriter;
use std::io::Write;
use std::path::Path;

use tempfile::NamedTempFile;
use wdl_analysis::stdlib::STDLIB as ANALYSIS_STDLIB;
use wdl_analysis::types::PrimitiveTypeKind;
use wdl_ast::Diagnostic;

use super::CallContext;
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
fn write_lines(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert!(context.arguments.len() == 1);
    debug_assert!(context.return_type_eq(PrimitiveTypeKind::File));

    // Helper for handling errors while writing to the file.
    let write_error = |e: std::io::Error| {
        function_call_failed(
            "write_lines",
            format!("failed to write to temporary file: {e}"),
            context.call_site,
        )
    };

    let lines = context
        .coerce_argument(0, ANALYSIS_STDLIB.array_string_type())
        .unwrap_array();

    // Create a temporary file that will be persisted after writing the lines
    let mut file = NamedTempFile::new_in(context.tmp()).map_err(|e| {
        function_call_failed(
            "write_lines",
            format!("failed to create temporary file: {e}"),
            context.call_site,
        )
    })?;

    // Write the lines
    let mut writer = BufWriter::new(file.as_file_mut());
    for line in lines.elements() {
        writer
            .write(line.as_string().unwrap().as_bytes())
            .map_err(write_error)?;
        writeln!(&mut writer).map_err(write_error)?;
    }

    // Consume the writer, flushing the buffer to disk.
    writer
        .into_inner()
        .map_err(|e| write_error(e.into_error()))?;

    let (_, path) = file.keep().map_err(|e| {
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

/// Gets the function describing `write_lines`.
pub const fn descriptor() -> Function {
    Function::new(const { &[Signature::new("(Array[String]) -> File", write_lines)] })
}

#[cfg(test)]
mod test {
    use std::fs;

    use pretty_assertions::assert_eq;
    use wdl_ast::version::V1;

    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[test]
    fn write_lines() {
        let mut env = TestEnv::default();

        let value = eval_v1_expr(&mut env, V1::Two, "write_lines([])").unwrap();
        assert!(
            value
                .as_file()
                .expect("should be file")
                .as_str()
                .starts_with(env.tmp().to_str().expect("should be UTF-8")),
            "file should be in temp directory"
        );
        assert_eq!(
            fs::read_to_string(value.unwrap_file().as_str()).expect("failed to read file"),
            "",
        );

        let value = eval_v1_expr(
            &mut env,
            V1::Two,
            "write_lines(['hello', 'world', '!\n', '!'])",
        )
        .unwrap();
        assert!(
            value
                .as_file()
                .expect("should be file")
                .as_str()
                .starts_with(env.tmp().to_str().expect("should be UTF-8")),
            "file should be in temp directory"
        );
        assert_eq!(
            fs::read_to_string(value.unwrap_file().as_str()).expect("failed to read file"),
            "hello\nworld\n!\n\n!\n"
        );
    }
}
