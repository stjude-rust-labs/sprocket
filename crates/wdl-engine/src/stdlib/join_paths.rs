//! Implements the `join_paths` function from the WDL standard library.

use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use wdl_analysis::stdlib::STDLIB as ANALYSIS_STDLIB;
use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::PrimitiveValue;
use crate::Value;
use crate::diagnostics::function_call_failed;
use crate::path;

/// The name of the function defined in this file for use in diagnostics.
const FUNCTION_NAME: &str = "join_paths";

/// Joins together two paths into an absolute path in the host
/// filesystem.
///
/// `File join_paths(File, String)`: Joins together exactly two paths. The first
/// path may be either absolute or relative and must specify a directory; the
/// second path is relative to the first path and may specify a file or
/// directory.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#-join_paths
fn join_paths_simple(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert!(context.arguments.len() == 2);
    debug_assert!(context.return_type_eq(PrimitiveType::File));

    let first = context
        .coerce_argument(0, PrimitiveType::File)
        .unwrap_file();

    let second = context
        .coerce_argument(1, PrimitiveType::String)
        .unwrap_string();

    if let Some(mut url) = path::parse_url(&first) {
        if second.starts_with('/') | second.contains(":") {
            return Err(function_call_failed(
                FUNCTION_NAME,
                format!("path `{second}` is not a relative path"),
                context.arguments[1].span,
            ));
        }

        // For consistency with `PathBuf::push`, push an empty segment so that we treat
        // the last segment as a directory; otherwise, `Url::join` will treat it as a
        // file.
        if let Ok(mut segments) = url.path_segments_mut() {
            segments.pop_if_empty();
            segments.push("");
        }

        return url
            .join(&second)
            .map(|u| PrimitiveValue::new_file(u).into())
            .map_err(|_| {
                function_call_failed(
                    FUNCTION_NAME,
                    format!("path `{second}` cannot be joined with URL `{url}`"),
                    context.arguments[1].span,
                )
            });
    }

    let second = Path::new(second.as_str());
    if !second.is_relative() {
        return Err(function_call_failed(
            FUNCTION_NAME,
            format!(
                "path `{second}` is not a relative path",
                second = second.display()
            ),
            context.arguments[1].span,
        ));
    }

    let mut path = PathBuf::from(Arc::unwrap_or_clone(first));
    path.push(second);

    Ok(PrimitiveValue::new_file(
        path.into_os_string()
            .into_string()
            .expect("should be UTF-8"),
    )
    .into())
}

/// Joins together two or more paths into an absolute path in the host
/// filesystem.
///
/// `File join_paths(File, Array[String]+)`: Joins together any number of
/// relative paths with a base path. The first argument may be either an
/// absolute or a relative path and must specify a directory. The paths in the
/// second array argument must all be relative. The last element may specify a
/// file or directory; all other elements must specify a directory.
///
/// `File join_paths(Array[String]+)`: Joins together any number of paths. The
/// array must not be empty. The first element of the array may be either
/// absolute or relative; subsequent path(s) must be relative. The last element
/// may specify a file or directory; all other elements must specify a
/// directory.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#-join_paths
fn join_paths(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert!(!context.arguments.is_empty() && context.arguments.len() < 3);
    debug_assert!(context.return_type_eq(PrimitiveType::File));

    // Handle being provided one or two arguments
    let (first, array, skip, array_span) = if context.arguments.len() == 1 {
        let array = context
            .coerce_argument(0, ANALYSIS_STDLIB.array_string_non_empty_type().clone())
            .unwrap_array();

        (
            array.as_slice()[0].clone().unwrap_string(),
            array,
            true,
            context.arguments[0].span,
        )
    } else {
        let first = context
            .coerce_argument(0, PrimitiveType::File)
            .unwrap_file();

        let array = context
            .coerce_argument(1, ANALYSIS_STDLIB.array_string_non_empty_type().clone())
            .unwrap_array();

        (first, array, false, context.arguments[1].span)
    };

    if let Some(mut url) = path::parse_url(&first) {
        for (i, element) in array
            .as_slice()
            .iter()
            .enumerate()
            .skip(if skip { 1 } else { 0 })
        {
            let next = element.as_string().expect("element should be string");
            if next.starts_with('/') || next.contains(":") {
                return Err(function_call_failed(
                    FUNCTION_NAME,
                    format!("path `{next}` (array index {i}) is not a relative path"),
                    array_span,
                ));
            }

            // For consistency with `PathBuf::push`, push an empty segment so that we treat
            // the last segment as a directory; otherwise, `Url::join` will treat it as a
            // file.
            if let Ok(mut segments) = url.path_segments_mut() {
                segments.pop_if_empty();
                segments.push("");
            }

            url = url.join(next).map_err(|_| {
                function_call_failed(
                    FUNCTION_NAME,
                    format!("path `{next}` (array index {i}) cannot be joined with URL `{url}`"),
                    context.arguments[1].span,
                )
            })?;
        }

        return Ok(PrimitiveValue::new_file(url).into());
    }

    let mut path = PathBuf::from(Arc::unwrap_or_clone(first));

    for (i, element) in array
        .as_slice()
        .iter()
        .enumerate()
        .skip(if skip { 1 } else { 0 })
    {
        let next = element.as_string().expect("element should be string");
        let p = Path::new(next.as_str());
        if !p.is_relative() {
            return Err(function_call_failed(
                FUNCTION_NAME,
                format!("path `{next}` (array index {i}) is not a relative path"),
                array_span,
            ));
        }

        path.push(p);
    }

    Ok(PrimitiveValue::new_file(
        path.into_os_string()
            .into_string()
            .expect("should be UTF-8"),
    )
    .into())
}

/// Gets the function describing `join_paths`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[
                Signature::new("(File, String) -> File", Callback::Sync(join_paths_simple)),
                Signature::new("(File, Array[String]+) -> File", Callback::Sync(join_paths)),
                Signature::new("(Array[String]+) -> File", Callback::Sync(join_paths)),
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

    #[tokio::test]
    async fn join_paths() {
        let env = TestEnv::default();
        let value = eval_v1_expr(&env, V1::Two, "join_paths('/usr', ['bin', 'echo'])")
            .await
            .unwrap();
        assert_eq!(
            value.unwrap_file().as_str().replace('\\', "/"),
            "/usr/bin/echo"
        );

        let value = eval_v1_expr(&env, V1::Two, "join_paths(['/usr', 'bin', 'echo'])")
            .await
            .unwrap();
        assert_eq!(
            value.unwrap_file().as_str().replace('\\', "/"),
            "/usr/bin/echo"
        );

        let value = eval_v1_expr(&env, V1::Two, "join_paths('mydir', 'mydata.txt')")
            .await
            .unwrap();
        assert_eq!(
            value.unwrap_file().as_str().replace('\\', "/"),
            "mydir/mydata.txt"
        );

        let value = eval_v1_expr(&env, V1::Two, "join_paths('/usr', 'bin/echo')")
            .await
            .unwrap();
        assert_eq!(
            value.unwrap_file().as_str().replace('\\', "/"),
            "/usr/bin/echo"
        );

        #[cfg(unix)]
        {
            let diagnostic = eval_v1_expr(&env, V1::Two, "join_paths('/usr', '/bin/echo')")
                .await
                .unwrap_err();
            assert_eq!(
                diagnostic.message(),
                "call to function `join_paths` failed: path `/bin/echo` is not a relative path"
            );

            let diagnostic =
                eval_v1_expr(&env, V1::Two, "join_paths('/usr', ['foo', '/bin/echo'])")
                    .await
                    .unwrap_err();
            assert_eq!(
                diagnostic.message(),
                "call to function `join_paths` failed: path `/bin/echo` (array index 1) is not a \
                 relative path"
            );

            let diagnostic =
                eval_v1_expr(&env, V1::Two, "join_paths(['/usr', 'foo', '/bin/echo'])")
                    .await
                    .unwrap_err();
            assert_eq!(
                diagnostic.message(),
                "call to function `join_paths` failed: path `/bin/echo` (array index 2) is not a \
                 relative path"
            );
        }

        #[cfg(windows)]
        {
            let diagnostic = eval_v1_expr(&env, V1::Two, "join_paths('C:\\usr', 'C:\\bin\\echo')")
                .await
                .unwrap_err();
            assert_eq!(
                diagnostic.message(),
                "call to function `join_paths` failed: path `C:\\bin\\echo` is not a relative path"
            );

            let diagnostic = eval_v1_expr(
                &env,
                V1::Two,
                "join_paths('C:\\usr', ['foo', 'C:\\bin\\echo'])",
            )
            .await
            .unwrap_err();
            assert_eq!(
                diagnostic.message(),
                "call to function `join_paths` failed: path `C:\\bin\\echo` (array index 1) is \
                 not a relative path"
            );

            let diagnostic = eval_v1_expr(
                &env,
                V1::Two,
                "join_paths(['C:\\usr', 'foo', 'C:\\bin\\echo'])",
            )
            .await
            .unwrap_err();
            assert_eq!(
                diagnostic.message(),
                "call to function `join_paths` failed: path `C:\\bin\\echo` (array index 2) is \
                 not a relative path"
            );
        }

        let diagnostic = eval_v1_expr(
            &env,
            V1::Two,
            "join_paths('https://example.com', '/foo/bar')",
        )
        .await
        .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `join_paths` failed: path `/foo/bar` is not a relative path"
        );

        let diagnostic = eval_v1_expr(
            &env,
            V1::Two,
            "join_paths('https://example.com', '//wrong.org/foo')",
        )
        .await
        .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `join_paths` failed: path `//wrong.org/foo` is not a relative path"
        );

        let diagnostic = eval_v1_expr(
            &env,
            V1::Two,
            "join_paths('https://example.com', 'https://wrong.org/foo')",
        )
        .await
        .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `join_paths` failed: path `https://wrong.org/foo` is not a relative \
             path"
        );

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "join_paths('https://example.com', 'foo/bar/baz')",
        )
        .await
        .unwrap();
        assert_eq!(
            value.unwrap_file().as_str(),
            "https://example.com/foo/bar/baz"
        );

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "join_paths('https://example.com/foo/bar/', 'baz')",
        )
        .await
        .unwrap();
        assert_eq!(
            value.unwrap_file().as_str(),
            "https://example.com/foo/bar/baz"
        );

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "join_paths('https://example.com/foo/bar', '../baz')",
        )
        .await
        .unwrap();
        assert_eq!(value.unwrap_file().as_str(), "https://example.com/foo/baz");

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "join_paths('https://example.com/foo/bar', ['nope', '../baz', 'qux'])",
        )
        .await
        .unwrap();
        assert_eq!(
            value.unwrap_file().as_str(),
            "https://example.com/foo/bar/baz/qux"
        );

        let value = eval_v1_expr(
            &env,
            V1::Two,
            "join_paths('https://example.com/foo/bar?foo=jam', 'baz?foo=qux')",
        )
        .await
        .unwrap();
        assert_eq!(
            value.unwrap_file().as_str(),
            "https://example.com/foo/bar/baz?foo=qux"
        );
    }
}
