//! Implements the `exists` function from the WDL standard library.

use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use futures::FutureExt;
use futures::future::BoxFuture;
use tokio::fs;
use url::Url;
use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::Coercible;
use crate::EvaluationPath;
use crate::EvaluationPathKind;
use crate::PrimitiveValue;
use crate::Value;
use crate::diagnostics::function_call_failed;
use crate::is_supported_url;

/// The name of the function defined in this file for use in diagnostics.
const FUNCTION_NAME: &str = "exists";

/// Represents the location of a path whose existence is checked.
enum Location {
    /// The path is local to the execution environment.
    Local(PathBuf),
    /// The path is a remote URL.
    Remote(Url),
}

impl Location {
    /// Converts a URL into a location.
    ///
    /// `file` URLs are converted to local paths.
    fn from_url(url: Url) -> Result<Self> {
        if url.scheme() == "file" {
            return url
                .to_file_path()
                .map(Self::Local)
                .map_err(|_| anyhow!("URL `{url}` cannot be represented as a local file path"));
        }

        Ok(Self::Remote(url))
    }

    /// Resolves the given path relative to the given base directory.
    fn resolve(base_dir: &EvaluationPath, path: &str) -> Result<Self> {
        if !is_supported_url(path) {
            // Local paths are joined without lexical normalization so that any
            // `..` components are resolved by the file system after following
            // symbolic links
            match base_dir.kind() {
                EvaluationPathKind::Local(dir) => return Ok(Self::Local(dir.join(path))),
                EvaluationPathKind::Remote(_) if Path::new(path).is_absolute() => {
                    return Ok(Self::Local(path.into()));
                }
                EvaluationPathKind::Remote(_) => {}
            }
        }

        match base_dir.join(path)?.into_kind() {
            EvaluationPathKind::Local(path) => Ok(Self::Local(path)),
            EvaluationPathKind::Remote(url) => Self::from_url(url),
        }
    }
}

/// Determines whether a path refers to an existing file or directory.
///
/// Returns `false` only if the path does not exist; an error is returned if
/// existence of the path cannot be determined.
///
/// <https://github.com/openwdl/wdl/pull/780>
fn exists(context: CallContext<'_>) -> BoxFuture<'_, Result<Value, Diagnostic>> {
    async move {
        debug_assert_eq!(context.arguments.len(), 1);
        debug_assert!(context.return_type_eq(PrimitiveType::Boolean));

        let path = match &context.arguments[0].value {
            // File and directory values are already host paths; coercing them
            // to `String` would translate them to guest paths
            Value::Primitive(PrimitiveValue::File(path) | PrimitiveValue::Directory(path)) => {
                path.clone()
            }
            _ => {
                // Coerce to `String` first as some arguments (e.g. enum
                // choices) coerce to `String` but not to `File`; the subsequent
                // coercion to `File` performs any guest-to-host path
                // translation
                context
                    .coerce_argument(0, PrimitiveType::String)
                    .coerce(Some(context.inner), &PrimitiveType::File.into())
                    .map_err(|e| {
                        function_call_failed(FUNCTION_NAME, format!("{e:#}"), context.call_site)
                    })?
                    .unwrap_file()
            }
        };

        let result = match Location::resolve(context.base_dir(), path.as_str()) {
            Ok(Location::Local(path)) => local_exists(&path).await,
            Ok(Location::Remote(url)) => {
                let (client, token) = context.http();
                client.exists_uncached(&url, token).await
            }
            Err(e) => Err(e),
        };

        result
            .map(Into::into)
            .map_err(|e| function_call_failed(FUNCTION_NAME, format!("{e:#}"), context.call_site))
    }
    .boxed()
}

/// Determines whether a local path refers to an existing file or directory.
///
/// Symbolic links are followed to their final targets.
async fn local_exists(path: &Path) -> Result<bool> {
    match fs::metadata(path).await {
        Ok(metadata) if metadata.is_file() || metadata.is_dir() => Ok(true),
        Ok(_) => bail!(
            "path `{path}` is not a file or directory",
            path = path.display()
        ),
        Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => Ok(false),
        Err(e) => Err(e).with_context(|| {
            format!(
                "failed to determine existence of path `{path}`",
                path = path.display()
            )
        }),
    }
}

/// Gets the function describing `exists`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(path: String) -> Boolean",
                Callback::Async(exists),
            )]
        },
    )
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;

    use pretty_assertions::assert_eq;
    use url::Url;
    use wdl_analysis::types::EnumType;
    use wdl_analysis::types::PrimitiveType;
    use wdl_ast::Span;
    use wdl_ast::SupportedVersion;
    use wdl_ast::version::V1;

    use crate::CompoundValue;
    use crate::PrimitiveValue;
    use crate::Value;
    use crate::v1::tests::TestEnv;
    use crate::v1::tests::TestEvaluationContext;
    use crate::v1::tests::eval_v1_expr;
    use crate::v1::tests::eval_v1_expr_with_context;
    use crate::value::EnumChoice;

    /// Evaluates the given expression and returns its boolean result.
    async fn eval(env: &TestEnv, source: &str) -> bool {
        eval_v1_expr(env, V1::Four, source)
            .await
            .unwrap_or_else(|e| panic!("failed to evaluate `{source}`: {e:?}"))
            .unwrap_boolean()
    }

    /// Gets the local path of the given name within the test's base directory.
    fn local(env: &TestEnv, name: &str) -> std::path::PathBuf {
        env.base_dir().join(name).unwrap().unwrap_local()
    }

    #[tokio::test]
    async fn local_paths() {
        let mut env = TestEnv::default();
        env.write_file("foo.txt", "foo");
        fs::create_dir(local(&env, "dir")).unwrap();

        assert!(eval(&env, "exists('foo.txt')").await);
        assert!(eval(&env, "exists('dir')").await);
        assert!(!eval(&env, "exists('missing.txt')").await);
        assert!(!eval(&env, "exists('missing/foo.txt')").await);
        assert!(!eval(&env, "exists('foo.txt/bar')").await);
        assert!(eval(&env, "if exists('foo.txt') then true else false").await);

        let absolute = local(&env, "foo.txt");
        assert!(eval(&env, &format!("exists('{}')", absolute.display())).await);
        let url = Url::from_file_path(&absolute).unwrap();
        assert!(eval(&env, &format!("exists('{url}')")).await);
        let url = Url::from_file_path(local(&env, "missing.txt")).unwrap();
        assert!(!eval(&env, &format!("exists('{url}')")).await);

        env.insert_name("file", PrimitiveValue::new_file(absolute.to_str().unwrap()));
        env.insert_name(
            "directory",
            PrimitiveValue::new_directory(local(&env, "dir").to_str().unwrap()),
        );
        assert!(eval(&env, "exists(file)").await);
        assert!(eval(&env, "exists(directory)").await);
    }

    #[tokio::test]
    async fn enum_choice() {
        let mut env = TestEnv::default();
        env.write_file("Red", "red");

        let enum_ty = EnumType::new(
            "Color",
            Span::new(0, 0),
            PrimitiveType::String.into(),
            vec![
                ("Red".into(), PrimitiveType::String.into()),
                ("Blue".into(), PrimitiveType::String.into()),
            ],
            &[Span::new(0, 0), Span::new(0, 0)],
        )
        .unwrap();
        for name in ["Red", "Blue"] {
            env.insert_name(
                name.to_lowercase(),
                Value::Compound(CompoundValue::EnumChoice(EnumChoice::new(
                    enum_ty.clone(),
                    name,
                    PrimitiveValue::String(Arc::new(name.to_string())),
                ))),
            );
        }

        assert!(eval(&env, "exists(red)").await);
        assert!(!eval(&env, "exists(blue)").await);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symbolic_links() {
        use std::os::unix::fs::symlink;

        let env = TestEnv::default();
        env.write_file("target.txt", "target");
        fs::create_dir_all(local(&env, "a/b")).unwrap();
        symlink(local(&env, "target.txt"), local(&env, "link.txt")).unwrap();
        symlink(local(&env, "missing.txt"), local(&env, "broken.txt")).unwrap();
        symlink(local(&env, "a/b"), local(&env, "dir-link")).unwrap();
        symlink(local(&env, "loop"), local(&env, "loop")).unwrap();

        assert!(eval(&env, "exists('link.txt')").await);
        assert!(!eval(&env, "exists('broken.txt')").await);
        assert!(eval(&env, "exists('dir-link')").await);

        // `dir-link/..` refers to `a`, not the base directory, once the link
        // is followed
        env.write_file("a/only-in-a.txt", "a");
        env.write_file("only-in-base.txt", "base");
        assert!(eval(&env, "exists('dir-link/../only-in-a.txt')").await);
        assert!(!eval(&env, "exists('dir-link/../only-in-base.txt')").await);

        let diagnostic = eval_v1_expr(&env, V1::Four, "exists('loop')")
            .await
            .unwrap_err();
        assert!(
            diagnostic
                .message()
                .starts_with("call to function `exists` failed: failed to determine existence"),
            "{}",
            diagnostic.message()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn permission_denied() {
        use std::os::unix::fs::PermissionsExt;

        let env = TestEnv::default();
        let locked = local(&env, "locked");
        fs::create_dir(&locked).unwrap();
        env.write_file("locked/file.txt", "locked");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

        // Privileged users may bypass the permission check
        let accessible = fs::metadata(locked.join("file.txt")).is_ok();
        let result = eval_v1_expr(&env, V1::Four, "exists('locked/file.txt')").await;
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

        if accessible {
            assert!(result.unwrap().unwrap_boolean());
        } else {
            let diagnostic = result.unwrap_err();
            assert!(
                diagnostic.message().starts_with(
                    "call to function `exists` failed: failed to determine existence of path"
                ),
                "{}",
                diagnostic.message()
            );
        }
    }

    #[tokio::test]
    async fn remote_paths() {
        let env = TestEnv::default();
        env.write_file("foo.txt", "foo");

        assert!(eval(&env, "exists('https://example.com/foo.txt')").await);
        assert!(!eval(&env, "exists('https://example.com/missing.txt')").await);

        let diagnostic = eval_v1_expr(&env, V1::Four, "exists('https://example.com/error')")
            .await
            .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "call to function `exists` failed: failed to determine existence of \
             `https://example.com/error`"
        );
    }

    #[tokio::test]
    async fn remote_paths_are_not_cached() {
        let env = TestEnv::default();
        let context = TestEvaluationContext::new(&env, SupportedVersion::V1(V1::Four)).await;
        let url: Url = "https://example.com/later.txt".parse().unwrap();

        // Populate the evaluation's cache with a negative result
        let client = context.http_client().clone();
        assert!(!client.exists(&url, &Default::default()).await.unwrap());

        env.write_file("later.txt", "later");
        let value = eval_v1_expr_with_context(context, &format!("exists('{url}')"))
            .await
            .unwrap();
        assert!(value.unwrap_boolean());
    }
}
