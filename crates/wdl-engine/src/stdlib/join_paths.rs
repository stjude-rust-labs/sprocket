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
use crate::diagnostics::array_path_not_relative;
use crate::diagnostics::path_not_relative;

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

    let second = Path::new(second.as_str());
    if !second.is_relative() {
        return Err(path_not_relative(context.arguments[1].span));
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

    let mut path = PathBuf::from(Arc::unwrap_or_clone(first));

    for (i, element) in array
        .as_slice()
        .iter()
        .enumerate()
        .skip(if skip { 1 } else { 0 })
    {
        let next = element.as_string().expect("element should be string");

        let next = Path::new(next.as_str());
        if !next.is_relative() {
            return Err(array_path_not_relative(i, array_span));
        }

        path.push(next);
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
                "path is required to be a relative path, but an absolute path was provided"
            );

            let diagnostic =
                eval_v1_expr(&env, V1::Two, "join_paths('/usr', ['foo', '/bin/echo'])")
                    .await
                    .unwrap_err();
            assert_eq!(
                diagnostic.message(),
                "index 1 of the array is required to be a relative path, but an absolute path was \
                 provided"
            );

            let diagnostic =
                eval_v1_expr(&env, V1::Two, "join_paths(['/usr', 'foo', '/bin/echo'])")
                    .await
                    .unwrap_err();
            assert_eq!(
                diagnostic.message(),
                "index 2 of the array is required to be a relative path, but an absolute path was \
                 provided"
            );
        }

        #[cfg(windows)]
        {
            let diagnostic = eval_v1_expr(&env, V1::Two, "join_paths('C:\\usr', 'C:\\bin\\echo')")
                .await
                .unwrap_err();
            assert_eq!(
                diagnostic.message(),
                "path is required to be a relative path, but an absolute path was provided"
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
                "index 1 of the array is required to be a relative path, but an absolute path was \
                 provided"
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
                "index 2 of the array is required to be a relative path, but an absolute path was \
                 provided"
            );
        }
    }
}
