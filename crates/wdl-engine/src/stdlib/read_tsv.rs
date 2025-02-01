//! Implements the `read_tsv` function from the WDL standard library.

use std::fs;
use std::io::BufRead;
use std::io::BufReader;

use anyhow::Context;
use indexmap::IndexMap;
use itertools::Either;
use itertools::EitherOrBoth;
use itertools::Itertools;
use wdl_analysis::stdlib::STDLIB as ANALYSIS_STDLIB;
use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;
use wdl_grammar::lexer::v1::is_ident;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::Array;
use crate::CompoundValue;
use crate::PrimitiveValue;
use crate::Value;
use crate::diagnostics::function_call_failed;

/// Represents a header in a TSV (tab-separated value) file.
enum TsvHeader {
    /// The header was explicitly specified as an `Array[String]`.
    Specified(Array),
    /// The header was read from the file.
    File(String),
}

impl TsvHeader {
    /// Gets the column names in the header.
    ///
    /// # Panics
    ///
    /// Panics if a specified header contains a value that is not a string.
    pub fn columns(&self) -> impl Iterator<Item = &str> {
        match self {
            Self::Specified(array) => Either::Left(array.as_slice().iter().map(|v| {
                v.as_string()
                    .expect("header value must be a string")
                    .as_str()
            })),
            Self::File(s) => Either::Right(s.split('\t')),
        }
    }
}

/// Reads a tab-separated value (TSV) file as an Array[Array[String]]
/// representing a table of values.
///
/// Trailing end-of-line characters (\r and \n) are removed from each line.
///
/// `Array[Array[String]] read_tsv(File, [false])`: Returns each row of the
/// table as an Array[String]. There is no requirement that the rows of the
/// table are all the same length.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#read_tsv
fn read_tsv_simple(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert!(context.arguments.len() == 1);
    debug_assert!(context.return_type_eq(ANALYSIS_STDLIB.array_array_string_type().clone()));

    let path = context.work_dir().join(
        context
            .coerce_argument(0, PrimitiveType::File)
            .unwrap_file()
            .as_str(),
    );

    let file = fs::File::open(&path)
        .with_context(|| format!("failed to open file `{path}`", path = path.display()))
        .map_err(|e| function_call_failed("read_tsv", format!("{e:?}"), context.call_site))?;

    let mut rows: Vec<Value> = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = line
            .with_context(|| format!("failed to read file `{path}`", path = path.display()))
            .map_err(|e| function_call_failed("read_tsv", format!("{e:?}"), context.call_site))?;
        let values = line
            .split('\t')
            .map(|s| PrimitiveValue::new_string(s).into())
            .collect::<Vec<Value>>();
        rows.push(Array::new_unchecked(ANALYSIS_STDLIB.array_string_type().clone(), values).into());
    }

    Ok(Array::new_unchecked(ANALYSIS_STDLIB.array_array_string_type().clone(), rows).into())
}

/// Reads a tab-separated value (TSV) file as an Array[Object] representing a
/// table of values.
///
/// Trailing end-of-line characters (\r and \n) are removed from each line.
///
/// `Array[Object] read_tsv(File, true)`: The second parameter must be true and
/// specifies that the TSV file contains a header line. Each row is returned as
/// an Object with its keys determined by the header (the first line in the
/// file) and its values as Strings. All rows in the file must be the same
/// length and the field names in the header row must be valid Object field
/// names, or an error is raised.
///
/// `Array[Object] read_tsv(File, Boolean, Array[String])`: The second
/// parameter specifies whether the TSV file contains a header line, and the
/// third parameter is an array of field names that is used to specify the field
/// names to use for the returned Objects. If the second parameter is true, the
/// specified field names override those in the file's header (i.e., the header
/// line is ignored).
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#read_tsv
fn read_tsv(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert!(context.arguments.len() >= 2 && context.arguments.len() <= 3);
    debug_assert!(context.return_type_eq(ANALYSIS_STDLIB.array_object_type().clone()));

    let path = context.work_dir().join(
        context
            .coerce_argument(0, PrimitiveType::File)
            .unwrap_file()
            .as_str(),
    );

    let file = fs::File::open(&path)
        .with_context(|| format!("failed to open file `{path}`", path = path.display()))
        .map_err(|e| function_call_failed("read_tsv", format!("{e:?}"), context.call_site))?;

    let mut lines = BufReader::new(file).lines();

    // Read the file header if there is one; ignore it if the header was directly
    // specified.
    let file_has_header = context
        .coerce_argument(1, PrimitiveType::Boolean)
        .unwrap_boolean();
    let header = if context.arguments.len() == 3 {
        if file_has_header {
            lines.next();
        }

        TsvHeader::Specified(
            context
                .coerce_argument(2, ANALYSIS_STDLIB.array_string_type().clone())
                .unwrap_array(),
        )
    } else if !file_has_header {
        return Err(function_call_failed(
            "read_tsv",
            "argument specifying presence of a file header must be `true`",
            context.arguments[1].span,
        ));
    } else {
        TsvHeader::File(
            lines
                .next()
                .unwrap_or_else(|| Ok(String::default()))
                .with_context(|| format!("failed to read file `{path}`", path = path.display()))
                .map_err(|e| {
                    function_call_failed("read_tsv", format!("{e:?}"), context.call_site)
                })?,
        )
    };

    let mut column_count = 0;
    if let Some(invalid) = header.columns().find(|c| {
        column_count += 1;
        !is_ident(c)
    }) {
        return Err(function_call_failed(
            "read_tsv",
            if context.arguments.len() == 2 {
                format!(
                    "column name `{invalid}` in file `{path}` is not a valid WDL object field name",
                    path = path.display()
                )
            } else {
                format!("specified name `{invalid}` is not a valid WDL object field name")
            },
            context.call_site,
        ));
    }

    let mut rows: Vec<Value> = Vec::new();
    for (index, line) in lines.enumerate() {
        let line = line
            .with_context(|| format!("failed to read file `{path}`", path = path.display()))
            .map_err(|e| function_call_failed("read_tsv", format!("{e:?}"), context.call_site))?;

        let mut members: IndexMap<String, Value> = IndexMap::with_capacity(column_count);

        for e in header.columns().zip_longest(line.split('\t')) {
            match e {
                EitherOrBoth::Both(c, v) => {
                    if members
                        .insert(c.to_string(), PrimitiveValue::new_string(v).into())
                        .is_some()
                    {
                        return Err(function_call_failed(
                            "read_tsv",
                            if context.arguments.len() == 2 {
                                format!(
                                    "duplicate column name `{c}` found in file `{path}`",
                                    path = path.display()
                                )
                            } else {
                                format!("duplicate column name `{c}` was specified")
                            },
                            context.call_site,
                        ));
                    }
                }
                _ => {
                    return Err(function_call_failed(
                        "read_tsv",
                        format!(
                            "line {index} in file `{path}` does not have the expected number of \
                             columns",
                            index = index + 1 + if file_has_header { 1 } else { 0 },
                            path = path.display()
                        ),
                        context.call_site,
                    ));
                }
            }
        }

        rows.push(CompoundValue::Object(members.into()).into());
    }

    Ok(Array::new_unchecked(ANALYSIS_STDLIB.array_object_type().clone(), rows).into())
}

/// Gets the function describing `read_tsv`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[
                Signature::new("(File) -> Array[Array[String]]", read_tsv_simple),
                Signature::new("(File, Boolean) -> Array[Object]", read_tsv),
                Signature::new("(File, Boolean, Array[String]) -> Array[Object]", read_tsv),
            ]
        },
    )
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use wdl_ast::version::V1;

    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[test]
    fn read_tsv() {
        let mut env = TestEnv::default();
        env.write_file(
            "foo.tsv",
            "row1_1\trow1_2\trow1_3\nrow2_1\trow2_2\trow2_3\trow2_4\nrow3_1\trow3_2\n",
        );
        env.write_file(
            "bar.tsv",
            "foo\tbar\tbaz\nrow1_1\trow1_2\trow1_3\nrow2_1\trow2_2\trow2_3\nrow3_1\trow3_2\trow3_3",
        );
        env.write_file(
            "baz.tsv",
            "row1_1\trow1_2\trow1_3\nrow2_1\trow2_2\trow2_3\nrow3_1\trow3_2\trow3_3",
        );
        env.write_file("empty.tsv", "");
        env.write_file("invalid_name.tsv", "invalid-name\nfoo");
        env.write_file(
            "missing_column.tsv",
            "foo\tbar\tbaz\nnrow1_1\trow1_2\trow1_3\nrow2_1\trow2_3\nrow3_1\trow3_2\trow3_3",
        );
        env.write_file(
            "duplicate_column_name.tsv",
            "foo\tbar\tfoo\nrow1_1\trow1_2\trow1_3\nrow2_1\trow2_2\trow2_3\nrow3_1\trow3_2\trow3_3",
        );

        let diagnostic = eval_v1_expr(&mut env, V1::Two, "read_tsv('unknown.tsv')").unwrap_err();
        assert!(
            diagnostic
                .message()
                .contains("call to function `read_tsv` failed: failed to open file")
        );

        let value = eval_v1_expr(&mut env, V1::Two, "read_tsv('empty.tsv')").unwrap();
        assert!(value.unwrap_array().is_empty());

        let diagnostic = eval_v1_expr(&mut env, V1::Two, "read_tsv('foo.tsv', false)").unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `read_tsv` failed: argument specifying presence of a file header \
             must be `true`"
        );

        let value = eval_v1_expr(&mut env, V1::Two, "read_tsv('foo.tsv')").unwrap();
        let elements = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| {
                v.as_array()
                    .unwrap()
                    .as_slice()
                    .iter()
                    .map(|v| v.as_string().unwrap().as_str())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            elements,
            [
                Vec::from_iter(["row1_1", "row1_2", "row1_3"]),
                Vec::from_iter(["row2_1", "row2_2", "row2_3", "row2_4"]),
                Vec::from_iter(["row3_1", "row3_2"])
            ]
        );

        let value = eval_v1_expr(&mut env, V1::Two, "read_tsv('bar.tsv', true)").unwrap();
        let elements = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| {
                v.as_object()
                    .unwrap()
                    .iter()
                    .map(|(k, v)| (k, v.as_string().unwrap().as_str()))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            elements,
            [
                Vec::from_iter([("foo", "row1_1"), ("bar", "row1_2"), ("baz", "row1_3")]),
                Vec::from_iter([("foo", "row2_1"), ("bar", "row2_2"), ("baz", "row2_3")]),
                Vec::from_iter([("foo", "row3_1"), ("bar", "row3_2"), ("baz", "row3_3")]),
            ]
        );

        let value = eval_v1_expr(
            &mut env,
            V1::Two,
            "read_tsv('bar.tsv', true, ['qux', 'jam', 'cakes'])",
        )
        .unwrap();
        let elements = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| {
                v.as_object()
                    .unwrap()
                    .iter()
                    .map(|(k, v)| (k, v.as_string().unwrap().as_str()))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            elements,
            [
                Vec::from_iter([("qux", "row1_1"), ("jam", "row1_2"), ("cakes", "row1_3")]),
                Vec::from_iter([("qux", "row2_1"), ("jam", "row2_2"), ("cakes", "row2_3")]),
                Vec::from_iter([("qux", "row3_1"), ("jam", "row3_2"), ("cakes", "row3_3")]),
            ]
        );

        let diagnostic =
            eval_v1_expr(&mut env, V1::Two, "read_tsv('bar.tsv', true, ['nope'])").unwrap_err();
        assert!(
            diagnostic
                .message()
                .contains("call to function `read_tsv` failed: line 2 in file")
        );
        assert!(
            diagnostic
                .message()
                .contains("does not have the expected number of column")
        );

        let diagnostic =
            eval_v1_expr(&mut env, V1::Two, "read_tsv('missing_column.tsv', true)").unwrap_err();
        assert!(
            diagnostic
                .message()
                .contains("call to function `read_tsv` failed: line 3 in file")
        );
        assert!(
            diagnostic
                .message()
                .contains("does not have the expected number of column")
        );

        let value = eval_v1_expr(
            &mut env,
            V1::Two,
            "read_tsv('baz.tsv', false, ['foo', 'bar', 'baz'])",
        )
        .unwrap();
        let elements = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| {
                v.as_object()
                    .unwrap()
                    .iter()
                    .map(|(k, v)| (k, v.as_string().unwrap().as_str()))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            elements,
            [
                Vec::from_iter([("foo", "row1_1"), ("bar", "row1_2"), ("baz", "row1_3")]),
                Vec::from_iter([("foo", "row2_1"), ("bar", "row2_2"), ("baz", "row2_3")]),
                Vec::from_iter([("foo", "row3_1"), ("bar", "row3_2"), ("baz", "row3_3")]),
            ]
        );

        let diagnostic = eval_v1_expr(
            &mut env,
            V1::Two,
            "read_tsv('missing_column.tsv', true, ['not-valid'])",
        )
        .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `read_tsv` failed: specified name `not-valid` is not a valid WDL \
             object field name"
        );

        let diagnostic = eval_v1_expr(
            &mut env,
            V1::Two,
            "read_tsv('missing_column.tsv', true, ['not-valid'])",
        )
        .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `read_tsv` failed: specified name `not-valid` is not a valid WDL \
             object field name"
        );

        let diagnostic =
            eval_v1_expr(&mut env, V1::Two, "read_tsv('invalid_name.tsv', true)").unwrap_err();
        assert!(
            diagnostic
                .message()
                .contains("call to function `read_tsv` failed: column name `invalid-name`")
        );
        assert!(
            diagnostic
                .message()
                .contains("is not a valid WDL object field name")
        );

        let diagnostic = eval_v1_expr(
            &mut env,
            V1::Two,
            "read_tsv('duplicate_column_name.tsv', true)",
        )
        .unwrap_err();
        assert!(diagnostic.message().contains(
            "call to function `read_tsv` failed: duplicate column name `foo` found in file"
        ));

        let diagnostic = eval_v1_expr(
            &mut env,
            V1::Two,
            "read_tsv('bar.tsv', true, ['foo', 'bar', 'foo'])",
        )
        .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `read_tsv` failed: duplicate column name `foo` was specified"
        );
    }
}
