//! Implements the `read_map` function from the WDL standard library.

use std::fs;
use std::io::BufRead;
use std::io::BufReader;
use std::sync::Arc;

use anyhow::Context;
use indexmap::IndexMap;
use wdl_analysis::stdlib::STDLIB as ANALYSIS_STDLIB;
use wdl_analysis::types::PrimitiveTypeKind;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::Map;
use crate::PrimitiveValue;
use crate::Value;
use crate::diagnostics::function_call_failed;

/// Reads a tab-separated value (TSV) file representing a set of pairs.
///
/// Each row must have exactly two columns, e.g., col1\tcol2.
///
/// Trailing end-of-line characters (\r and \n) are removed from each line.
///
/// Each pair is added to a Map[String, String] in order.
///
/// The values in the first column must be unique; if there are any duplicate
/// keys, an error is raised.
///
/// If the file is empty, an empty map is returned.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#read_map
fn read_map(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert!(context.arguments.len() == 1);
    debug_assert!(context.return_type_eq(ANALYSIS_STDLIB.map_string_string_type()));

    let path = context.cwd().join(
        context
            .coerce_argument(0, PrimitiveTypeKind::File)
            .unwrap_file()
            .as_str(),
    );
    let file = fs::File::open(&path)
        .with_context(|| format!("failed to open file `{path}`", path = path.display()))
        .map_err(|e| function_call_failed("read_map", format!("{e:?}"), context.call_site))?;

    let mut map: IndexMap<Option<PrimitiveValue>, Value> = IndexMap::new();
    for (i, line) in BufReader::new(file).lines().enumerate() {
        let line = line
            .with_context(|| format!("failed to read file `{path}`", path = path.display()))
            .map_err(|e| function_call_failed("read_map", format!("{e:?}"), context.call_site))?;

        let (key, value) = match line.split_once('\t') {
            Some((key, value)) if !value.contains('\t') => (key, value),
            _ => {
                return Err(function_call_failed(
                    "read_map",
                    format!(
                        "line {i} in file `{path}` does not contain exactly two columns",
                        i = i + 1,
                        path = path.display()
                    ),
                    context.call_site,
                ));
            }
        };

        if map
            .insert(
                Some(PrimitiveValue::new_string(key)),
                PrimitiveValue::new_string(value).into(),
            )
            .is_some()
        {
            return Err(function_call_failed(
                "read_map",
                format!(
                    "line {i} in file `{path}` contains duplicate key name `{key}`",
                    i = i + 1,
                    path = path.display()
                ),
                context.call_site,
            ));
        }
    }

    Ok(Map::new_unchecked(ANALYSIS_STDLIB.map_string_string_type(), Arc::new(map)).into())
}

/// Gets the function describing `read_map`.
pub const fn descriptor() -> Function {
    Function::new(const { &[Signature::new("(File) -> Map[String, String]", read_map)] })
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use wdl_ast::version::V1;

    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[test]
    fn read_map() {
        let mut env = TestEnv::default();
        env.write_file("empty.tsv", "");
        env.write_file("map.tsv", "foo\tbar\nbaz\tqux\njam\tcakes\n");
        env.write_file("wrong.tsv", "foo\tbar\nbaz\tqux\twrong\njam\tcakes\n");
        env.write_file("duplicate.tsv", "foo\tbar\nbaz\tqux\nfoo\tcakes\n");

        let diagnostic = eval_v1_expr(&mut env, V1::Two, "read_map('wrong.tsv')").unwrap_err();
        assert!(
            diagnostic
                .message()
                .contains("call to function `read_map` failed: line 2 in file")
        );
        assert!(
            diagnostic
                .message()
                .contains("does not contain exactly two columns")
        );

        let diagnostic = eval_v1_expr(&mut env, V1::Two, "read_map('duplicate.tsv')").unwrap_err();
        assert!(
            diagnostic
                .message()
                .contains("call to function `read_map` failed: line 3 in file")
        );
        assert!(
            diagnostic
                .message()
                .contains("contains duplicate key name `foo`")
        );

        let value = eval_v1_expr(&mut env, V1::Two, "read_map('empty.tsv')").unwrap();
        assert_eq!(value.unwrap_map().to_string(), "{}");

        let value = eval_v1_expr(&mut env, V1::Two, "read_map('map.tsv')").unwrap();
        assert_eq!(
            value.unwrap_map().to_string(),
            r#"{"foo": "bar", "baz": "qux", "jam": "cakes"}"#
        );
    }
}
