//! Implements the `read_json` function from the WDL standard library.

use std::fs;
use std::io::BufReader;

use anyhow::Context;
use futures::FutureExt;
use futures::future::BoxFuture;
use wdl_analysis::types::PrimitiveType;
use wdl_analysis::types::Type;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::Value;
use crate::diagnostics::function_call_failed;
use crate::stdlib::download_file;

/// The name of the function defined in this file for use in diagnostics.
const FUNCTION_NAME: &str = "read_json";

/// Reads a JSON file into a WDL value whose type depends on the file's
/// contents.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#read_json
fn read_json(context: CallContext<'_>) -> BoxFuture<'_, Result<Value, Diagnostic>> {
    async move {
        debug_assert!(context.arguments.len() == 1);
        debug_assert!(context.return_type_eq(Type::Union));

        let path = context
            .coerce_argument(0, PrimitiveType::File)
            .unwrap_file();

        let file_path = download_file(context.transferer(), context.base_dir(), &path)
            .await
            .map_err(|e| function_call_failed(FUNCTION_NAME, e, context.arguments[0].span))?;

        // Note: `serde-json` does not support asynchronous readers, so we are
        // performing a synchronous read here
        let file = fs::File::open(&file_path)
            .with_context(|| format!("failed to open file `{path}`", path = file_path.display()))
            .map_err(|e| {
                function_call_failed(FUNCTION_NAME, format!("{e:?}"), context.call_site)
            })?;

        serde_json::from_reader::<_, serde_json::Value>(BufReader::new(file))
            .map_err(anyhow::Error::new)
            .and_then(Value::try_from)
            .map_err(|e| {
                function_call_failed(
                    FUNCTION_NAME,
                    format!("failed to read JSON file `{path}`: {e}"),
                    context.call_site,
                )
            })
    }
    .boxed()
}

/// Gets the function describing `read_json`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(file: File) -> Union",
                Callback::Async(read_json),
            )]
        },
    )
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use wdl_ast::version::V1;

    use crate::Value;
    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    fn make_env() -> TestEnv {
        let env = TestEnv::default();
        env.write_file("empty.json", "");
        env.write_file("not-json.json", "not json!");
        env.write_file("null.json", "null");
        env.write_file("true.json", "true");
        env.write_file("false.json", "false");
        env.write_file("string.json", r#""hello\nworld!""#);
        env.write_file("int.json", r#"12345"#);
        env.write_file("float.json", r#"12345.6789"#);
        env.write_file("array.json", "[1, 2, 3]");
        env.write_file("bad_array.json", r#"[1, "2", 3]"#);
        env.write_file(
            "object.json",
            r#"{ "foo": "bar", "bar": 12345, "baz": [1, 2, 3] }"#,
        );
        env
    }

    #[tokio::test]
    async fn read_empty_json() {
        let diagnostic = eval_v1_expr(&make_env(), V1::One, "read_json('empty.json')")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `read_json` failed: failed to read JSON file `empty.json`: EOF \
             while parsing a value at line 1 column 0"
        );
    }

    #[tokio::test]
    async fn read_not_json() {
        let diagnostic = eval_v1_expr(&make_env(), V1::One, "read_json('not-json.json')")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `read_json` failed: failed to read JSON file `not-json.json`: \
             expected ident at line 1 column 2"
        );
    }

    #[tokio::test]
    async fn read_true_json() {
        for file in ["true.json", "https://example.com/true.json"] {
            let value = eval_v1_expr(&make_env(), V1::Two, &format!("read_json('{file}')"))
                .await
                .unwrap();
            assert!(value.unwrap_boolean());
        }
    }

    #[tokio::test]
    async fn read_false_json() {
        let value = eval_v1_expr(&make_env(), V1::One, "read_json('false.json')")
            .await
            .unwrap();
        assert!(!value.unwrap_boolean());
    }

    #[tokio::test]
    async fn read_string_json() {
        let value = eval_v1_expr(&make_env(), V1::One, "read_json('string.json')")
            .await
            .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "hello\nworld!");
    }

    #[tokio::test]
    async fn read_int_json() {
        let value = eval_v1_expr(&make_env(), V1::One, "read_json('int.json')")
            .await
            .unwrap();
        assert_eq!(value.unwrap_integer(), 12345);
    }

    #[tokio::test]
    async fn read_float_json() {
        let value = eval_v1_expr(&make_env(), V1::One, "read_json('float.json')")
            .await
            .unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 12345.6789);
    }

    #[tokio::test]
    async fn read_array_json() {
        let value = eval_v1_expr(&make_env(), V1::One, "read_json('array.json')")
            .await
            .unwrap();
        assert_eq!(
            value
                .unwrap_array()
                .as_slice()
                .iter()
                .cloned()
                .map(Value::unwrap_integer)
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );
    }

    #[tokio::test]
    async fn read_bad_array_json() {
        let diagnostic = eval_v1_expr(&make_env(), V1::One, "read_json('bad_array.json')")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `read_json` failed: failed to read JSON file `bad_array.json`: a \
             common element type does not exist between `Int` and `String`"
        );
    }

    #[tokio::test]
    async fn read_object_json() {
        let value = eval_v1_expr(&make_env(), V1::One, "read_json('object.json')")
            .await
            .unwrap()
            .unwrap_object();
        assert_eq!(
            value.get("foo").unwrap().as_string().unwrap().as_str(),
            "bar"
        );
        assert_eq!(value.get("bar").unwrap().as_integer().unwrap(), 12345);
        assert_eq!(
            value
                .get("baz")
                .unwrap()
                .as_array()
                .unwrap()
                .as_slice()
                .iter()
                .cloned()
                .map(Value::unwrap_integer)
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );
    }
}
