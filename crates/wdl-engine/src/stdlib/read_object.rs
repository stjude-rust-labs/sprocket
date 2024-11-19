//! Implements the `read_object` function from the WDL standard library.

use std::fs;
use std::io::BufRead;
use std::io::BufReader;

use anyhow::Context;
use indexmap::IndexMap;
use itertools::EitherOrBoth;
use itertools::Itertools;
use wdl_analysis::types::PrimitiveTypeKind;
use wdl_analysis::types::Type;
use wdl_ast::Diagnostic;
use wdl_grammar::lexer::v1::is_ident;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::Object;
use crate::PrimitiveValue;
use crate::Value;
use crate::diagnostics::function_call_failed;

/// Reads a tab-separated value (TSV) file representing the names and values of
/// the members of an Object.
///
/// There must be exactly two rows, and each row must have the same number of
/// elements, otherwise an error is raised.
///
/// Trailing end-of-line characters (\r and \n) are removed from each line.
///
/// The first row specifies the object member names. The names in the first row
/// must be unique; if there are any duplicate names, an error is raised.
///
/// The second row specifies the object member values corresponding to the names
/// in the first row. All of the Object's values are of type String.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#read_object
fn read_object(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert!(context.arguments.len() == 1);
    debug_assert!(context.return_type_eq(Type::Object));

    let path = context.cwd().join(
        context
            .coerce_argument(0, PrimitiveTypeKind::File)
            .unwrap_file()
            .as_str(),
    );

    let expected_two_lines = || {
        function_call_failed(
            "read_object",
            format!(
                "expected exactly two lines in file `{path}`",
                path = path.display()
            ),
            context.call_site,
        )
    };

    let file = fs::File::open(&path)
        .with_context(|| format!("failed to open file `{path}`", path = path.display()))
        .map_err(|e| function_call_failed("read_object", format!("{e:?}"), context.call_site))?;

    let mut lines = BufReader::new(file).lines();
    let names = lines
        .next()
        .ok_or_else(expected_two_lines)?
        .with_context(|| format!("failed to read file `{path}`", path = path.display()))
        .map_err(|e| function_call_failed("read_object", format!("{e:?}"), context.call_site))?;

    let values = lines
        .next()
        .ok_or_else(expected_two_lines)?
        .with_context(|| format!("failed to read file `{path}`", path = path.display()))
        .map_err(|e| function_call_failed("read_object", format!("{e:?}"), context.call_site))?;

    if lines.next().is_some() {
        return Err(expected_two_lines());
    }

    let mut members = IndexMap::new();
    for e in names.split('\t').zip_longest(values.split('\t')) {
        match e {
            EitherOrBoth::Both(name, value) => {
                if !is_ident(name) {
                    return Err(function_call_failed(
                        "read_object",
                        format!(
                            "invalid column name `{name}` at {path}:1: column name must be a \
                             valid WDL identifier",
                            path = path.display()
                        ),
                        context.call_site,
                    ));
                }

                if members
                    .insert(name.to_string(), PrimitiveValue::new_string(value).into())
                    .is_some()
                {
                    return Err(function_call_failed(
                        "read_object",
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
                    "read_object",
                    format!(
                        "line 2 of file `{path}` does not contain the expected number of columns",
                        path = path.display()
                    ),
                    context.call_site,
                ));
            }
        }
    }

    Ok(Object::from(members).into())
}

/// Gets the function describing `read_object`.
pub const fn descriptor() -> Function {
    Function::new(const { &[Signature::new("(File) -> Object", read_object)] })
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use wdl_ast::version::V1;

    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[test]
    fn read_object() {
        let mut env = TestEnv::default();
        env.write_file("empty.tsv", "");
        env.write_file("object.tsv", "foo\tbar\nbaz\tqux");
        env.write_file("too-many-lines.tsv", "foo\tbar\nbaz\tqux\njam\tcakes\n");
        env.write_file("too-few-lines.tsv", "foo\tbar\n");
        env.write_file("too-few-columns.tsv", "foo\tbar\nbaz\n");
        env.write_file("too-many-columns.tsv", "foo\tbar\nbaz\tqux\twrong\n");
        env.write_file("duplicate.tsv", "foo\tbar\tfoo\nbaz\tqux\tfoo\n");
        env.write_file("invalid-name.tsv", "foo\tbar-wrong\tfoo\nbaz\tqux\tfoo\n");

        let diagnostic = eval_v1_expr(&mut env, V1::Two, "read_object('empty.tsv')").unwrap_err();
        assert!(
            diagnostic.message().contains(
                "call to function `read_object` failed: expected exactly two lines in file"
            )
        );

        let diagnostic =
            eval_v1_expr(&mut env, V1::Two, "read_object('too-many-lines.tsv')").unwrap_err();
        assert!(
            diagnostic.message().contains(
                "call to function `read_object` failed: expected exactly two lines in file"
            )
        );

        let diagnostic =
            eval_v1_expr(&mut env, V1::Two, "read_object('too-few-lines.tsv')").unwrap_err();
        assert!(
            diagnostic.message().contains(
                "call to function `read_object` failed: expected exactly two lines in file"
            )
        );

        let diagnostic =
            eval_v1_expr(&mut env, V1::Two, "read_object('too-many-columns.tsv')").unwrap_err();
        assert!(
            diagnostic
                .message()
                .contains("call to function `read_object` failed: line 2 of file")
        );
        assert!(
            diagnostic
                .message()
                .contains("does not contain the expected number of columns")
        );

        let diagnostic =
            eval_v1_expr(&mut env, V1::Two, "read_object('too-few-columns.tsv')").unwrap_err();
        assert!(
            diagnostic
                .message()
                .contains("call to function `read_object` failed: line 2 of file")
        );
        assert!(
            diagnostic
                .message()
                .contains("does not contain the expected number of columns")
        );

        let diagnostic =
            eval_v1_expr(&mut env, V1::Two, "read_object('duplicate.tsv')").unwrap_err();
        assert!(
            diagnostic
                .message()
                .starts_with("call to function `read_object` failed: duplicate column name `foo`")
        );

        let diagnostic =
            eval_v1_expr(&mut env, V1::Two, "read_object('invalid-name.tsv')").unwrap_err();
        assert!(
            diagnostic.message().starts_with(
                "call to function `read_object` failed: invalid column name `bar-wrong`"
            )
        );

        let value = eval_v1_expr(&mut env, V1::Two, "read_object('object.tsv')").unwrap();
        assert_eq!(
            value.unwrap_object().to_string(),
            r#"object {foo: "baz", bar: "qux"}"#
        );
    }
}
