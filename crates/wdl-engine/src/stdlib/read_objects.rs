//! Implements the `read_objects` function from the WDL standard library.

use std::fs;
use std::io::BufRead;
use std::io::BufReader;

use anyhow::Context;
use indexmap::IndexMap;
use itertools::EitherOrBoth;
use itertools::Itertools;
use wdl_analysis::stdlib::STDLIB as ANALYSIS_STDLIB;
use wdl_analysis::types::PrimitiveTypeKind;
use wdl_ast::Diagnostic;
use wdl_grammar::lexer::v1::is_ident;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::Array;
use crate::Object;
use crate::PrimitiveValue;
use crate::Value;
use crate::diagnostics::function_call_failed;

/// Reads a tab-separated value (TSV) file representing the names and values of
/// the members of any number of Objects.
///
/// Trailing end-of-line characters (\r and \n) are removed from each line.
///
/// The first line of the file must be a header row with the names of the object
/// members. The names in the first row must be unique; if there are any
/// duplicate names, an error is raised.
///
/// There are any number of additional rows, where each additional row contains
/// the values of an object corresponding to the member names. Each row in the
/// file must have the same number of fields as the header row. All of the
/// Object's values are of type String.
///
/// If the file is empty or contains only a header line, an empty array is
/// returned.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#read_objects
fn read_objects(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert!(context.arguments.len() == 1);
    debug_assert!(context.return_type_eq(ANALYSIS_STDLIB.array_object_type()));

    let path = context.work_dir().join(
        context
            .coerce_argument(0, PrimitiveTypeKind::File)
            .unwrap_file()
            .as_str(),
    );

    let file = fs::File::open(&path)
        .with_context(|| format!("failed to open file `{path}`", path = path.display()))
        .map_err(|e| function_call_failed("read_objects", format!("{e:?}"), context.call_site))?;

    let mut lines = BufReader::new(file).lines();
    let names = match lines.next() {
        Some(line) => line
            .with_context(|| format!("failed to read file `{path}`", path = path.display()))
            .map_err(|e| {
                function_call_failed("read_objects", format!("{e:?}"), context.call_site)
            })?,
        None => {
            return Ok(
                Array::new_unchecked(ANALYSIS_STDLIB.array_object_type(), Vec::new()).into(),
            );
        }
    };

    for name in names.split('\t') {
        if !is_ident(name) {
            return Err(function_call_failed(
                "read_objects",
                format!(
                    "invalid column name `{name}` at {path}:1: column name must be a valid WDL \
                     identifier",
                    path = path.display()
                ),
                context.call_site,
            ));
        }
    }

    let mut objects = Vec::new();
    for (i, line) in lines.enumerate() {
        let line = line
            .with_context(|| format!("failed to read file `{path}`", path = path.display()))
            .map_err(|e| {
                function_call_failed("read_objects", format!("{e:?}"), context.call_site)
            })?;

        let mut members = IndexMap::new();
        for e in names.split('\t').zip_longest(line.split('\t')) {
            match e {
                EitherOrBoth::Both(name, value) => {
                    if members
                        .insert(name.to_string(), PrimitiveValue::new_string(value).into())
                        .is_some()
                    {
                        return Err(function_call_failed(
                            "read_objects",
                            format!(
                                "duplicate column name `{name}` at {path}:1",
                                path = path.display()
                            ),
                            context.call_site,
                        ));
                    }
                }
                EitherOrBoth::Left(_) | EitherOrBoth::Right(_) => {
                    return Err(function_call_failed(
                        "read_objects",
                        format!(
                            "line {i} of file `{path}` does not contain the expected number of \
                             columns",
                            i = i + 2,
                            path = path.display()
                        ),
                        context.call_site,
                    ));
                }
            }
        }

        objects.push(Object::from(members).into());
    }

    Ok(Array::new_unchecked(ANALYSIS_STDLIB.array_object_type(), objects).into())
}

/// Gets the function describing `read_objects`.
pub const fn descriptor() -> Function {
    Function::new(const { &[Signature::new("(File) -> Array[Object]", read_objects)] })
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use wdl_ast::version::V1;

    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[test]
    fn read_objects() {
        let mut env = TestEnv::default();
        env.write_file("empty.tsv", "");
        env.write_file(
            "objects.tsv",
            "k0\tk1\tk2\na0\ta1\ta2\nb0\tb1\tb2\nc0\tc1\tc2\n",
        );
        env.write_file("only-header.tsv", "foo\tbar\n");
        env.write_file("too-few-columns.tsv", "foo\tbar\nbaz\n");
        env.write_file("too-many-columns.tsv", "foo\tbar\nbaz\tqux\twrong\n");
        env.write_file("duplicate.tsv", "foo\tbar\tfoo\nbaz\tqux\tfoo\n");
        env.write_file("invalid-name.tsv", "foo\tbar-wrong\tfoo\nbaz\tqux\tfoo\n");

        let value = eval_v1_expr(&mut env, V1::Two, "read_objects('empty.tsv')").unwrap();
        assert!(value.unwrap_array().is_empty());

        let value = eval_v1_expr(&mut env, V1::Two, "read_objects('only-header.tsv')").unwrap();
        assert!(value.unwrap_array().is_empty());

        let diagnostic =
            eval_v1_expr(&mut env, V1::Two, "read_objects('too-many-columns.tsv')").unwrap_err();
        assert!(
            diagnostic
                .message()
                .contains("call to function `read_objects` failed: line 2 of file")
        );
        assert!(
            diagnostic
                .message()
                .contains("does not contain the expected number of columns")
        );

        let diagnostic =
            eval_v1_expr(&mut env, V1::Two, "read_objects('too-few-columns.tsv')").unwrap_err();
        assert!(
            diagnostic
                .message()
                .contains("call to function `read_objects` failed: line 2 of file")
        );
        assert!(
            diagnostic
                .message()
                .contains("does not contain the expected number of columns")
        );

        let diagnostic =
            eval_v1_expr(&mut env, V1::Two, "read_objects('duplicate.tsv')").unwrap_err();
        assert!(
            diagnostic
                .message()
                .starts_with("call to function `read_objects` failed: duplicate column name `foo`")
        );

        let diagnostic =
            eval_v1_expr(&mut env, V1::Two, "read_objects('invalid-name.tsv')").unwrap_err();
        assert!(diagnostic.message().starts_with(
            "call to function `read_objects` failed: invalid column name `bar-wrong`"
        ));

        let value = eval_v1_expr(&mut env, V1::Two, "read_objects('objects.tsv')").unwrap();
        assert_eq!(
            value.unwrap_array().to_string(),
            r#"[object {k0: "a0", k1: "a1", k2: "a2"}, object {k0: "b0", k1: "b1", k2: "b2"}, object {k0: "c0", k1: "c1", k2: "c2"}]"#
        );
    }
}
