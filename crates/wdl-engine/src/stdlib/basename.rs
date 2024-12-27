//! Implements the `baseline` function from the WDL standard library.

use std::path::Path;

use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::PrimitiveValue;
use crate::Value;

/// Returns the "basename" of a file or directory - the name after the last
/// directory separator in the path.
///
/// The optional second parameter specifies a literal suffix to remove from the
/// file name. If the file name does not end with the specified suffix then it
/// is ignored.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#basename
fn basename(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert!(!context.arguments.is_empty() && context.arguments.len() < 3);
    debug_assert!(context.return_type_eq(PrimitiveType::String));

    let path = context
        .coerce_argument(0, PrimitiveType::String)
        .unwrap_string();

    match Path::new(path.as_str()).file_name() {
        Some(base) => {
            let base = base.to_str().expect("should be UTF-8");
            let base = if context.arguments.len() == 2 {
                base.strip_suffix(
                    context
                        .coerce_argument(1, PrimitiveType::String)
                        .unwrap_string()
                        .as_str(),
                )
                .unwrap_or(base)
            } else {
                base
            };

            Ok(PrimitiveValue::new_string(base).into())
        }
        None => Ok(PrimitiveValue::String(path).into()),
    }
}

/// Gets the function describing `basename`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[
                Signature::new("(File, <String>) -> String", basename),
                Signature::new("(String, <String>) -> String", basename),
                Signature::new("(Directory, <String>) -> String", basename),
            ]
        },
    )
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use wdl_ast::version::V1;

    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[test]
    fn basename() {
        let mut env = TestEnv::default();
        let value = eval_v1_expr(&mut env, V1::Two, "basename('/path/to/file.txt')").unwrap();
        assert_eq!(value.unwrap_string().as_str(), "file.txt");

        let value =
            eval_v1_expr(&mut env, V1::Two, "basename('/path/to/file.txt', '.txt')").unwrap();
        assert_eq!(value.unwrap_string().as_str(), "file");

        let value = eval_v1_expr(&mut env, V1::Two, "basename('/path/to/dir')").unwrap();
        assert_eq!(value.unwrap_string().as_str(), "dir");

        let value = eval_v1_expr(&mut env, V1::Two, "basename('file.txt')").unwrap();
        assert_eq!(value.unwrap_string().as_str(), "file.txt");

        let value = eval_v1_expr(&mut env, V1::Two, "basename('file.txt', '.txt')").unwrap();
        assert_eq!(value.unwrap_string().as_str(), "file");
    }
}
