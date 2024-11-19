//! Implements the `read_json` function from the WDL standard library.

use std::fs;
use std::io::BufReader;

use anyhow::Context;
use wdl_analysis::types::PrimitiveTypeKind;
use wdl_analysis::types::Type;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::Value;
use crate::diagnostics::function_call_failed;

/// Reads a JSON file into a WDL value whose type depends on the file's
/// contents.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#read_json
fn read_json(mut context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert!(context.arguments.len() == 1);
    debug_assert!(context.return_type_eq(Type::Union));

    let path = context.cwd().join(
        context
            .coerce_argument(0, PrimitiveTypeKind::File)
            .unwrap_file()
            .as_str(),
    );
    let file = fs::File::open(&path)
        .with_context(|| format!("failed to open file `{path}`", path = path.display()))
        .map_err(|e| function_call_failed("read_json", format!("{e:?}"), context.call_site))?;

    let mut deserializer = serde_json::Deserializer::from_reader(BufReader::new(file));
    Value::deserialize(context.types_mut(), &mut deserializer).map_err(|e| {
        function_call_failed(
            "read_json",
            format!(
                "failed to read JSON file `{path}`: {e}",
                path = path.display()
            ),
            context.call_site,
        )
    })
}

/// Gets the function describing `read_json`.
pub const fn descriptor() -> Function {
    Function::new(const { &[Signature::new("(File) -> Union", read_json)] })
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use wdl_ast::version::V1;

    use crate::Value;
    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[test]
    fn read_json() {
        let mut env = TestEnv::default();
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

        let diagnostic = eval_v1_expr(&mut env, V1::One, "read_json('empty.json')").unwrap_err();
        assert!(
            diagnostic
                .message()
                .contains("call to function `read_json` failed: failed to read JSON file")
        );
        assert!(
            diagnostic
                .message()
                .contains("EOF while parsing a value at line 1 column 0")
        );

        let diagnostic = eval_v1_expr(&mut env, V1::One, "read_json('not-json.json')").unwrap_err();
        assert!(
            diagnostic
                .message()
                .contains("call to function `read_json` failed: failed to read JSON file")
        );
        assert!(
            diagnostic
                .message()
                .contains("expected ident at line 1 column 2")
        );

        let value = eval_v1_expr(&mut env, V1::One, "read_json('true.json')").unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(&mut env, V1::One, "read_json('false.json')").unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(&mut env, V1::One, "read_json('string.json')").unwrap();
        assert_eq!(value.unwrap_string().as_str(), "hello\nworld!");

        let value = eval_v1_expr(&mut env, V1::One, "read_json('int.json')").unwrap();
        assert_eq!(value.unwrap_integer(), 12345);

        let value = eval_v1_expr(&mut env, V1::One, "read_json('float.json')").unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 12345.6789);

        let value = eval_v1_expr(&mut env, V1::One, "read_json('array.json')").unwrap();
        assert_eq!(
            value
                .unwrap_array()
                .elements()
                .iter()
                .cloned()
                .map(Value::unwrap_integer)
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );

        let diagnostic =
            eval_v1_expr(&mut env, V1::One, "read_json('bad_array.json')").unwrap_err();
        assert!(
            diagnostic
                .message()
                .contains("call to function `read_json` failed: failed to read JSON file")
        );
        assert!(
            diagnostic
                .message()
                .contains("a common element type does not exist between `Int` and `String`")
        );

        let value = eval_v1_expr(&mut env, V1::One, "read_json('object.json')")
            .unwrap()
            .unwrap_object();
        assert_eq!(value.members()["foo"].as_string().unwrap().as_str(), "bar");
        assert_eq!(value.members()["bar"].as_integer().unwrap(), 12345);
        assert_eq!(
            value.members()["baz"]
                .as_array()
                .unwrap()
                .elements()
                .iter()
                .cloned()
                .map(Value::unwrap_integer)
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );

        let diagnostic =
            eval_v1_expr(&mut env, V1::One, "read_json('bad_object.json')").unwrap_err();
        assert!(
            diagnostic
                .message()
                .contains("call to function `read_json` failed: failed to read JSON file")
        );
        assert!(
            diagnostic
                .message()
                .contains("object key `bar!` is not a valid WDL identifier")
        );
    }
}
