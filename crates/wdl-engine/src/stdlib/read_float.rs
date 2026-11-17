//! Implements the `read_float` function from the WDL standard library.

use futures::FutureExt;
use futures::future::BoxFuture;
use tokio::fs;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;
use wdl_ast::SupportedVersion;
use wdl_ast::version::V1;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use super::numbers;
use crate::Value;
use crate::diagnostics::function_call_failed;

/// The name of the function defined in this file for use in diagnostics.
const FUNCTION_NAME: &str = "read_float";

/// Reads a file that contains only a float value and (optional) whitespace.
///
/// If the line contains a valid floating point number, that value is returned
/// as a Float. If the file is empty or does not contain a single float, an
/// error is raised.
///
/// Starting with WDL 1.1, the value may have an optional `+` or `-` sign
/// followed by any form accepted for an `Int` or `Float` literal; integer
/// forms are converted to a float and non-finite values are errors.
///
/// <https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#read_float>
fn read_float(context: CallContext<'_>) -> BoxFuture<'_, Result<Value, Diagnostic>> {
    async move {
        debug_assert_eq!(context.arguments.len(), 1);
        debug_assert!(context.return_type_eq(PrimitiveType::Float));

        let path = context
            .coerce_argument(0, PrimitiveType::File)
            .unwrap_file();

        let file_path = context
            .download_file(&path)
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

        let invalid_contents = || {
            function_call_failed(
                FUNCTION_NAME,
                format!("file `{path}` does not contain a float value on a single line"),
                context.call_site,
            )
        };

        let mut lines =
            BufReader::new(fs::File::open(&file_path).await.map_err(read_error)?).lines();
        let line = lines
            .next_line()
            .await
            .map_err(read_error)?
            .ok_or_else(invalid_contents)?;

        if lines.next_line().await.map_err(read_error)?.is_some() {
            return Err(invalid_contents());
        }

        // WDL 1.0 predates the specification of the accepted numeric forms
        let value = if context.inner().version() >= SupportedVersion::V1(V1::One) {
            numbers::parse_float(&line)
        } else {
            line.trim().parse::<f64>().ok()
        };

        Ok(value.ok_or_else(invalid_contents)?.into())
    }
    .boxed()
}

/// Gets the function describing `read_float`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(file: File) -> Float",
                Callback::Async(read_float),
            )]
        },
    )
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use wdl_ast::version::V1;

    use crate::PrimitiveValue;
    use crate::v1::tests::TestEnv;
    use crate::v1::tests::eval_v1_expr;

    #[tokio::test]
    async fn read_float() {
        let mut env = TestEnv::default();
        env.write_file("foo", "12345.6789 hello world!");
        env.write_file("bar", "\t \t 12345.6789   \n");
        env.insert_name("file", PrimitiveValue::new_file("bar"));

        let diagnostic = eval_v1_expr(&env, V1::Two, "read_float('does-not-exist')")
            .await
            .unwrap_err();
        assert!(
            diagnostic
                .message()
                .starts_with("call to function `read_float` failed: failed to read file")
        );

        let diagnostic = eval_v1_expr(&env, V1::Two, "read_float('foo')")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `read_float` failed: file `foo` does not contain a float value on a \
             single line"
        );

        for file in ["bar", "https://example.com/bar"] {
            let value = eval_v1_expr(&env, V1::Two, &format!("read_float('{file}')"))
                .await
                .unwrap();
            approx::assert_relative_eq!(value.unwrap_float(), 12345.6789);
        }

        let value = eval_v1_expr(&env, V1::Two, "read_float(file)")
            .await
            .unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 12345.6789);
    }

    #[tokio::test]
    async fn read_float_literal_forms() {
        let env = TestEnv::default();
        for (file, contents) in [
            ("decimal", "  -12345  \n"),
            ("point", "  +1.25e-3  \n"),
            ("trailing-point", "1."),
            ("leading-point", "-.5"),
            ("hex", "0xcafe"),
            ("octal", "-0o755"),
            ("binary", "0b101010"),
            ("legacy-octal", "0755"),
            ("nan", "NaN"),
            ("infinity", "-inf"),
            ("overflow", "1e309"),
        ] {
            env.write_file(file, contents);
        }

        let read = async |version, file: &str| {
            eval_v1_expr(&env, version, &format!("read_float('{file}')"))
                .await
                .map(|v| v.unwrap_float())
                .map_err(|e| e.message().to_string())
        };

        let invalid = |file: &str| {
            Err(format!(
                "call to function `read_float` failed: file `{file}` does not contain a float \
                 value on a single line"
            ))
        };

        // WDL 1.0 uses the legacy behavior
        assert_eq!(read(V1::Zero, "decimal").await, Ok(-12345.0));
        assert_eq!(read(V1::Zero, "point").await, Ok(0.00125));
        assert_eq!(read(V1::Zero, "legacy-octal").await, Ok(755.0));
        assert_eq!(read(V1::Zero, "infinity").await, Ok(f64::NEG_INFINITY));
        assert_eq!(read(V1::Zero, "hex").await, invalid("hex"));

        for version in [V1::One, V1::Two, V1::Three] {
            assert_eq!(read(version, "decimal").await, Ok(-12345.0));
            assert_eq!(read(version, "point").await, Ok(0.00125));
            assert_eq!(read(version, "trailing-point").await, Ok(1.0));
            assert_eq!(read(version, "leading-point").await, Ok(-0.5));
            assert_eq!(read(version, "hex").await, Ok(51966.0));
            assert_eq!(read(version, "octal").await, Ok(-493.0));
            assert_eq!(read(version, "binary").await, Ok(42.0));
            assert_eq!(read(version, "legacy-octal").await, invalid("legacy-octal"));
            assert_eq!(read(version, "nan").await, invalid("nan"));
            assert_eq!(read(version, "infinity").await, invalid("infinity"));
            assert_eq!(read(version, "overflow").await, invalid("overflow"));
        }
    }
}
