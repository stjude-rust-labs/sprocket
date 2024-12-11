//! Implements the `glob` function from the WDL standard library.

use std::path::Path;

use wdl_analysis::stdlib::STDLIB as ANALYSIS_STDLIB;
use wdl_analysis::types::PrimitiveTypeKind;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::Array;
use crate::PrimitiveValue;
use crate::Value;
use crate::diagnostics::function_call_failed;
use crate::diagnostics::invalid_glob_pattern;

/// Returns the Bash expansion of the glob string relative to the task's
/// execution directory, and in the same order (i.e. lexicographical).
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#glob
fn glob(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert!(context.arguments.len() == 1);
    debug_assert!(context.return_type_eq(ANALYSIS_STDLIB.array_file_type()));

    let path = context
        .coerce_argument(0, PrimitiveTypeKind::String)
        .unwrap_string();

    // TODO: replace glob with walkpath and globmatch
    let mut elements: Vec<Value> = Vec::new();
    for path in glob::glob(&context.work_dir().join(path.as_str()).to_string_lossy())
        .map_err(|e| invalid_glob_pattern(&e, context.arguments[0].span))?
    {
        let path = path.map_err(|e| function_call_failed("glob", &e, context.call_site))?;

        // Filter out directories (only files are returned from WDL's `glob` function)
        if path.is_dir() {
            continue;
        }

        // Strip the CWD prefix if there is one
        let path = match path.strip_prefix(context.work_dir()) {
            Ok(path) => {
                // Create a string from the stripped path
                path.to_str()
                    .ok_or_else(|| {
                        function_call_failed(
                            "glob",
                            format!(
                                "path `{path}` cannot be represented as UTF-8",
                                path = path.display()
                            ),
                            context.call_site,
                        )
                    })?
                    .to_string()
            }
            Err(_) => {
                // Convert the path directly to a string
                path.into_os_string().into_string().map_err(|path| {
                    function_call_failed(
                        "glob",
                        format!(
                            "path `{path}` cannot be represented as UTF-8",
                            path = Path::new(&path).display()
                        ),
                        context.call_site,
                    )
                })?
            }
        };

        elements.push(PrimitiveValue::new_file(path).into());
    }

    Ok(Array::new_unchecked(context.return_type, elements).into())
}

/// Gets the function describing `glob`.
pub const fn descriptor() -> Function {
    Function::new(const { &[Signature::new("(String) -> Array[File]", glob)] })
}

#[cfg(test)]
mod test {
    use std::fs;

    use pretty_assertions::assert_eq;
    use wdl_ast::version::V1;

    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[test]
    fn glob() {
        let mut env = TestEnv::default();
        let diagnostic = eval_v1_expr(&mut env, V1::Two, "glob('invalid***')").unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "invalid glob pattern specified: wildcards are either regular `*` or recursive `**`"
        );

        env.write_file("qux", "qux");
        env.write_file("baz", "baz");
        env.write_file("foo", "foo");
        env.write_file("bar", "bar");
        fs::create_dir_all(env.work_dir().join("nested")).expect("failed to create directory");
        env.write_file("nested/bar", "bar");
        env.write_file("nested/baz", "baz");

        let value = eval_v1_expr(&mut env, V1::Two, "glob('jam')").unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v.as_file().unwrap().as_str())
            .collect();
        assert!(elements.is_empty());

        let value = eval_v1_expr(&mut env, V1::Two, "glob('*')").unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v.as_file().unwrap().as_str())
            .collect();
        assert_eq!(elements, ["bar", "baz", "foo", "qux"]);

        let value = eval_v1_expr(&mut env, V1::Two, "glob('ba?')").unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v.as_file().unwrap().as_str())
            .collect();
        assert_eq!(elements, ["bar", "baz"]);

        let value = eval_v1_expr(&mut env, V1::Two, "glob('b*')").unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v.as_file().unwrap().as_str())
            .collect();
        assert_eq!(elements, ["bar", "baz"]);

        let value = eval_v1_expr(&mut env, V1::Two, "glob('**/b*')").unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v.as_file().unwrap().as_str().replace('\\', "/"))
            .collect();
        assert_eq!(elements, ["bar", "baz", "nested/bar", "nested/baz"]);
    }
}
