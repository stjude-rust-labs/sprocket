//! Implements the `read_json` function from the WDL standard library.

use std::borrow::Cow;
use std::fs;
use std::io::BufReader;
use std::path::Path;

use anyhow::Context;
use futures::FutureExt;
use futures::future::BoxFuture;
use serde::Deserialize;
use wdl_analysis::types::PrimitiveType;
use wdl_analysis::types::Type;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::Value;
use crate::diagnostics::function_call_failed;

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

        let location = context
            .context
            .downloader()
            .download(&path)
            .await
            .map_err(|e| {
                function_call_failed(
                    "read_json",
                    format!("failed to download file `{path}`: {e:?}"),
                    context.call_site,
                )
            })?;

        let cache_path: Cow<'_, Path> = location
            .as_deref()
            .map(Into::into)
            .unwrap_or_else(|| context.work_dir().join(path.as_str()).into());

        // Note: `serde-json` does not support asynchronous readers, so we are
        // performing a synchronous read here
        let file = fs::File::open(&cache_path)
            .with_context(|| format!("failed to open file `{path}`", path = cache_path.display()))
            .map_err(|e| function_call_failed("read_json", format!("{e:?}"), context.call_site))?;

        let mut deserializer = serde_json::Deserializer::from_reader(BufReader::new(file));
        Value::deserialize(&mut deserializer).map_err(|e| {
            function_call_failed(
                "read_json",
                format!("failed to deserialize JSON file `{path}`: {e}"),
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
                "(File) -> Union",
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

    #[tokio::test]
    async fn read_json() {
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
        env.write_file(
            "bad_object.json",
            r#"{ "foo": "bar", "bar!": 12345, "baz": [1, 2, 3] }"#,
        );

        let diagnostic = eval_v1_expr(&env, V1::One, "read_json('empty.json')")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `read_json` failed: failed to deserialize JSON file `empty.json`: \
             EOF while parsing a value at line 1 column 0"
        );

        let diagnostic = eval_v1_expr(&env, V1::One, "read_json('not-json.json')")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `read_json` failed: failed to deserialize JSON file \
             `not-json.json`: expected ident at line 1 column 2"
        );

        for file in ["true.json", "https://example.com/true.json"] {
            let value = eval_v1_expr(&env, V1::Two, &format!("read_json('{file}')"))
                .await
                .unwrap();
            assert!(value.unwrap_boolean());
        }

        let value = eval_v1_expr(&env, V1::One, "read_json('false.json')")
            .await
            .unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(&env, V1::One, "read_json('string.json')")
            .await
            .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "hello\nworld!");

        let value = eval_v1_expr(&env, V1::One, "read_json('int.json')")
            .await
            .unwrap();
        assert_eq!(value.unwrap_integer(), 12345);

        let value = eval_v1_expr(&env, V1::One, "read_json('float.json')")
            .await
            .unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 12345.6789);

        let value = eval_v1_expr(&env, V1::One, "read_json('array.json')")
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

        let diagnostic = eval_v1_expr(&env, V1::One, "read_json('bad_array.json')")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `read_json` failed: failed to deserialize JSON file \
             `bad_array.json`: a common element type does not exist between `Int` and `String` at \
             line 1 column 11"
        );

        let value = eval_v1_expr(&env, V1::One, "read_json('object.json')")
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

        let diagnostic = eval_v1_expr(&env, V1::One, "read_json('bad_object.json')")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `read_json` failed: failed to deserialize JSON file \
             `bad_object.json`: object key `bar!` is not a valid WDL identifier at line 1 column \
             23",
        );
    }
}
