//! Implements the `glob` function from the WDL standard library.

use std::path::Path;

use anyhow::Result;
use wdl_analysis::stdlib::STDLIB as ANALYSIS_STDLIB;
use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::Array;
use crate::PrimitiveValue;
use crate::Value;
use crate::diagnostics::function_call_failed;
use crate::stdlib::ensure_local_path;

/// The name of the function defined in this file for use in diagnostics.
const FUNCTION_NAME: &str = "glob";

/// Returns the Bash expansion of the glob string relative to the task's
/// execution directory, and in the same order (i.e. lexicographical).
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#glob
fn glob(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert!(context.arguments.len() == 1);
    debug_assert!(context.return_type_eq(ANALYSIS_STDLIB.array_file_type().clone()));

    let path = context
        .coerce_argument(0, PrimitiveType::String)
        .unwrap_string();

    let path = ensure_local_path(context.base_dir(), &path)
        .map_err(|e| function_call_failed(FUNCTION_NAME, e, context.call_site))?;

    let path = path.to_str().ok_or_else(|| {
        function_call_failed(
            FUNCTION_NAME,
            format!(
                "path `{path}` cannot be represented as UTF-8",
                path = path.display()
            ),
            context.call_site,
        )
    })?;

    // TODO: replace glob with walkpath and globmatch
    let mut elements: Vec<Value> = Vec::new();
    for path in glob::glob(path).map_err(|e| {
        function_call_failed(
            FUNCTION_NAME,
            format!("invalid glob pattern specified: {msg}", msg = e.msg),
            context.arguments[0].span,
        )
    })? {
        let path = path.map_err(|e| function_call_failed(FUNCTION_NAME, &e, context.call_site))?;

        // Filter out directories (only files are returned from WDL's `glob` function)
        if path.is_dir() {
            continue;
        }

        // Strip the CWD prefix if there is one
        let path = match path.strip_prefix(context.base_dir().to_str().unwrap_or("")) {
            Ok(path) => {
                // Create a string from the stripped path
                path.to_str()
                    .ok_or_else(|| {
                        function_call_failed(
                            FUNCTION_NAME,
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
                        FUNCTION_NAME,
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
    Function::new(
        const {
            &[Signature::new(
                "(String) -> Array[File]",
                Callback::Sync(glob),
            )]
        },
    )
}

#[cfg(test)]
mod test {
    use std::fs;

    use pretty_assertions::assert_eq;
    use wdl_ast::version::V1;

    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[tokio::test]
    async fn glob() {
        let env = TestEnv::default();

        let diagnostic = eval_v1_expr(&env, V1::Two, "glob('invalid***')")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `glob` failed: invalid glob pattern specified: wildcards are either \
             regular `*` or recursive `**`"
        );

        let diagnostic = eval_v1_expr(&env, V1::Two, "glob('https://example.com/**')")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `glob` failed: operation not supported for URL `https://example.com/**`"
        );

        env.write_file("qux", "qux");
        env.write_file("baz", "baz");
        env.write_file("foo", "foo");
        env.write_file("bar", "bar");
        fs::create_dir_all(env.base_dir().join("nested").unwrap().unwrap_local())
            .expect("failed to create directory");
        env.write_file("nested/bar", "bar");
        env.write_file("nested/baz", "baz");

        let value = eval_v1_expr(&env, V1::Two, "glob('jam')").await.unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v.as_file().unwrap().as_str())
            .collect();
        assert!(elements.is_empty());

        let value = eval_v1_expr(&env, V1::Two, "glob('*')").await.unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v.as_file().unwrap().as_str())
            .collect();
        assert_eq!(elements, ["bar", "baz", "foo", "qux"]);

        let value = eval_v1_expr(
            &env,
            V1::Two,
            &format!(
                "glob('{url}')",
                url = env.base_dir().join("*").unwrap().unwrap_local().display()
            ),
        )
        .await
        .unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v.as_file().unwrap().as_str())
            .collect();
        assert_eq!(elements, ["bar", "baz", "foo", "qux"]);

        let value = eval_v1_expr(&env, V1::Two, "glob('ba?')").await.unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v.as_file().unwrap().as_str())
            .collect();
        assert_eq!(elements, ["bar", "baz"]);

        let value = eval_v1_expr(&env, V1::Two, "glob('b*')").await.unwrap();
        let elements: Vec<_> = value
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v.as_file().unwrap().as_str())
            .collect();
        assert_eq!(elements, ["bar", "baz"]);

        let value = eval_v1_expr(&env, V1::Two, "glob('**/b*')").await.unwrap();
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
