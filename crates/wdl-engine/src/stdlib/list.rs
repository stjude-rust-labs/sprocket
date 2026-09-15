//! Implements the `list` function from the WDL standard library.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use cloud_copy::UrlExt;
use futures::FutureExt;
use futures::future::BoxFuture;
use globset::GlobBuilder;
use globset::GlobMatcher;
use tokio::fs;
use url::Url;
use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::Array;
use crate::EvaluationPath;
use crate::EvaluationPathKind;
use crate::Pair;
use crate::PrimitiveValue;
use crate::Value;
use crate::diagnostics::function_call_failed;

/// The name of the function defined in this file for use in diagnostics.
const FUNCTION_NAME: &str = "list";

/// A directory's contents, expressed as relative paths with `/` separators.
#[derive(Default)]
struct Listing {
    /// The files in the directory.
    files: Vec<String>,
    /// The directories, deduplicated when inferred from remote file paths.
    directories: BTreeSet<String>,
}

/// The kind of a listed directory entry.
enum EntryKind {
    /// A file, including a broken symbolic link.
    File,
    /// A directory.
    Directory,
}

/// Lists files and directories, optionally recursively and with a file filter.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.4/SPEC.md#-list
fn list(context: CallContext<'_>) -> BoxFuture<'_, Result<Value, Diagnostic>> {
    async move {
        debug_assert!((1..=4).contains(&context.arguments.len()));

        let directory = context
            .coerce_argument(0, PrimitiveType::Directory)
            .unwrap_directory();
        let root = context.base_dir().join(directory.as_str()).map_err(|e| {
            function_call_failed(FUNCTION_NAME, format!("{e:#}"), context.call_site)
        })?;
        let recursive = context.arguments.len() >= 2
            && context
                .coerce_argument(1, PrimitiveType::Boolean)
                .unwrap_boolean();
        let include_symlinks = context.arguments.len() < 3
            || context
                .coerce_argument(2, PrimitiveType::Boolean)
                .unwrap_boolean();
        let matcher = if context.arguments.len() == 4 {
            Some(
                GlobBuilder::new(
                    &context
                        .coerce_argument(3, PrimitiveType::String)
                        .unwrap_string(),
                )
                .literal_separator(true)
                .build()
                .map_err(|e| function_call_failed(FUNCTION_NAME, e, context.arguments[3].span))?
                .compile_matcher(),
            )
        } else {
            None
        };

        let mut listing = match root.kind() {
            EvaluationPathKind::Local(path) => {
                list_local(path, recursive, include_symlinks, matcher.as_ref()).await
            }
            EvaluationPathKind::Remote(url) => {
                list_remote(&context, url, recursive, matcher.as_ref()).await
            }
        }
        .map_err(|e| function_call_failed(FUNCTION_NAME, format!("{e:#}"), context.call_site))?;

        // Sort strings rather than path components so separators participate
        // in the ordering, independently of the host platform.
        listing.files.sort_unstable();
        let to_path = |relative: &str| {
            listed_path(&root, relative).map_err(|e| {
                function_call_failed(FUNCTION_NAME, format!("{e:#}"), context.call_site)
            })
        };
        let files = listing
            .files
            .iter()
            .map(|p| to_path(p).map(|p| PrimitiveValue::new_file(p).into()))
            .collect::<Result<Vec<Value>, _>>()?;
        let directories = listing
            .directories
            .iter()
            .map(|p| to_path(p).map(|p| PrimitiveValue::new_directory(p).into()))
            .collect::<Result<Vec<Value>, _>>()?;
        let pair_ty = context
            .return_type
            .as_pair()
            .expect("return type should be a pair");

        Ok(Pair::new_unchecked(
            context.return_type.clone(),
            Array::new_unchecked(pair_ty.left_type().clone(), files).into(),
            Array::new_unchecked(pair_ty.right_type().clone(), directories).into(),
        )
        .into())
    }
    .boxed()
}

/// Tests a file's basename without applying the pattern to its parent paths.
fn matches_file(matcher: Option<&GlobMatcher>, name: &str) -> bool {
    matcher.is_none_or(|matcher| {
        (!name.starts_with('.') || matcher.glob().glob().starts_with('.')) && matcher.is_match(name)
    })
}

/// Lists a local directory without traversing directory symlinks.
async fn list_local(
    root: &Path,
    recursive: bool,
    include_symlinks: bool,
    matcher: Option<&GlobMatcher>,
) -> Result<Listing> {
    let metadata = fs::metadata(root)
        .await
        .with_context(|| format!("failed to read metadata for directory `{}`", root.display()))?;
    if !metadata.is_dir() {
        bail!("path `{}` is not a directory", root.display());
    }

    let mut listing = Listing::default();
    let mut pending = vec![String::new()];
    while let Some(parent) = pending.pop() {
        let path = root.join(&parent);
        let mut entries = fs::read_dir(&path)
            .await
            .with_context(|| format!("failed to read directory `{}`", path.display()))?;
        while let Some(entry) = entries
            .next_entry()
            .await
            .with_context(|| format!("failed to read entry of directory `{}`", path.display()))?
        {
            let file_type = entry.file_type().await.with_context(|| {
                format!("failed to read file type of `{}`", entry.path().display())
            })?;
            let symlink = file_type.is_symlink();
            let kind = if symlink {
                if !include_symlinks {
                    continue;
                }

                match fs::metadata(entry.path()).await {
                    Ok(metadata) if metadata.is_file() => EntryKind::File,
                    Ok(metadata) if metadata.is_dir() => EntryKind::Directory,
                    Ok(_) => continue,
                    // An unresolved symbolic link is broken for listing purposes.
                    Err(_) => EntryKind::File,
                }
            } else if file_type.is_file() {
                EntryKind::File
            } else if file_type.is_dir() {
                EntryKind::Directory
            } else {
                continue;
            };

            let name = entry.file_name().into_string().map_err(|_| {
                anyhow!(
                    "path `{}` cannot be represented as UTF-8",
                    entry.path().display()
                )
            })?;
            let relative = if parent.is_empty() {
                name.clone()
            } else {
                format!("{parent}/{name}")
            };
            match kind {
                EntryKind::Directory => {
                    if recursive && !symlink {
                        pending.push(relative.clone());
                    }
                    listing.directories.insert(relative);
                }
                EntryKind::File if matches_file(matcher, &name) => {
                    listing.files.push(relative);
                }
                EntryKind::File => {}
            }
        }
    }
    Ok(listing)
}

/// Lists cloud directory metadata without downloading its file contents.
async fn list_remote(
    context: &CallContext<'_>,
    url: &Url,
    recursive: bool,
    matcher: Option<&GlobMatcher>,
) -> Result<Listing> {
    let (client, token) = context.http();
    let paths = client.walk(url, token).await?;

    let mut listing = Listing::default();
    for path in paths.iter() {
        for (index, _) in path.match_indices('/') {
            listing.directories.insert(path[..index].to_owned());
            if !recursive {
                break;
            }
        }

        let name = path
            .rsplit('/')
            .next()
            .expect("path should have a basename");
        if !name.is_empty() && (recursive || name == path) && matches_file(matcher, name) {
            listing.files.push(path.clone());
        }
    }
    Ok(listing)
}

/// Resolves a listed path while preserving URL queries and literal file names.
fn listed_path(root: &EvaluationPath, relative: &str) -> Result<String> {
    match root.kind() {
        EvaluationPathKind::Local(root) => {
            let path = root.join(relative);
            path.to_str()
                .with_context(|| {
                    format!("path `{}` cannot be represented as UTF-8", path.display())
                })
                .map(str::to_owned)
        }
        EvaluationPathKind::Remote(root) => {
            let mut url = root.clone();
            url.path_segments_mut()
                .map_err(|_| anyhow!("URL `{}` cannot contain directory entries", root.display()))?
                .pop_if_empty()
                .extend(relative.split('/'));
            Ok(url.into())
        }
    }
}

/// Gets the function describing `list`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(directory: Directory, <recursive: Boolean>, <include_symlinks: Boolean>, \
                 <pattern: String>) -> Pair[Array[File], Array[Directory]]",
                Callback::Async(list),
            )]
        },
    )
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use pretty_assertions::assert_eq;
    use url::Url;
    use wdl_analysis::stdlib::FunctionBindError;
    use wdl_analysis::stdlib::STDLIB;
    use wdl_analysis::types::ArrayType;
    use wdl_analysis::types::Optional;
    use wdl_analysis::types::PairType;
    use wdl_analysis::types::PrimitiveType;
    use wdl_analysis::types::Type;
    use wdl_ast::SupportedVersion;
    use wdl_ast::version::V1;

    use crate::PrimitiveValue;
    use crate::Value;
    use crate::v1::tests::TestEnv;
    use crate::v1::tests::eval_v1_expr;

    /// Creates an unsorted directory tree with hidden and nested entries.
    fn test_env() -> TestEnv {
        let mut env = TestEnv::default();
        for path in [
            "data/a/nested",
            "data/.hidden_dir",
            "data/empty",
            "data/a-dir",
        ] {
            fs::create_dir_all(env.base_dir().join(path).unwrap().unwrap_local()).unwrap();
        }
        for path in [
            "data/z.txt",
            "data/a/nested/z.txt",
            "data/a/z.txt",
            "data/a/.hidden.txt",
            "data/.hidden_dir/visible.txt",
            "data/a.txt",
            "data/.hidden.txt",
            "data/a/readme.md",
        ] {
            env.write_file(path, path);
        }
        env.insert_name("directory", PrimitiveValue::new_directory("data"));
        env
    }

    /// Extracts the two typed arrays as paths relative to the test environment.
    fn relative_paths(env: &TestEnv, value: Value) -> (Vec<String>, Vec<String>) {
        let pair = value.unwrap_pair();
        let relative = |path: &str| {
            Path::new(path)
                .strip_prefix(env.base_dir().as_local().unwrap())
                .unwrap()
                .components()
                .map(|c| c.as_os_str().to_str().unwrap())
                .collect::<Vec<_>>()
                .join("/")
        };
        let files = pair
            .left()
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| relative(v.as_file().unwrap().as_str()))
            .collect();
        let directories = pair
            .right()
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| relative(v.as_directory().unwrap().as_str()))
            .collect();
        (files, directories)
    }

    #[tokio::test]
    async fn list_defaults() {
        let env = test_env();
        let value = eval_v1_expr(&env, V1::Four, "list(directory)")
            .await
            .unwrap();
        let (files, directories) = relative_paths(&env, value);
        assert_eq!(files, ["data/.hidden.txt", "data/a.txt", "data/z.txt"]);
        assert_eq!(
            directories,
            ["data/.hidden_dir", "data/a", "data/a-dir", "data/empty"]
        );
    }

    #[tokio::test]
    async fn list_requires_wdl_1_4() {
        let env = test_env();
        for version in [V1::Zero, V1::One, V1::Two, V1::Three] {
            let diagnostic = eval_v1_expr(&env, version, "list(directory)")
                .await
                .unwrap_err();
            assert_eq!(
                diagnostic.message(),
                "this use of function `list` requires a minimum WDL version of 1.4"
            );
        }
    }

    #[test]
    fn it_binds_list_with_all_argument_counts() {
        let f = STDLIB.function("list").expect("should have function");
        let version = SupportedVersion::V1(V1::Four);
        let arguments = [
            PrimitiveType::Directory.into(),
            PrimitiveType::Boolean.into(),
            PrimitiveType::Boolean.into(),
            PrimitiveType::String.into(),
        ];
        let expected: Type = PairType::new(
            ArrayType::new(PrimitiveType::File),
            ArrayType::new(PrimitiveType::Directory),
        )
        .into();

        assert_eq!(f.param_min_max(version), Some((1, 4)));
        for count in 1..=arguments.len() {
            let binding = f
                .bind(version, &arguments[..count])
                .expect("binding should succeed");
            assert_eq!(binding.index(), 0);
            assert_eq!(binding.return_type(), &expected);
        }
    }

    #[test]
    fn list_rejects_incorrect_argument_counts() {
        let f = STDLIB.function("list").expect("should have function");
        let version = SupportedVersion::V1(V1::Four);
        assert_eq!(
            f.bind(version, &[]).expect_err("binding should fail"),
            FunctionBindError::TooFewArguments(1)
        );
        assert_eq!(
            f.bind(
                version,
                &[
                    PrimitiveType::Directory.into(),
                    PrimitiveType::Boolean.into(),
                    PrimitiveType::Boolean.into(),
                    PrimitiveType::String.into(),
                    PrimitiveType::String.into(),
                ]
            )
            .expect_err("binding should fail"),
            FunctionBindError::TooManyArguments(4)
        );
    }

    #[test]
    fn list_rejects_incorrect_argument_types() {
        let f = STDLIB.function("list").expect("should have function");
        let valid: [Type; 4] = [
            PrimitiveType::Directory.into(),
            PrimitiveType::Boolean.into(),
            PrimitiveType::Boolean.into(),
            PrimitiveType::String.into(),
        ];

        for (index, invalid) in [
            (0, PrimitiveType::File.into()),
            (0, PrimitiveType::Integer.into()),
            (0, ArrayType::new(PrimitiveType::Directory).into()),
            (1, PrimitiveType::String.into()),
            (2, PrimitiveType::Integer.into()),
            (3, PrimitiveType::Boolean.into()),
        ] {
            let mut arguments = valid.clone();
            arguments[index] = invalid;
            assert_eq!(
                f.bind(SupportedVersion::V1(V1::Four), &arguments)
                    .expect_err("binding should fail"),
                FunctionBindError::ArgumentTypeMismatch {
                    index,
                    expected: format!("{:#}", valid[index]),
                }
            );
        }
    }

    #[test]
    fn list_rejects_optional_and_none_arguments() {
        let f = STDLIB.function("list").expect("should have function");
        let valid: [Type; 4] = [
            PrimitiveType::Directory.into(),
            PrimitiveType::Boolean.into(),
            PrimitiveType::Boolean.into(),
            PrimitiveType::String.into(),
        ];

        for (index, ty) in valid.iter().enumerate() {
            for invalid in [ty.optional(), Type::None] {
                let mut arguments = valid.clone();
                arguments[index] = invalid;
                assert_eq!(
                    f.bind(SupportedVersion::V1(V1::Four), &arguments)
                        .expect_err("binding should fail"),
                    FunctionBindError::ArgumentTypeMismatch {
                        index,
                        expected: format!("{ty:#}"),
                    }
                );
            }
        }

        for (index, ty) in [
            (0, PrimitiveType::String),
            (3, PrimitiveType::File),
            (3, PrimitiveType::Directory),
        ] {
            let mut arguments = valid.clone();
            arguments[index] = Type::from(ty).optional();
            assert_eq!(
                f.bind(SupportedVersion::V1(V1::Four), &arguments)
                    .expect_err("binding should fail"),
                FunctionBindError::ArgumentTypeMismatch {
                    index,
                    expected: format!("{:#}", valid[index]),
                }
            );
        }
    }

    #[test]
    fn it_binds_list_with_existing_coercions() {
        let f = STDLIB.function("list").expect("should have function");
        let version = SupportedVersion::V1(V1::Four);

        for directory in [PrimitiveType::Directory, PrimitiveType::String] {
            for pattern in [
                PrimitiveType::String,
                PrimitiveType::File,
                PrimitiveType::Directory,
            ] {
                let binding = f
                    .bind(
                        version,
                        &[
                            directory.into(),
                            PrimitiveType::Boolean.into(),
                            PrimitiveType::Boolean.into(),
                            pattern.into(),
                        ],
                    )
                    .expect("binding should succeed");
                assert_eq!(
                    binding.return_type().to_string(),
                    "Pair[Array[File], Array[Directory]]"
                );
            }
        }

        let binding = f
            .bind(version, &[const { Type::Union }; 4])
            .expect("binding should succeed");
        assert_eq!(
            binding.return_type().to_string(),
            "Pair[Array[File], Array[Directory]]"
        );
    }

    #[tokio::test]
    async fn list_recursive() {
        let env = test_env();
        let value = eval_v1_expr(&env, V1::Four, "list(directory, true)")
            .await
            .unwrap();
        let (files, directories) = relative_paths(&env, value);
        assert_eq!(
            files,
            [
                "data/.hidden.txt",
                "data/.hidden_dir/visible.txt",
                "data/a.txt",
                "data/a/.hidden.txt",
                "data/a/nested/z.txt",
                "data/a/readme.md",
                "data/a/z.txt",
                "data/z.txt",
            ]
        );
        assert_eq!(
            directories,
            [
                "data/.hidden_dir",
                "data/a",
                "data/a-dir",
                "data/a/nested",
                "data/empty",
            ]
        );
    }

    #[tokio::test]
    async fn list_filters_only_file_basenames() {
        let env = test_env();
        for (pattern, expected) in [
            (
                "*.txt",
                vec![
                    "data/.hidden_dir/visible.txt",
                    "data/a.txt",
                    "data/a/nested/z.txt",
                    "data/a/z.txt",
                    "data/z.txt",
                ],
            ),
            (".*", vec!["data/.hidden.txt", "data/a/.hidden.txt"]),
            (
                "[az].txt",
                vec![
                    "data/a.txt",
                    "data/a/nested/z.txt",
                    "data/a/z.txt",
                    "data/z.txt",
                ],
            ),
            ("a/*.txt", vec![]),
            ("missing*", vec![]),
            ("", vec![]),
        ] {
            let value = eval_v1_expr(
                &env,
                V1::Four,
                &format!("list(directory, true, true, '{pattern}')"),
            )
            .await
            .unwrap();
            let (files, directories) = relative_paths(&env, value);
            assert_eq!(files, expected, "pattern: {pattern}");
            assert_eq!(
                directories,
                [
                    "data/.hidden_dir",
                    "data/a",
                    "data/a-dir",
                    "data/a/nested",
                    "data/empty",
                ],
                "pattern: {pattern}"
            );
        }

        let value = eval_v1_expr(&env, V1::Four, "list(directory, false, true, '*')")
            .await
            .unwrap();
        assert_eq!(relative_paths(&env, value).0, ["data/a.txt", "data/z.txt"]);
    }

    #[tokio::test]
    async fn list_empty_directory() {
        let env = test_env();
        for expr in [
            "list('data/empty')",
            "list('data/empty', true)",
            "list('data/empty', true, false)",
            "list('data/empty', true, false, '*')",
        ] {
            let value = eval_v1_expr(&env, V1::Four, expr).await.unwrap();
            assert_eq!(relative_paths(&env, value), (vec![], vec![]));
        }
    }

    #[tokio::test]
    async fn list_resolves_local_paths_and_file_urls() {
        let mut env = test_env();
        let path = env.base_dir().join("data").unwrap().unwrap_local();
        env.insert_name(
            "absolute",
            PrimitiveValue::new_directory(path.to_str().unwrap()),
        );
        env.insert_name(
            "url",
            PrimitiveValue::new_directory(Url::from_directory_path(path).unwrap()),
        );
        for expr in ["list('data')", "list(absolute)", "list(url)"] {
            let value = eval_v1_expr(&env, V1::Four, expr).await.unwrap();
            assert_eq!(
                relative_paths(&env, value).0,
                ["data/.hidden.txt", "data/a.txt", "data/z.txt"]
            );
        }
    }

    #[tokio::test]
    async fn list_reports_invalid_inputs() {
        let env = test_env();
        for (expr, expected) in [
            ("list('missing')", "failed to read metadata for directory"),
            ("list('data/a.txt')", "is not a directory"),
            (
                "list(directory, false, true, 'invalid{')",
                "error parsing glob 'invalid{'",
            ),
        ] {
            let diagnostic = eval_v1_expr(&env, V1::Four, expr).await.unwrap_err();
            assert!(
                diagnostic
                    .message()
                    .starts_with("call to function `list` failed:"),
                "{diagnostic:?}"
            );
            assert!(diagnostic.message().contains(expected), "{diagnostic:?}");
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn list_includes_but_never_traverses_directory_symlinks() {
        use std::os::unix::fs::symlink;

        let env = test_env();
        let root = env.base_dir().as_local().unwrap();
        fs::create_dir(root.join("outside")).unwrap();
        env.write_file("outside/not-listed.txt", "outside");
        symlink("a.txt", root.join("data/link.txt")).unwrap();
        symlink("../outside", root.join("data/link_dir")).unwrap();
        symlink("..", root.join("data/a/loop")).unwrap();

        let value = eval_v1_expr(&env, V1::Four, "list(directory)")
            .await
            .unwrap();
        let (files, directories) = relative_paths(&env, value);
        assert!(files.iter().any(|p| p == "data/link.txt"));
        assert!(directories.iter().any(|p| p == "data/link_dir"));

        let value = eval_v1_expr(&env, V1::Four, "list(directory, true)")
            .await
            .unwrap();
        let (files, directories) = relative_paths(&env, value);
        assert_eq!(files.len(), 9);
        assert!(files.iter().any(|p| p == "data/link.txt"));
        assert_eq!(directories.len(), 7);
        assert!(directories.iter().any(|p| p == "data/a/loop"));
        assert!(directories.iter().any(|p| p == "data/link_dir"));

        let value = eval_v1_expr(&env, V1::Four, "list(directory, true, false)")
            .await
            .unwrap();
        let (files, directories) = relative_paths(&env, value);
        assert_eq!(files.len(), 8);
        assert_eq!(directories.len(), 5);
        assert!(
            !files
                .iter()
                .chain(&directories)
                .any(|p| p.contains("link") || p.contains("loop"))
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn list_includes_broken_symlinks_unless_excluded() {
        use std::os::unix::fs::symlink;

        let env = test_env();
        symlink(
            "missing",
            env.base_dir().as_local().unwrap().join("data/broken.txt"),
        )
        .unwrap();
        symlink(
            "cycle-b.txt",
            env.base_dir().as_local().unwrap().join("data/cycle-a.txt"),
        )
        .unwrap();
        symlink(
            "cycle-a.txt",
            env.base_dir().as_local().unwrap().join("data/cycle-b.txt"),
        )
        .unwrap();

        let value = eval_v1_expr(&env, V1::Four, "list(directory)")
            .await
            .unwrap();
        assert_eq!(
            relative_paths(&env, value).0,
            [
                "data/.hidden.txt",
                "data/a.txt",
                "data/broken.txt",
                "data/cycle-a.txt",
                "data/cycle-b.txt",
                "data/z.txt",
            ]
        );

        let value = eval_v1_expr(&env, V1::Four, "list(directory, false, true, '*.txt')")
            .await
            .unwrap();
        assert_eq!(
            relative_paths(&env, value).0,
            [
                "data/a.txt",
                "data/broken.txt",
                "data/cycle-a.txt",
                "data/cycle-b.txt",
                "data/z.txt",
            ]
        );

        let value = eval_v1_expr(&env, V1::Four, "list(directory, false, true, '*.bam')")
            .await
            .unwrap();
        assert!(relative_paths(&env, value).0.is_empty());

        let value = eval_v1_expr(&env, V1::Four, "list(directory, false, false)")
            .await
            .unwrap();
        assert_eq!(relative_paths(&env, value).0.len(), 3);
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn list_reports_non_utf8_paths() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let env = test_env();
        fs::write(
            env.base_dir()
                .as_local()
                .unwrap()
                .join("data")
                .join(OsString::from_vec(b"invalid\xff".to_vec())),
            "",
        )
        .unwrap();
        let diagnostic = eval_v1_expr(&env, V1::Four, "list(directory)")
            .await
            .unwrap_err();
        assert!(
            diagnostic
                .message()
                .contains("cannot be represented as UTF-8")
        );
    }

    #[tokio::test]
    async fn list_remote_directory() {
        let mut env = test_env();
        for name in ["a b.txt", "a#b.txt", "a%b.txt"] {
            env.write_file(&format!("data/{name}"), name);
        }
        env.insert_name(
            "remote",
            PrimitiveValue::new_directory("https://example.com/data?version=test"),
        );

        for include_symlinks in [false, true] {
            let value = eval_v1_expr(
                &env,
                V1::Four,
                &format!("list(remote, true, {include_symlinks}, '*.txt')"),
            )
            .await
            .unwrap()
            .unwrap_pair();
            let files: Vec<_> = value
                .left()
                .as_array()
                .unwrap()
                .as_slice()
                .iter()
                .map(|v| v.as_file().unwrap().as_str())
                .collect();
            assert_eq!(
                files,
                [
                    "https://example.com/data/.hidden_dir/visible.txt?version=test",
                    "https://example.com/data/a%20b.txt?version=test",
                    "https://example.com/data/a%23b.txt?version=test",
                    "https://example.com/data/a%25b.txt?version=test",
                    "https://example.com/data/a.txt?version=test",
                    "https://example.com/data/a/nested/z.txt?version=test",
                    "https://example.com/data/a/z.txt?version=test",
                    "https://example.com/data/z.txt?version=test",
                ]
            );
            let directories: Vec<_> = value
                .right()
                .as_array()
                .unwrap()
                .as_slice()
                .iter()
                .map(|v| v.as_directory().unwrap().as_str())
                .collect();
            assert_eq!(
                directories,
                [
                    "https://example.com/data/.hidden_dir?version=test",
                    "https://example.com/data/a?version=test",
                    "https://example.com/data/a/nested?version=test",
                ]
            );
        }

        let value = eval_v1_expr(&env, V1::Four, "list(remote)")
            .await
            .unwrap()
            .unwrap_pair();
        assert_eq!(value.left().as_array().unwrap().len(), 6);
        assert_eq!(value.right().as_array().unwrap().len(), 2);

        let value = eval_v1_expr(&env, V1::Four, "list(remote, true, true, 'missing*')")
            .await
            .unwrap()
            .unwrap_pair();
        assert!(value.left().as_array().unwrap().is_empty());
        assert_eq!(value.right().as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn list_returns_empty_remote_listing() {
        let env = test_env();
        for path in ["missing", "data/a.txt"] {
            let value = eval_v1_expr(
                &env,
                V1::Four,
                &format!("list('https://example.com/{path}')"),
            )
            .await
            .unwrap();
            assert_eq!(relative_paths(&env, value), (vec![], vec![]));
        }
    }

    #[tokio::test]
    async fn list_in_workflow_and_task_contexts() {
        use wdl_analysis::Analyzer;
        use wdl_analysis::Config as AnalysisConfig;
        use wdl_analysis::DiagnosticsConfig;
        use wdl_analysis::FeatureFlags;

        use crate::Config;
        use crate::Engine;
        use crate::Events;
        use crate::WorkflowInputs;

        let env = test_env();
        env.write_file(
            "source.wdl",
            r#"
version 1.4

workflow test {
    input {
        Directory directory
    }

    Pair[Array[File], Array[Directory]] entries = list(directory, true, true, "*.txt")
    Array[File] top_level_files = list(directory).left

    scatter (file in entries.left) {
        call read_file { input: file }
    }

    scatter (subdirectory in entries.right) {
        Int child_count = length(list(subdirectory).left)
    }

    call list_task { input: directory }

    output {
        Array[String] contents = read_file.contents
        Array[Int] child_counts = child_count
        Int top_level_count = length(top_level_files)
        String task_contents = list_task.contents
        Int task_top_level_count = list_task.top_level_count
        Array[File] generated_files = list_task.generated.left
        Array[Directory] generated_directories = list_task.generated.right
    }
}

task read_file {
    input {
        File file
    }

    command <<<>>>

    output {
        String contents = read_string(file)
    }
}

task list_task {
    input {
        Directory directory
    }

    Array[File] files = list(directory, true, true, "*.txt").left
    String directory_path = directory
    Array[File] mapped_files = list(directory_path, true, true, "*.txt").left
    Array[File] top_level_files = list(directory).left

    command <<<
        cat ~{sep(" ", squote(files))} > contents.txt
        cat ~{sep(" ", squote(mapped_files))} > mapped_contents.txt
        cmp contents.txt mapped_contents.txt
        mkdir -p generated/nested
        printf 'first' > generated/first.txt
        printf 'second' > generated/nested/second.txt
    >>>

    output {
        String contents = read_string("contents.txt")
        Int top_level_count = length(top_level_files)
        Pair[Array[File], Array[Directory]] generated = list("generated", true)
    }
}
"#,
        );
        let analyzer = Analyzer::new(
            AnalysisConfig::default()
                .with_diagnostics_config(DiagnosticsConfig::except_all())
                .with_feature_flags(FeatureFlags::default().with_wdl_1_4()),
            |(), _, _, _| async {},
        );
        analyzer
            .add_directory(env.base_dir().as_local().unwrap())
            .await
            .unwrap();
        let results = analyzer.analyze(()).await.unwrap();
        let document = results
            .iter()
            .find(|r| r.document().uri().path().ends_with("source.wdl"))
            .unwrap()
            .document();
        assert!(
            !document.has_errors(),
            "{:?}",
            document.diagnostics().collect::<Vec<_>>()
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            let data = env.base_dir().as_local().unwrap().join("data");
            symlink("missing", data.join("broken")).unwrap();
            symlink("cycle-b", data.join("cycle-a")).unwrap();
            symlink("cycle-a", data.join("cycle-b")).unwrap();
        }

        let engine = Engine::new(Config::local()).await.unwrap();
        let evaluator = engine.create_v1_evaluator(Events::disabled(), Default::default());
        let mut inputs = WorkflowInputs::default();
        inputs.set(
            "directory",
            PrimitiveValue::new_directory(env.base_dir().join("data").unwrap().to_string()),
        );
        let outputs = evaluator
            .evaluate_workflow(
                document,
                inputs,
                &env.base_dir().join("outputs").unwrap().unwrap_local(),
            )
            .await
            .map_err(|e| e.to_string())
            .unwrap();

        let expected_contents = [
            "data/.hidden_dir/visible.txt",
            "data/a.txt",
            "data/a/nested/z.txt",
            "data/a/z.txt",
            "data/z.txt",
        ];
        let contents: Vec<_> = outputs
            .get("contents")
            .unwrap()
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v.as_string().unwrap().as_str())
            .collect();
        assert_eq!(contents, expected_contents);
        assert_eq!(
            outputs
                .get("task_contents")
                .unwrap()
                .as_string()
                .unwrap()
                .as_str(),
            expected_contents.concat()
        );
        let expected_top_level_count = if cfg!(unix) { 6 } else { 3 };
        assert_eq!(
            outputs
                .get("top_level_count")
                .unwrap()
                .as_integer()
                .unwrap(),
            expected_top_level_count
        );
        assert_eq!(
            outputs
                .get("task_top_level_count")
                .unwrap()
                .as_integer()
                .unwrap(),
            expected_top_level_count
        );
        let counts: Vec<_> = outputs
            .get("child_counts")
            .unwrap()
            .as_array()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v.as_integer().unwrap())
            .collect();
        assert_eq!(counts, [1, 3, 0, 1, 0]);
        let files = outputs.get("generated_files").unwrap().as_array().unwrap();
        let contents: Vec<_> = files
            .as_slice()
            .iter()
            .map(|v| fs::read_to_string(v.as_file().unwrap().as_str()).unwrap())
            .collect();
        assert_eq!(contents, ["first", "second"]);
        let directories = outputs
            .get("generated_directories")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(directories.len(), 1);
        assert!(Path::new(directories.as_slice()[0].as_directory().unwrap().as_str()).is_dir());
    }
}
