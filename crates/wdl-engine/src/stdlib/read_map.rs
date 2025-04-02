//! Implements the `read_map` function from the WDL standard library.

use futures::FutureExt;
use futures::future::BoxFuture;
use indexmap::IndexMap;
use tokio::fs;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use wdl_analysis::stdlib::STDLIB as ANALYSIS_STDLIB;
use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::Map;
use crate::PrimitiveValue;
use crate::Value;
use crate::diagnostics::function_call_failed;
use crate::stdlib::download_file;

/// The name of the function defined in this file for use in diagnostics.
const FUNCTION_NAME: &str = "read_map";

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
fn read_map(context: CallContext<'_>) -> BoxFuture<'_, Result<Value, Diagnostic>> {
    async move {
        debug_assert!(context.arguments.len() == 1);
        debug_assert!(context.return_type_eq(ANALYSIS_STDLIB.map_string_string_type().clone()));

        let path = context
            .coerce_argument(0, PrimitiveType::File)
            .unwrap_file();

        let file_path = download_file(
            context.context.downloader(),
            context.work_dir(),
            path.as_str(),
        )
        .await
        .map_err(|e| function_call_failed(FUNCTION_NAME, e, context.arguments[0].span))?;

        let read_error = |e: std::io::Error| {
            function_call_failed(
                FUNCTION_NAME,
                format!(
                    "failed to read file `{path}`: {e}",
                    path = file_path.display()
                ),
                context.call_site,
            )
        };

        let file = fs::File::open(&file_path).await.map_err(read_error)?;

        let mut i = 1;
        let mut lines = BufReader::new(file).lines();
        let mut map: IndexMap<Option<PrimitiveValue>, Value> = IndexMap::new();
        while let Some(line) = lines.next_line().await.map_err(read_error)? {
            let (key, value) = match line.split_once('\t') {
                Some((key, value)) if !value.contains('\t') => (key, value),
                _ => {
                    return Err(function_call_failed(
                        FUNCTION_NAME,
                        format!("line {i} in file `{path}` does not contain exactly two columns",),
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
                    FUNCTION_NAME,
                    format!("line {i} in file `{path}` contains duplicate key name `{key}`",),
                    context.call_site,
                ));
            }

            i += 1;
        }

        Ok(Map::new_unchecked(ANALYSIS_STDLIB.map_string_string_type().clone(), map).into())
    }
    .boxed()
}

/// Gets the function describing `read_map`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(File) -> Map[String, String]",
                Callback::Async(read_map),
            )]
        },
    )
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use wdl_ast::version::V1;

    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[tokio::test]
    async fn read_map() {
        let env = TestEnv::default();
        env.write_file("empty.tsv", "");
        env.write_file("map.tsv", "foo\tbar\nbaz\tqux\njam\tcakes\n");
        env.write_file("wrong.tsv", "foo\tbar\nbaz\tqux\twrong\njam\tcakes\n");
        env.write_file("duplicate.tsv", "foo\tbar\nbaz\tqux\nfoo\tcakes\n");

        let diagnostic = eval_v1_expr(&env, V1::Two, "read_map('wrong.tsv')")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `read_map` failed: line 2 in file `wrong.tsv` does not contain \
             exactly two columns"
        );

        let diagnostic = eval_v1_expr(&env, V1::Two, "read_map('duplicate.tsv')")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `read_map` failed: line 3 in file `duplicate.tsv` contains \
             duplicate key name `foo`"
        );

        let value = eval_v1_expr(&env, V1::Two, "read_map('empty.tsv')")
            .await
            .unwrap();
        assert_eq!(value.unwrap_map().to_string(), "{}");

        for file in ["map.tsv", "https://example.com/map.tsv"] {
            let value = eval_v1_expr(&env, V1::Two, &format!("read_map('{file}')"))
                .await
                .unwrap();

            assert_eq!(
                value.unwrap_map().to_string(),
                r#"{"foo": "bar", "baz": "qux", "jam": "cakes"}"#
            );
        }
    }
}
