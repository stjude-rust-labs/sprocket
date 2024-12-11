//! Implements the `read_boolean` function from the WDL standard library.

use std::fs;
use std::io::BufRead;
use std::io::BufReader;

use wdl_analysis::types::PrimitiveTypeKind;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::Value;
use crate::diagnostics::function_call_failed;

/// Reads a file that contains a single line containing only a boolean value and
/// (optional) whitespace.
///
/// If the non-whitespace content of the line is "true" or "false", that value
/// is returned as a Boolean. If the file is empty or does not contain a single
/// boolean, an error is raised. The comparison is case-insensitive.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#read_boolean
fn read_boolean(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert!(context.arguments.len() == 1);
    debug_assert!(context.return_type_eq(PrimitiveTypeKind::Boolean));

    let path = context.work_dir().join(
        context
            .coerce_argument(0, PrimitiveTypeKind::File)
            .unwrap_file()
            .as_str(),
    );

    let read_error = |e: std::io::Error| {
        function_call_failed(
            "read_boolean",
            format!("failed to read file `{path}`: {e}", path = path.display()),
            context.call_site,
        )
    };

    let invalid_contents = || {
        function_call_failed(
            "read_boolean",
            format!(
                "file `{path}` does not contain a boolean value on a single line",
                path = path.display()
            ),
            context.call_site,
        )
    };

    let mut lines = BufReader::new(fs::File::open(&path).map_err(read_error)?).lines();
    let mut line = lines
        .next()
        .ok_or_else(invalid_contents)?
        .map_err(read_error)?;

    if lines.next().is_some() {
        return Err(invalid_contents());
    }

    line.make_ascii_lowercase();
    Ok(line
        .trim()
        .parse::<bool>()
        .map_err(|_| invalid_contents())?
        .into())
}

/// Gets the function describing `read_boolean`.
pub const fn descriptor() -> Function {
    Function::new(const { &[Signature::new("(File) -> Boolean", read_boolean)] })
}

#[cfg(test)]
mod test {
    use wdl_ast::version::V1;

    use crate::PrimitiveValue;
    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[test]
    fn read_boolean() {
        let mut env = TestEnv::default();
        env.write_file("foo", "true false hello world!");
        env.write_file("bar", "\t\tTrUe   \n");
        env.write_file("baz", "\t\tfalse   \n");
        env.insert_name("t", PrimitiveValue::new_file("bar"));
        env.insert_name("f", PrimitiveValue::new_file("baz"));

        let diagnostic =
            eval_v1_expr(&mut env, V1::Two, "read_boolean('does-not-exist')").unwrap_err();
        assert!(
            diagnostic
                .message()
                .starts_with("call to function `read_boolean` failed: failed to read file")
        );

        let diagnostic = eval_v1_expr(&mut env, V1::Two, "read_boolean('foo')").unwrap_err();
        assert!(
            diagnostic
                .message()
                .contains("does not contain a boolean value on a single line")
        );

        let value = eval_v1_expr(&mut env, V1::Two, "read_boolean('bar')").unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(&mut env, V1::Two, "read_boolean(t)").unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(&mut env, V1::Two, "read_boolean('baz')").unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(&mut env, V1::Two, "read_boolean(f)").unwrap();
        assert!(!value.unwrap_boolean());
    }
}
