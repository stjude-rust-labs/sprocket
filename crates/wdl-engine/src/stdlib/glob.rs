//! Implements the `glob` function from the WDL standard library.

use std::path::Path;

use anyhow::Result;
use futures::FutureExt;
use futures::future::BoxFuture;
use globset::GlobBuilder;
use globset::GlobMatcher;
use url::Url;
use walkdir::WalkDir;
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

/// The name of the function defined in this file for use in diagnostics.
const FUNCTION_NAME: &str = "glob";

/// Returns the Bash expansion of the glob string relative to the task's
/// execution directory, and in the same order (i.e. lexicographical).
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#glob
fn glob(context: CallContext<'_>) -> BoxFuture<'_, Result<Value, Diagnostic>> {
    async move {
        debug_assert!(context.arguments.len() == 1);
        debug_assert!(context.return_type_eq(ANALYSIS_STDLIB.array_file_type().clone()));

        // Construct a glob from the given argument
        let glob = GlobBuilder::new(
            &context
                .coerce_argument(0, PrimitiveType::String)
                .unwrap_string(),
        )
        .literal_separator(true)
        .build()
        .map_err(|e| function_call_failed(FUNCTION_NAME, e, context.arguments[0].span))?;

        let matcher = glob.compile_matcher();

        let matches = if let Some(path) = context.base_dir().as_local() {
            glob_local_path(&context, &matcher, path)?
        } else if let Some(url) = context.base_dir().as_remote() {
            glob_remote_path(&context, &matcher, url).await?
        } else {
            unreachable!("evaluation path should be either local or remote");
        };

        Ok(Array::new_unchecked(context.return_type, matches).into())
    }
    .boxed()
}

/// Globs a local path and returns the matching entries.
fn glob_local_path(
    context: &CallContext<'_>,
    matcher: &GlobMatcher,
    path: &Path,
) -> Result<Vec<Value>, Diagnostic> {
    let mut matches: Vec<Value> = Vec::new();
    for entry in WalkDir::new(path).sort_by_file_name() {
        let entry = entry.map_err(|e| {
            function_call_failed(
                FUNCTION_NAME,
                format!(
                    "failed to read directory `{path}`: {e}",
                    path = path.display()
                ),
                context.call_site,
            )
        })?;

        let metadata = entry.metadata().map_err(|e| {
            function_call_failed(
                FUNCTION_NAME,
                format!(
                    "failed to read metadata of path `{path}`: {e}",
                    path = path.display()
                ),
                context.call_site,
            )
        })?;

        // Filter out directories (only files are returned from WDL's `glob` function)
        if !metadata.is_file() {
            continue;
        }

        let relative_path = entry.path().strip_prefix(path).unwrap_or(entry.path());

        // Add it to the list if it matches
        if matcher.is_match(relative_path) {
            matches.push(
                PrimitiveValue::new_file(relative_path.to_str().ok_or_else(|| {
                    function_call_failed(
                        FUNCTION_NAME,
                        format!(
                            "path `{path}` cannot be represented as UTF-8",
                            path = relative_path.display()
                        ),
                        context.call_site,
                    )
                })?)
                .into(),
            );
        }
    }

    Ok(matches)
}

/// Globs a remote URL path.
async fn glob_remote_path(
    context: &CallContext<'_>,
    matcher: &GlobMatcher,
    url: &Url,
) -> Result<Vec<Value>, Diagnostic> {
    let mut matches: Vec<Value> = Vec::new();

    // Use `Transferer::walk` to walk the URL looking for matches
    let paths = context
        .transferer()
        .walk(url)
        .await
        .map_err(|e| function_call_failed(FUNCTION_NAME, e, context.call_site))?;

    for path in paths.iter() {
        // Add it to the list if it matches
        if matcher.is_match(path) {
            matches.push(PrimitiveValue::new_file(path.as_str()).into());
        }
    }

    Ok(matches)
}

/// Gets the function describing `glob`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(pattern: String) -> Array[File]",
                Callback::Async(glob),
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

        let diagnostic = eval_v1_expr(&env, V1::Two, "glob('invalid{')")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `glob` failed: error parsing glob 'invalid{': unclosed alternate \
             group; missing '}' (maybe escape '{' with '[{]'?)"
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
