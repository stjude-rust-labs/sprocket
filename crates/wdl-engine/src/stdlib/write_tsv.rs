//! Implements the `write_tsv` function from the WDL standard library.

use std::path::Path;

use futures::FutureExt;
use futures::future::BoxFuture;
use tempfile::NamedTempFile;
use tokio::fs;
use tokio::io::AsyncWrite;
use tokio::io::AsyncWriteExt;
use tokio::io::BufWriter;
use wdl_analysis::stdlib::STDLIB as ANALYSIS_STDLIB;
use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;
use wdl_ast::Span;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::Array;
use crate::EvaluationContext;
use crate::PrimitiveValue;
use crate::Value;
use crate::diagnostics::function_call_failed;

/// The name of the function defined in this file for use in diagnostics.
const FUNCTION_NAME: &str = "write_tsv";

/// Writes a primitive value as a TSV value.
///
/// Returns `Ok(true)` if the value was written.
///
/// Returns `Ok(false)` if the value contains a tab character.
///
/// Returns `Err(_)` if there was an I/O error.
pub(crate) async fn write_tsv_value<W: AsyncWrite + Unpin>(
    context: &dyn EvaluationContext,
    writer: &mut W,
    value: &PrimitiveValue,
) -> Result<bool, std::io::Error> {
    match value {
        PrimitiveValue::String(v) | PrimitiveValue::File(v) | PrimitiveValue::Directory(v)
            if v.contains('\t') =>
        {
            Ok(false)
        }
        v => {
            writer
                .write_all(v.raw(Some(context)).to_string().as_bytes())
                .await?;
            Ok(true)
        }
    }
}

/// Helper for writing a `Array[Array[String]]` to a TSV file.
async fn write_array_tsv_file(
    tmp: &Path,
    rows: Array,
    header: Option<Array>,
    call_site: Span,
) -> Result<Value, Diagnostic> {
    // Helper for handling errors while writing to the file.
    let write_error = |e: std::io::Error| {
        function_call_failed(
            FUNCTION_NAME,
            format!("failed to write to temporary file: {e}"),
            call_site,
        )
    };

    // Create a temporary file that will be persisted after writing
    let (file, path) = NamedTempFile::with_prefix_in("tmp", tmp)
        .map_err(|e| {
            function_call_failed(
                FUNCTION_NAME,
                format!("failed to create temporary file: {e}"),
                call_site,
            )
        })?
        .into_parts();

    let mut writer = BufWriter::new(fs::File::from(file));

    // Start by writing the header, if one was provided
    let column_count = match header {
        Some(header) => {
            for (i, name) in header.as_slice().iter().enumerate() {
                let name = name.as_string().unwrap();
                if name.contains('\t') {
                    return Err(function_call_failed(
                        FUNCTION_NAME,
                        format!("specified column name at index {i} contains a tab character"),
                        call_site,
                    ));
                }

                if i > 0 {
                    writer.write_all(b"\t").await.map_err(write_error)?;
                }

                writer
                    .write_all(name.as_bytes())
                    .await
                    .map_err(write_error)?;
            }

            writer.write_all(b"\n").await.map_err(write_error)?;
            Some(header.len())
        }
        _ => None,
    };

    // Write the rows
    for (index, row) in rows.as_slice().iter().enumerate() {
        let row = row.as_array().unwrap();
        if let Some(column_count) = column_count
            && row.len() != column_count
        {
            return Err(function_call_failed(
                FUNCTION_NAME,
                format!(
                    "expected {column_count} column{s1} for every row but array at index {index} \
                     has length {len}",
                    s1 = if column_count == 1 { "s" } else { "" },
                    len = row.len(),
                ),
                call_site,
            ));
        }

        for (i, column) in row.as_slice().iter().enumerate() {
            let column = column.as_string().unwrap();
            if column.contains('\t') {
                return Err(function_call_failed(
                    FUNCTION_NAME,
                    format!("element of array at index {index} contains a tab character"),
                    call_site,
                ));
            }

            if i > 0 {
                writer.write_all(b"\t").await.map_err(write_error)?;
            }

            writer
                .write_all(column.as_bytes())
                .await
                .map_err(write_error)?;
        }

        writer.write_all(b"\n").await.map_err(write_error)?;
    }

    // Flush the writer and drop it
    writer.flush().await.map_err(write_error)?;
    drop(writer);

    let path = path.keep().map_err(|e| {
        function_call_failed(
            FUNCTION_NAME,
            format!("failed to keep temporary file: {e}"),
            call_site,
        )
    })?;

    Ok(
        PrimitiveValue::new_file(path.into_os_string().into_string().map_err(|path| {
            function_call_failed(
                FUNCTION_NAME,
                format!(
                    "path `{path}` cannot be represented as UTF-8",
                    path = Path::new(&path).display()
                ),
                call_site,
            )
        })?)
        .into(),
    )
}

/// Given an Array of elements, writes a tab-separated value (TSV) file with one
/// line for each element.
///
/// `File write_tsv(Array[Array[String]])`: Each element is concatenated using a
/// tab ('\t') delimiter and written as a row in the file. There is no header
/// row.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#write_tsv
fn write_tsv(context: CallContext<'_>) -> BoxFuture<'_, Result<Value, Diagnostic>> {
    async move {
        debug_assert!(context.arguments.len() == 1);
        debug_assert!(context.return_type_eq(PrimitiveType::File));

        let rows = context
            .coerce_argument(0, ANALYSIS_STDLIB.array_array_string_type().clone())
            .unwrap_array();

        write_array_tsv_file(context.temp_dir(), rows, None, context.call_site).await
    }
    .boxed()
}

/// Given an Array of elements, writes a tab-separated value (TSV) file with one
/// line for each element.
///
/// `File write_tsv(Array[Array[String]], Boolean, Array[String])`: The second
/// argument must be true and the third argument provides an Array of column
/// names. The column names are concatenated to create a header that is written
/// as the first row of the file. All elements must be the same length as the
/// header array.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#write_tsv
fn write_tsv_with_header(context: CallContext<'_>) -> BoxFuture<'_, Result<Value, Diagnostic>> {
    async move {
        debug_assert!(context.arguments.len() == 3);
        debug_assert!(context.return_type_eq(PrimitiveType::File));

        let rows = context
            .coerce_argument(0, ANALYSIS_STDLIB.array_array_string_type().clone())
            .unwrap_array();
        let write_header = context
            .coerce_argument(1, PrimitiveType::Boolean)
            .unwrap_boolean();
        let header = context
            .coerce_argument(2, ANALYSIS_STDLIB.array_string_type().clone())
            .unwrap_array();

        write_array_tsv_file(
            context.temp_dir(),
            rows,
            if write_header { Some(header) } else { None },
            context.call_site,
        )
        .await
    }
    .boxed()
}

/// Given an Array of elements, writes a tab-separated value (TSV) file with one
/// line for each element.
///
/// `File write_tsv(Array[Struct], [Boolean, [Array[String]]])`: Each element is
/// a struct whose field values are concatenated in the order the fields are
/// defined. The optional second argument specifies whether to write a header
/// row. If it is true, then the header is created from the struct field names.
/// If the second argument is true, then the optional third argument may be used
/// to specify column names to use instead of the struct field names.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#write_tsv
fn write_tsv_struct(context: CallContext<'_>) -> BoxFuture<'_, Result<Value, Diagnostic>> {
    async move {
        debug_assert!(!context.arguments.is_empty() && context.arguments.len() <= 3);
        debug_assert!(context.return_type_eq(PrimitiveType::File));

        // Helper for handling errors while writing to the file.
        let write_error = |e: std::io::Error| {
            function_call_failed(
                FUNCTION_NAME,
                format!("failed to write to temporary file: {e}"),
                context.call_site,
            )
        };

        let rows = context.arguments[0].value.as_array().unwrap();
        let write_header = if context.arguments.len() >= 2 {
            context
                .coerce_argument(1, PrimitiveType::Boolean)
                .unwrap_boolean()
        } else {
            false
        };
        let header = if context.arguments.len() == 3 {
            Some(
                context
                    .coerce_argument(2, ANALYSIS_STDLIB.array_string_type().clone())
                    .unwrap_array(),
            )
        } else {
            None
        };

        // Create a temporary file that will be persisted after writing
        let (file, path) = NamedTempFile::with_prefix_in("tmp", context.temp_dir())
            .map_err(|e| {
                function_call_failed(
                    FUNCTION_NAME,
                    format!("failed to create temporary file: {e}"),
                    context.call_site,
                )
            })?
            .into_parts();

        let mut writer = BufWriter::new(fs::File::from(file));

        // Get the struct type to print the columns; we need to do this even when the
        // array is empty
        let rows_ty = rows.ty();
        let ty = match rows_ty.as_array() {
            Some(ty) => ty
                .element_type()
                .as_struct()
                .expect("should be struct type"),
            _ => panic!("expected an array"),
        };

        // Start by writing the header
        if write_header {
            match header {
                Some(header) => {
                    // Ensure the header count matches the element count
                    if header.len() != ty.members().len() {
                        return Err(function_call_failed(
                            FUNCTION_NAME,
                            format!(
                                "expected {expected} header{s1} as the struct has {expected} \
                                 member{s1}, but only given {actual} header{s2}",
                                expected = ty.members().len(),
                                s1 = if ty.members().len() == 1 { "" } else { "s" },
                                actual = header.len(),
                                s2 = if header.len() == 1 { "" } else { "s" },
                            ),
                            context.arguments[2].span,
                        ));
                    }

                    // Header was explicitly specified, write out the values
                    for (i, name) in header.as_slice().iter().enumerate() {
                        let name = name.as_string().unwrap();
                        if name.contains('\t') {
                            return Err(function_call_failed(
                                FUNCTION_NAME,
                                format!(
                                    "specified column name at index {i} contains a tab character"
                                ),
                                context.call_site,
                            ));
                        }

                        if i > 0 {
                            writer.write_all(b"\t").await.map_err(write_error)?;
                        }

                        writer
                            .write_all(name.as_bytes())
                            .await
                            .map_err(write_error)?;
                    }
                }
                _ => {
                    // Write out the names of each struct member
                    for (i, name) in ty.members().keys().enumerate() {
                        if i > 0 {
                            writer.write_all(b"\t").await.map_err(write_error)?;
                        }

                        writer
                            .write_all(name.as_bytes())
                            .await
                            .map_err(write_error)?;
                    }
                }
            }

            writer.write_all(b"\n").await.map_err(write_error)?;
        }

        // Write the rows
        for row in rows.as_slice() {
            let row = row.as_struct().unwrap();
            for (i, (name, column)) in row.iter().enumerate() {
                if i > 0 {
                    writer.write_all(b"\t").await.map_err(write_error)?;
                }

                match column {
                    Value::None => {}
                    Value::Primitive(v) => {
                        if !write_tsv_value(context.context, &mut writer, v)
                            .await
                            .map_err(write_error)?
                        {
                            return Err(function_call_failed(
                                FUNCTION_NAME,
                                format!("member `{name}` contains a tab character"),
                                context.call_site,
                            ));
                        }
                    }
                    _ => panic!("value is expected to be primitive"),
                }
            }

            writer.write_all(b"\n").await.map_err(write_error)?;
        }

        // Flush the writer and drop it
        writer.flush().await.map_err(write_error)?;
        drop(writer);

        let path = path.keep().map_err(|e| {
            function_call_failed(
                FUNCTION_NAME,
                format!("failed to keep temporary file: {e}"),
                context.call_site,
            )
        })?;

        Ok(
            PrimitiveValue::new_file(path.into_os_string().into_string().map_err(|path| {
                function_call_failed(
                    FUNCTION_NAME,
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

/// Gets the function describing `write_tsv`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[
                Signature::new("(Array[Array[String]]) -> File", Callback::Async(write_tsv)),
                Signature::new(
                    "(Array[Array[String]], Boolean, Array[String]) -> File",
                    Callback::Async(write_tsv_with_header),
                ),
                Signature::new(
                    "(Array[S], <Boolean>, <Array[String]>) -> File where `S`: any structure \
                     containing only primitive types",
                    Callback::Async(write_tsv_struct),
                ),
            ]
        },
    )
}

#[cfg(test)]
mod test {
    use std::fs;

    use pretty_assertions::assert_eq;
    use wdl_analysis::types::Optional;
    use wdl_analysis::types::PrimitiveType;
    use wdl_analysis::types::StructType;
    use wdl_analysis::types::Type;
    use wdl_ast::version::V1;

    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[tokio::test]
    async fn write_tsv() {
        let mut env = TestEnv::default();

        let ty: Type = StructType::new(
            "Foo",
            [
                ("foo", PrimitiveType::Integer.into()),
                ("bar", PrimitiveType::String.into()),
                ("baz", Type::from(PrimitiveType::Boolean).optional()),
            ],
        )
        .into();

        env.insert_struct("Foo", ty);

        let value = eval_v1_expr(&env, V1::Two, "write_tsv([])").await.unwrap();
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
            "write_tsv([['foo'], ['foo', 'bar'], ['foo', 'bar', 'baz']])",
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
            "foo\nfoo\tbar\nfoo\tbar\tbaz\n",
        );

        let value = eval_v1_expr(&env, V1::Two, "write_tsv([], true, ['foo', 'bar', 'baz'])")
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
            "foo\tbar\tbaz\n",
        );

        let diagnostic = eval_v1_expr(
            &env,
            V1::Two,
            "write_tsv([['foo'], ['foo', 'bar'], ['foo', 'bar', 'baz']], true, ['foo', 'bar', \
             'baz'])",
        )
        .await
        .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `write_tsv` failed: expected 3 column for every row but array at \
             index 0 has length 1"
        );

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "write_tsv([['foo'], ['foo', 'bar'], ['foo', 'bar', 'baz']], false, ['foo', 'bar', \
             'baz'])",
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
            "foo\nfoo\tbar\nfoo\tbar\tbaz\n",
        );

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "write_tsv([Foo { foo: 1, bar: 'hi', baz: true }, Foo { foo: 1234, bar: 'there', baz: \
             false }])",
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
            "1\thi\ttrue\n1234\tthere\tfalse\n",
        );

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "write_tsv([Foo { foo: 1, bar: 'hi', baz: false }, Foo { foo: 1234, bar: 'there' }], \
             false)",
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
            "1\thi\tfalse\n1234\tthere\t\n",
        );

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "write_tsv([Foo { foo: 1, bar: 'hi', baz: true }, Foo { foo: 1234, bar: 'there', baz: \
             false }], true)",
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
            "foo\tbar\tbaz\n1\thi\ttrue\n1234\tthere\tfalse\n",
        );

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "write_tsv([Foo { foo: 1, bar: 'hi' }, Foo { foo: 1234, bar: 'there', baz: false }], \
             true, ['qux', 'jam', 'cakes'])",
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
            "qux\tjam\tcakes\n1\thi\t\n1234\tthere\tfalse\n",
        );

        let diagnostic = eval_v1_expr(
            &env,
            V1::Two,
            "write_tsv([Foo { foo: 1, bar: 'hi', baz: true }, Foo { foo: 1234, bar: 'there', baz: \
             false }], true, ['qux'])",
        )
        .await
        .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `write_tsv` failed: expected 3 headers as the struct has 3 members, \
             but only given 1 header"
        );

        let diagnostic = eval_v1_expr(&env, V1::Two, "write_tsv([['\tfoo']])")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `write_tsv` failed: element of array at index 0 contains a tab \
             character"
        );

        let diagnostic = eval_v1_expr(&env, V1::Two, "write_tsv([['foo']], true, ['\tfoo'])")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `write_tsv` failed: specified column name at index 0 contains a tab \
             character"
        );

        let diagnostic = eval_v1_expr(
            &env,
            V1::Two,
            "write_tsv([Foo { foo: 1, bar: 'hi', baz: true }, Foo { foo: 1234, bar: 'there', baz: \
             false }], true, ['foo', '\tbar', 'baz'])",
        )
        .await
        .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `write_tsv` failed: specified column name at index 1 contains a tab \
             character"
        );
    }
}
