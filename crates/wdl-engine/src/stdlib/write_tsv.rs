//! Implements the `write_tsv` function from the WDL standard library.

use std::io::BufWriter;
use std::io::Write;
use std::path::Path;

use tempfile::NamedTempFile;
use wdl_analysis::stdlib::STDLIB as ANALYSIS_STDLIB;
use wdl_analysis::types::CompoundTypeDef;
use wdl_analysis::types::PrimitiveTypeKind;
use wdl_ast::Diagnostic;
use wdl_ast::Span;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::Array;
use crate::PrimitiveValue;
use crate::Value;
use crate::diagnostics::function_call_failed;

/// Writes a primitive value as a TSV value.
///
/// Returns `Ok(true)` if the value was written.
///
/// Returns `Ok(false)` if the value contains a tab character.
///
/// Returns `Err(_)` if there was an I/O error.
pub(crate) fn write_tsv_value<W: Write>(
    mut writer: &mut W,
    value: &PrimitiveValue,
) -> Result<bool, std::io::Error> {
    match value {
        PrimitiveValue::String(v) | PrimitiveValue::File(v) | PrimitiveValue::Directory(v)
            if v.contains('\t') =>
        {
            Ok(false)
        }
        v => {
            write!(&mut writer, "{v}", v = v.raw())?;
            Ok(true)
        }
    }
}

/// Helper for writing a `Array[Array[String]]` to a TSV file.
fn write_array_tsv_file(
    tmp: &Path,
    rows: Array,
    header: Option<Array>,
    call_site: Span,
) -> Result<Value, Diagnostic> {
    // Helper for handling errors while writing to the file.
    let write_error = |e: std::io::Error| {
        function_call_failed(
            "write_tsv",
            format!("failed to write to temporary file: {e}"),
            call_site,
        )
    };

    // Create a temporary file that will be persisted after writing
    let mut file = NamedTempFile::new_in(tmp).map_err(|e| {
        function_call_failed(
            "write_tsv",
            format!("failed to create temporary file: {e}"),
            call_site,
        )
    })?;

    let mut writer = BufWriter::new(file.as_file_mut());

    // Start by writing the header, if one was provided
    let column_count = if let Some(header) = header {
        for (i, name) in header.elements().iter().enumerate() {
            let name = name.as_string().unwrap();
            if name.contains('\t') {
                return Err(function_call_failed(
                    "write_tsv",
                    format!("specified column name at index {i} contains a tab character"),
                    call_site,
                ));
            }

            if i > 0 {
                writer.write(b"\t").map_err(write_error)?;
            }

            writer.write(name.as_bytes()).map_err(write_error)?;
        }

        writeln!(&mut writer).map_err(write_error)?;
        Some(header.elements().len())
    } else {
        None
    };

    // Write the rows
    for (index, row) in rows.elements().iter().enumerate() {
        let row = row.as_array().unwrap();
        if let Some(column_count) = column_count {
            if row.elements().len() != column_count {
                return Err(function_call_failed(
                    "write_tsv",
                    format!(
                        "expected {column_count} column{s1} for every row but array at index \
                         {index} has length {len}",
                        s1 = if column_count == 1 { "s" } else { "" },
                        len = row.elements().len(),
                    ),
                    call_site,
                ));
            }
        }

        for (i, column) in row.elements().iter().enumerate() {
            let column = column.as_string().unwrap();
            if column.contains('\t') {
                return Err(function_call_failed(
                    "write_tsv",
                    format!("element of array at index {index} contains a tab character"),
                    call_site,
                ));
            }

            if i > 0 {
                writer.write(b"\t").map_err(write_error)?;
            }

            writer.write(column.as_bytes()).map_err(write_error)?;
        }

        writeln!(&mut writer).map_err(write_error)?;
    }

    // Consume the writer, flushing the buffer to disk.
    writer
        .into_inner()
        .map_err(|e| write_error(e.into_error()))?;

    let (_, path) = file.keep().map_err(|e| {
        function_call_failed(
            "write_tsv",
            format!("failed to keep temporary file: {e}"),
            call_site,
        )
    })?;

    Ok(
        PrimitiveValue::new_file(path.into_os_string().into_string().map_err(|path| {
            function_call_failed(
                "write_tsv",
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
fn write_tsv(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert!(context.arguments.len() == 1);
    debug_assert!(context.return_type_eq(PrimitiveTypeKind::File));

    let rows = context
        .coerce_argument(0, ANALYSIS_STDLIB.array_array_string_type())
        .unwrap_array();

    write_array_tsv_file(context.tmp(), rows, None, context.call_site)
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
fn write_tsv_with_header(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert!(context.arguments.len() == 3);
    debug_assert!(context.return_type_eq(PrimitiveTypeKind::File));

    let rows = context
        .coerce_argument(0, ANALYSIS_STDLIB.array_array_string_type())
        .unwrap_array();
    let write_header = context
        .coerce_argument(1, PrimitiveTypeKind::Boolean)
        .unwrap_boolean();
    let header = context
        .coerce_argument(2, ANALYSIS_STDLIB.array_string_type())
        .unwrap_array();

    write_array_tsv_file(
        context.tmp(),
        rows,
        if write_header { Some(header) } else { None },
        context.call_site,
    )
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
fn write_tsv_struct(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert!(!context.arguments.is_empty() && context.arguments.len() <= 3);
    debug_assert!(context.return_type_eq(PrimitiveTypeKind::File));

    // Helper for handling errors while writing to the file.
    let write_error = |e: std::io::Error| {
        function_call_failed(
            "write_tsv",
            format!("failed to write to temporary file: {e}"),
            context.call_site,
        )
    };

    let rows = context.arguments[0].value.as_array().unwrap();
    let write_header = if context.arguments.len() >= 2 {
        context
            .coerce_argument(1, PrimitiveTypeKind::Boolean)
            .unwrap_boolean()
    } else {
        false
    };
    let header = if context.arguments.len() == 3 {
        Some(
            context
                .coerce_argument(2, ANALYSIS_STDLIB.array_string_type())
                .unwrap_array(),
        )
    } else {
        None
    };

    // Create a temporary file that will be persisted after writing
    let mut file = NamedTempFile::new_in(context.tmp()).map_err(|e| {
        function_call_failed(
            "write_tsv",
            format!("failed to create temporary file: {e}"),
            context.call_site,
        )
    })?;

    let mut writer = BufWriter::new(file.as_file_mut());

    // Get the struct type to print the columns; we need to do this even when the
    // array is empty
    let ty = match context
        .types()
        .type_definition(rows.ty().as_compound().unwrap().definition())
    {
        CompoundTypeDef::Array(ty) => context.types().struct_type(ty.element_type()),
        _ => panic!("expected an array"),
    };

    // Start by writing the header
    if write_header {
        if let Some(header) = header {
            // Ensure the header count matches the element count
            if header.elements().len() != ty.members().len() {
                return Err(function_call_failed(
                    "write_tsv",
                    format!(
                        "expected {expected} header{s1} as the struct has {expected} member{s1}, \
                         but only given {actual} header{s2}",
                        expected = ty.members().len(),
                        s1 = if ty.members().len() == 1 { "" } else { "s" },
                        actual = header.elements().len(),
                        s2 = if header.elements().len() == 1 {
                            ""
                        } else {
                            "s"
                        },
                    ),
                    context.arguments[2].span,
                ));
            }

            // Header was explicitly specified, write out the values
            for (i, name) in header.elements().iter().enumerate() {
                let name = name.as_string().unwrap();
                if name.contains('\t') {
                    return Err(function_call_failed(
                        "write_tsv",
                        format!("specified column name at index {i} contains a tab character"),
                        context.call_site,
                    ));
                }

                if i > 0 {
                    writer.write(b"\t").map_err(write_error)?;
                }

                writer.write(name.as_bytes()).map_err(write_error)?;
            }
        } else {
            // Write out the names of each struct member
            for (i, name) in ty.members().keys().enumerate() {
                if i > 0 {
                    writer.write(b"\t").map_err(write_error)?;
                }

                writer.write(name.as_bytes()).map_err(write_error)?;
            }
        }

        writeln!(&mut writer).map_err(write_error)?;
    }

    // Write the rows
    for row in rows.elements() {
        let row = row.as_struct().unwrap();
        for (i, (name, column)) in row.members().iter().enumerate() {
            if i > 0 {
                writer.write(b"\t").map_err(write_error)?;
            }

            match column {
                Value::Primitive(v) => {
                    if !write_tsv_value(&mut writer, v).map_err(write_error)? {
                        return Err(function_call_failed(
                            "write_tsv",
                            format!("member `{name}` contains a tab character"),
                            context.call_site,
                        ));
                    }
                }
                _ => panic!("value is expected to be primitive"),
            }
        }

        writeln!(&mut writer).map_err(write_error)?;
    }

    // Consume the writer, flushing the buffer to disk.
    writer
        .into_inner()
        .map_err(|e| write_error(e.into_error()))?;

    let (_, path) = file.keep().map_err(|e| {
        function_call_failed(
            "write_tsv",
            format!("failed to keep temporary file: {e}"),
            context.call_site,
        )
    })?;

    Ok(
        PrimitiveValue::new_file(path.into_os_string().into_string().map_err(|path| {
            function_call_failed(
                "write_tsv",
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

/// Gets the function describing `write_tsv`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[
                Signature::new("(Array[Array[String]]) -> File", write_tsv),
                Signature::new(
                    "(Array[Array[String]], Boolean, Array[String]) -> File",
                    write_tsv_with_header,
                ),
                Signature::new(
                    "(Array[S], <Boolean>, <Array[String]>) -> File where `S`: any structure \
                     containing only primitive types",
                    write_tsv_struct,
                ),
            ]
        },
    )
}

#[cfg(test)]
mod test {
    use std::fs;

    use pretty_assertions::assert_eq;
    use wdl_analysis::types::PrimitiveTypeKind;
    use wdl_analysis::types::StructType;
    use wdl_ast::version::V1;

    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[test]
    fn write_tsv() {
        let mut env = TestEnv::default();

        let ty = env.types_mut().add_struct(StructType::new("Foo", [
            ("foo", PrimitiveTypeKind::Integer),
            ("bar", PrimitiveTypeKind::String),
            ("baz", PrimitiveTypeKind::Boolean),
        ]));

        env.insert_struct("Foo", ty);

        let value = eval_v1_expr(&mut env, V1::Two, "write_tsv([])").unwrap();
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
            "write_tsv([['foo'], ['foo', 'bar'], ['foo', 'bar', 'baz']])",
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
            "foo\nfoo\tbar\nfoo\tbar\tbaz\n",
        );

        let value = eval_v1_expr(
            &mut env,
            V1::Two,
            "write_tsv([], true, ['foo', 'bar', 'baz'])",
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
            "foo\tbar\tbaz\n",
        );

        let diagnostic = eval_v1_expr(
            &mut env,
            V1::Two,
            "write_tsv([['foo'], ['foo', 'bar'], ['foo', 'bar', 'baz']], true, ['foo', 'bar', \
             'baz'])",
        )
        .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `write_tsv` failed: expected 3 column for every row but array at \
             index 0 has length 1"
        );

        let value = eval_v1_expr(
            &mut env,
            V1::Two,
            "write_tsv([['foo'], ['foo', 'bar'], ['foo', 'bar', 'baz']], false, ['foo', 'bar', \
             'baz'])",
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
            "foo\nfoo\tbar\nfoo\tbar\tbaz\n",
        );

        let value = eval_v1_expr(
            &mut env,
            V1::Two,
            "write_tsv([Foo { foo: 1, bar: 'hi', baz: true }, Foo { foo: 1234, bar: 'there', baz: \
             false }])",
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
            "1\thi\ttrue\n1234\tthere\tfalse\n",
        );

        let value = eval_v1_expr(
            &mut env,
            V1::Two,
            "write_tsv([Foo { foo: 1, bar: 'hi', baz: true }, Foo { foo: 1234, bar: 'there', baz: \
             false }], false)",
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
            "1\thi\ttrue\n1234\tthere\tfalse\n",
        );

        let value = eval_v1_expr(
            &mut env,
            V1::Two,
            "write_tsv([Foo { foo: 1, bar: 'hi', baz: true }, Foo { foo: 1234, bar: 'there', baz: \
             false }], true)",
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
            "foo\tbar\tbaz\n1\thi\ttrue\n1234\tthere\tfalse\n",
        );

        let value = eval_v1_expr(
            &mut env,
            V1::Two,
            "write_tsv([Foo { foo: 1, bar: 'hi', baz: true }, Foo { foo: 1234, bar: 'there', baz: \
             false }], true, ['qux', 'jam', 'cakes'])",
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
            "qux\tjam\tcakes\n1\thi\ttrue\n1234\tthere\tfalse\n",
        );

        let diagnostic = eval_v1_expr(
            &mut env,
            V1::Two,
            "write_tsv([Foo { foo: 1, bar: 'hi', baz: true }, Foo { foo: 1234, bar: 'there', baz: \
             false }], true, ['qux'])",
        )
        .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `write_tsv` failed: expected 3 headers as the struct has 3 members, \
             but only given 1 header"
        );

        let diagnostic = eval_v1_expr(&mut env, V1::Two, "write_tsv([['\tfoo']])").unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `write_tsv` failed: element of array at index 0 contains a tab \
             character"
        );

        let diagnostic =
            eval_v1_expr(&mut env, V1::Two, "write_tsv([['foo']], true, ['\tfoo'])").unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `write_tsv` failed: specified column name at index 0 contains a tab \
             character"
        );

        let diagnostic = eval_v1_expr(
            &mut env,
            V1::Two,
            "write_tsv([Foo { foo: 1, bar: 'hi', baz: true }, Foo { foo: 1234, bar: 'there', baz: \
             false }], true, ['foo', '\tbar', 'baz'])",
        )
        .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `write_tsv` failed: specified column name at index 1 contains a tab \
             character"
        );
    }
}
