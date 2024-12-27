//! Implements the `read_string` function from the WDL standard library.

use std::fs;

use anyhow::Context;
use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::PrimitiveValue;
use crate::Value;
use crate::diagnostics::function_call_failed;

/// Reads an entire file as a String, with any trailing end-of-line characters
/// (\r and \n) stripped off.
///
/// If the file is empty, an empty string is returned.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#read_string
fn read_string(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert!(context.arguments.len() == 1);
    debug_assert!(context.return_type_eq(PrimitiveType::String));

    let path = context.work_dir().join(
        context
            .coerce_argument(0, PrimitiveType::File)
            .unwrap_file()
            .as_str(),
    );
    let mut contents = fs::read_to_string(&path)
        .with_context(|| format!("failed to read file `{path}`", path = path.display()))
        .map_err(|e| function_call_failed("read_string", format!("{e:?}"), context.call_site))?;

    let trimmed = contents.trim_end_matches(['\r', '\n']);
    contents.truncate(trimmed.len());
    Ok(PrimitiveValue::new_string(contents).into())
}

/// Gets the function describing `read_string`.
pub const fn descriptor() -> Function {
    Function::new(const { &[Signature::new("(File) -> String", read_string)] })
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use wdl_ast::version::V1;

    use crate::PrimitiveValue;
    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[test]
    fn read_string() {
        let mut env = TestEnv::default();
        env.write_file("foo", "hello\nworld!\n\r\n");
        env.insert_name(
            "file",
            PrimitiveValue::new_file(
                env.work_dir()
                    .join("foo")
                    .to_str()
                    .expect("should be UTF-8"),
            ),
        );

        let diagnostic =
            eval_v1_expr(&mut env, V1::Two, "read_string('does-not-exist')").unwrap_err();
        assert!(
            diagnostic
                .message()
                .starts_with("call to function `read_string` failed: failed to read file")
        );

        let value = eval_v1_expr(&mut env, V1::Two, "read_string('foo')").unwrap();
        assert_eq!(value.unwrap_string().as_str(), "hello\nworld!");

        let value = eval_v1_expr(&mut env, V1::Two, "read_string(file)").unwrap();
        assert_eq!(value.unwrap_string().as_str(), "hello\nworld!");
    }
}
