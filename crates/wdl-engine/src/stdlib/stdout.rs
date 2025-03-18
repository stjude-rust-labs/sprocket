//! Implements the `stdout` function from the WDL standard library.

use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::Value;
use crate::diagnostics::function_call_failed;

/// Returns the value of the executed command's standard output (stdout) as a
/// File.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#stdout
fn stdout(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert!(context.arguments.is_empty());
    debug_assert!(context.return_type_eq(PrimitiveType::File));

    match context.stdout() {
        Some(stdout) => {
            debug_assert!(
                stdout.as_file().is_some(),
                "expected the value to be a file"
            );
            Ok(stdout.clone())
        }
        None => Err(function_call_failed(
            "stdout",
            "function may only be called in a task output section",
            context.call_site,
        )),
    }
}

/// Gets the function describing `stdout`.
pub const fn descriptor() -> Function {
    Function::new(const { &[Signature::new("() -> File", stdout)] })
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use wdl_ast::version::V1;

    use crate::PrimitiveValue;
    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;
    use crate::v1::test::eval_v1_expr_with_stdio;

    #[tokio::test]
    async fn stdout() {
        let env = TestEnv::default();
        let diagnostic = eval_v1_expr(&env, V1::Two, "stdout()").await.unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `stdout` failed: function may only be called in a task output \
             section"
        );

        let value = eval_v1_expr_with_stdio(
            &env,
            V1::Zero,
            "stdout()",
            PrimitiveValue::new_file("stdout.txt"),
            PrimitiveValue::new_file("stderr.txt"),
        )
        .await
        .unwrap();
        assert_eq!(value.unwrap_file().as_str(), "stdout.txt");
    }
}
