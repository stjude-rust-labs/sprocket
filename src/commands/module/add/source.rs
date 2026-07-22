use std::path::PathBuf;

use wdl_modules::Lockfile;
use wdl_modules::Resolver as _;
use wdl_modules::dependency::DependencyName;
use wdl_modules::dependency::DependencySource;
use wdl_modules::dependency::GitModulePath;
use wdl_modules::dependency::GitSelector;
use wdl_modules::resolver::DependencyScope;
use wdl_modules::resolver::GitPlatform;
use wdl_modules::version_requirement::VersionRequirement;

use super::super::resolver::ResolverEnvironment;
use super::Args;
use crate::config::Config;

/// A dependency source and optional discovery note ready for manifest output.
pub(super) struct BuiltSource {
    /// The normalized dependency source.
    pub(super) source: DependencySource,
    /// A note describing an automatically selected fallback.
    pub(super) note: Option<String>,
}

/// Builds a dependency source from the `module add` arguments.
pub(super) struct DependencySourceBuilder<'a> {
    args: &'a Args,
    config: &'a Config,
    name: &'a DependencyName,
    source_arg: &'a str,
}

impl<'a> DependencySourceBuilder<'a> {
    /// Creates a dependency source builder for one dependency.
    pub(super) fn new(
        args: &'a Args,
        config: &'a Config,
        name: &'a DependencyName,
        source_arg: &'a str,
    ) -> Self {
        Self {
            args,
            config,
            name,
            source_arg,
        }
    }

    /// Builds a local or Git dependency source from the supplied arguments.
    pub(super) async fn build(self) -> anyhow::Result<BuiltSource> {
        let Some(url) = resolve_git_url(self.source_arg, self.args.git_platform, self.config)?
        else {
            let path = local_dependency_path(self.source_arg, self.args.path.as_deref())?;
            tracing::trace!(
                source = self.source_arg,
                path = %path.display(),
                "building local path dependency source"
            );
            return Ok(BuiltSource {
                source: DependencySource::LocalPath {
                    path,
                    extra: serde_json::Map::new(),
                },
                note: None,
            });
        };

        if !matches!(url.scheme(), "https" | "http" | "ssh" | "git" | "file") {
            tracing::trace!(
                source = self.source_arg,
                scheme = url.scheme(),
                "treating dependency source as a local path because the URL scheme is not a Git \
                 scheme"
            );
            let path = local_dependency_path(self.source_arg, self.args.path.as_deref())?;
            return Ok(BuiltSource {
                source: DependencySource::LocalPath {
                    path,
                    extra: serde_json::Map::new(),
                },
                note: None,
            });
        }

        let path = self.args.path.as_deref().map(str::parse).transpose()?;
        let (selector, note) = if let Some(commit) = self.args.commit.as_deref() {
            (GitSelector::Commit(commit.parse()?), None)
        } else if let Some(tag) = self.args.tag.clone() {
            (GitSelector::Tag(tag), None)
        } else if let Some(branch) = self.args.branch.clone() {
            (GitSelector::Branch(branch), None)
        } else if let Some(version) = self.args.version.as_deref() {
            (
                GitSelector::Version(version.parse::<VersionRequirement>()?),
                None,
            )
        } else {
            self.discover_latest_selector(&url, path.as_ref()).await?
        };
        tracing::trace!(
            dependency = self.name.manifest(),
            selector = git_selector_kind(&selector),
            has_path = path.is_some(),
            "built Git dependency source"
        );

        Ok(BuiltSource {
            source: DependencySource::Git {
                url,
                selector,
                path,
                extra: serde_json::Map::new(),
            },
            note,
        })
    }

    /// Selects the newest version tag or falls back to the default branch.
    async fn discover_latest_selector(
        &self,
        url: &url::Url,
        path: Option<&GitModulePath>,
    ) -> anyhow::Result<(GitSelector, Option<String>)> {
        let environment = ResolverEnvironment::from_config(self.config)?;
        let resolver = environment.resolver(Lockfile::default())?;
        let temp_source = wildcard_version_source(url.clone(), path.cloned());
        let versions = resolver
            .discover_versions(self.name, &temp_source, DependencyScope::TopLevel)
            .await?;
        tracing::debug!(
            dependency = self.name.manifest(),
            versions = versions.len(),
            has_path = path.is_some(),
            "discovered Git dependency versions"
        );
        let Some(version) = versions.first() else {
            let default_branch = resolver
                .discover_default_branch(self.name, url, DependencyScope::TopLevel)
                .await
                .map_err(|source| {
                    anyhow::anyhow!(
                        "could not determine a default branch for {url} ({source}); specify \
                         --tag, --branch, or --commit"
                    )
                })?;
            let note = if let Some(path) = path {
                format!(
                    "no path-scoped version tags found for `{path}`; tracking branch \
                     `{default_branch}`"
                )
            } else {
                format!("no version tags found; tracking branch `{default_branch}`")
            };
            return Ok((GitSelector::Branch(default_branch), Some(note)));
        };
        Ok((
            GitSelector::Version(
                format!("^{}.{}.{}", version.major, version.minor, version.patch).parse()?,
            ),
            None,
        ))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// The syntax inferred from a dependency source argument.
enum SourceKind {
    /// An absolute URL.
    Url,
    /// A hosted Git repository shorthand.
    Shorthand,
    /// An unsupported scp-like Git location.
    ScpLike,
    /// A local filesystem path.
    LocalPath,
}

/// Classifies a source argument without accessing the filesystem or network.
fn infer_source_kind(raw: &str) -> SourceKind {
    if url::Url::parse(raw).is_ok() {
        SourceKind::Url
    } else if scp_like_parts(raw).is_some() {
        SourceKind::ScpLike
    } else if GitPlatform::shorthand_repo_name(raw).is_some() {
        SourceKind::Shorthand
    } else {
        SourceKind::LocalPath
    }
}

/// Builds a temporary Git source that matches every semantic version.
fn wildcard_version_source(url: url::Url, path: Option<GitModulePath>) -> DependencySource {
    DependencySource::Git {
        url,
        selector: GitSelector::Version("*".parse().expect("wildcard is a valid requirement")),
        path,
        extra: serde_json::Map::new(),
    }
}

/// Joins an optional module subpath onto a local dependency path.
fn local_dependency_path(source: &str, module_path: Option<&str>) -> anyhow::Result<PathBuf> {
    let mut path = PathBuf::from(source);
    if let Some(module_path) = module_path {
        let module_path = module_path.parse::<GitModulePath>()?;
        path.push(module_path.as_path());
    }
    Ok(path)
}

/// Resolves URL and hosted shorthand sources while leaving local paths
/// unresolved.
fn resolve_git_url(
    source: &str,
    platform: Option<GitPlatform>,
    config: &Config,
) -> anyhow::Result<Option<url::Url>> {
    match infer_source_kind(source) {
        SourceKind::Url => {
            let url = url::Url::parse(source).expect("source kind was inferred from URL parsing");
            tracing::trace!(
                source,
                url = %url,
                scheme = url.scheme(),
                "parsed dependency source as a URL"
            );
            Ok(Some(url))
        }
        SourceKind::Shorthand => {
            let platform = platform.unwrap_or(config.modules.default_git_platform);
            let Some(url) = platform.expand_shorthand(source).transpose()? else {
                tracing::trace!(
                    source,
                    "dependency source is not a URL or hosted Git shorthand"
                );
                return Ok(None);
            };
            tracing::debug!(
                source,
                platform = ?platform,
                url = %url,
                "expanded hosted Git shorthand"
            );
            Ok(Some(url))
        }
        SourceKind::ScpLike => {
            let (user_host, path) =
                scp_like_parts(source).expect("source kind was inferred from scp-like syntax");
            anyhow::bail!(
                "`{source}` looks like an scp-style Git URL, which is not supported; use \
                 `ssh://{user_host}/{path}` instead"
            );
        }
        SourceKind::LocalPath => {
            tracing::trace!(
                source,
                "dependency source is not a URL or hosted Git shorthand"
            );
            Ok(None)
        }
    }
}

/// Returns a stable diagnostic label for a Git selector.
fn git_selector_kind(selector: &GitSelector) -> &'static str {
    match selector {
        GitSelector::Version(_) => "version",
        GitSelector::Tag(_) => "tag",
        GitSelector::Branch(_) => "branch",
        GitSelector::Commit(_) => "commit",
    }
}

/// Returns a stable diagnostic label for the requested selector.
pub(super) fn selector_arg_kind(args: &Args) -> &'static str {
    if args.commit.is_some() {
        "commit"
    } else if args.tag.is_some() {
        "tag"
    } else if args.branch.is_some() {
        "branch"
    } else if args.version.is_some() {
        "version"
    } else {
        "auto"
    }
}

/// Splits Git's scp-like `user@host:path` syntax into its parts.
///
/// Returns `None` for anything that is not scp-like (plain paths, Windows
/// drive-letter paths, URLs).
fn scp_like_parts(source: &str) -> Option<(&str, &str)> {
    let (user_host, path) = source.split_once(':')?;
    if user_host.contains('@') && !user_host.contains('/') && !path.starts_with("//") {
        Some((user_host, path))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use anyhow::Context as _;
    use clap::Parser as _;
    use git2::IndexAddOption;
    use git2::Oid;
    use git2::Repository;
    use git2::Signature;

    use super::super::super::project::Locator;
    use super::*;

    /// A temporary Git repository used for source-discovery tests.
    struct GitFixture {
        dir: tempfile::TempDir,
        repo: std::path::PathBuf,
    }

    impl GitFixture {
        /// Creates an empty Git repository and isolated module cache.
        fn new() -> anyhow::Result<Self> {
            let dir = tempfile::tempdir()?;
            let repo = dir.path().join("repo");
            fs::create_dir(&repo)?;
            Repository::init(&repo)?;
            Ok(Self { dir, repo })
        }

        /// Returns the repository as a `file` URL.
        fn url(&self) -> anyhow::Result<url::Url> {
            url::Url::from_file_path(&self.repo)
                .map_err(|()| anyhow::anyhow!("fixture path is not a valid file URL"))
        }

        /// Builds configuration that permits the fixture's `file` URL.
        fn config(&self) -> Config {
            let mut config = Config::default();
            config.modules.cache_path = Some(self.dir.path().join("cache"));
            config.modules.allowed_schemes = vec!["file".to_string()];
            config
        }

        /// Writes files and commits the resulting repository state.
        fn commit(&self, files: &[(&str, &str)]) -> anyhow::Result<Oid> {
            for (path, contents) in files {
                let path = self.repo.join(path);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(path, contents)?;
            }

            let repository = Repository::open(&self.repo)?;
            let mut index = repository.index()?;
            index.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)?;
            let tree = repository.find_tree(index.write_tree()?)?;
            let signature = Signature::now("sprocket test", "test@example.com")?;
            let commit = match repository.head().and_then(|head| head.peel_to_commit()) {
                Ok(parent) => repository.commit(
                    Some("HEAD"),
                    &signature,
                    &signature,
                    "fixture commit",
                    &tree,
                    &[&parent],
                )?,
                Err(_) => repository.commit(
                    Some("HEAD"),
                    &signature,
                    &signature,
                    "fixture commit",
                    &tree,
                    &[],
                )?,
            };
            Ok(commit)
        }

        /// Creates a lightweight tag at a fixture commit.
        fn tag(&self, name: &str, commit: Oid) -> anyhow::Result<()> {
            let repository = Repository::open(&self.repo)?;
            repository.reference(&format!("refs/tags/{name}"), commit, true, "fixture tag")?;
            Ok(())
        }

        /// Returns the branch referenced by the fixture's `HEAD`.
        fn default_branch(&self) -> anyhow::Result<String> {
            Repository::open(&self.repo)?
                .head()?
                .shorthand()
                .context("fixture HEAD has no branch")
                .map(str::to_owned)
        }
    }

    /// Builds default `module add` arguments for a source.
    fn make_args(source: &str) -> Args {
        Args {
            source_or_name: source.to_string(),
            source: None,
            name: None,
            version: None,
            tag: None,
            branch: None,
            commit: None,
            path: None,
            git_platform: None,
            no_lock: false,
            trust_mode: None,
            locator: Locator {
                manifest_path: None,
            },
        }
    }

    /// Returns the standard dependency name used by source tests.
    fn dependency_name() -> DependencyName {
        // SAFETY: `dep` is a valid dependency name.
        "dep".parse().unwrap()
    }

    #[tokio::test]
    async fn dependency_source_builder_builds_local_path() {
        let args = make_args("dep");
        let config = Config::default();
        let name = dependency_name();

        let built = DependencySourceBuilder::new(&args, &config, &name, "./dep")
            .build()
            .await
            // SAFETY: local source construction cannot fail for a valid path.
            .unwrap();

        assert!(matches!(
            built.source,
            DependencySource::LocalPath { ref path, .. } if path == std::path::Path::new("./dep")
        ));
        assert!(built.note.is_none());
    }

    #[test]
    fn infer_source_kind_distinguishes_urls_shorthands_and_local_paths() {
        assert_eq!(infer_source_kind("file:///repo"), SourceKind::Url);
        assert_eq!(infer_source_kind("openwdl/wdl"), SourceKind::Shorthand);
        assert_eq!(infer_source_kind("./openwdl/wdl"), SourceKind::LocalPath);
    }

    #[tokio::test]
    async fn dependency_source_builder_preserves_explicit_selectors() {
        let source = "file:///repo";
        let config = Config::default();

        let mut args = make_args(source);
        args.tag = Some("v1.2.3".to_string());
        let built = DependencySourceBuilder::new(&args, &config, &dependency_name(), source)
            .build()
            .await
            // SAFETY: explicit tag construction does not access the repository.
            .unwrap();
        assert!(matches!(
            built.source,
            DependencySource::Git {
                selector: GitSelector::Tag(tag),
                ..
            } if tag == "v1.2.3"
        ));

        let mut args = make_args(source);
        args.branch = Some("main".to_string());
        let built = DependencySourceBuilder::new(&args, &config, &dependency_name(), source)
            .build()
            .await
            // SAFETY: explicit branch construction does not access the repository.
            .unwrap();
        assert!(matches!(
            built.source,
            DependencySource::Git {
                selector: GitSelector::Branch(branch),
                ..
            } if branch == "main"
        ));

        let mut args = make_args(source);
        args.commit = Some("0123456789012345678901234567890123456789".to_string());
        let built = DependencySourceBuilder::new(&args, &config, &dependency_name(), source)
            .build()
            .await
            // SAFETY: explicit commit construction does not access the repository.
            .unwrap();
        assert!(matches!(
            built.source,
            DependencySource::Git {
                selector: GitSelector::Commit(_),
                ..
            }
        ));

        let mut args = make_args(source);
        args.version = Some("^1.2".to_string());
        let built = DependencySourceBuilder::new(&args, &config, &dependency_name(), source)
            .build()
            .await
            // SAFETY: explicit version construction does not access the repository.
            .unwrap();
        assert!(matches!(
            built.source,
            DependencySource::Git {
                selector: GitSelector::Version(_),
                ..
            }
        ));
    }

    #[test]
    fn selector_arguments_report_conflicts_at_the_cli_boundary() {
        for selectors in [
            ["--version", "^1.0", "--tag", "v1.0.0"],
            ["--tag", "v1.0.0", "--branch", "main"],
            [
                "--branch",
                "main",
                "--commit",
                "0123456789012345678901234567890123456789",
            ],
        ] {
            let result = Args::try_parse_from(
                ["sprocket", "file:///repo"]
                    .into_iter()
                    .chain(selectors)
                    .collect::<Vec<_>>(),
            );
            assert!(result.is_err(), "selectors should conflict: {selectors:?}");
        }
    }

    #[tokio::test]
    async fn dependency_source_builder_discovers_latest_version_tag() -> anyhow::Result<()> {
        let fixture = GitFixture::new()?;
        let commit = fixture.commit(&[("module.json", "{}")])?;
        fixture.tag("v1.2.3", commit)?;
        fixture.tag("v1.4.0", commit)?;
        let source = fixture.url()?.to_string();
        let args = make_args(&source);
        let config = fixture.config();
        let built = DependencySourceBuilder::new(&args, &config, &dependency_name(), &source)
            .build()
            .await?;

        let DependencySource::Git {
            selector: GitSelector::Version(requirement),
            ..
        } = built.source
        else {
            anyhow::bail!("expected a discovered version selector");
        };
        assert_eq!(requirement.to_string(), "^1.4.0");
        Ok(())
    }

    #[tokio::test]
    async fn dependency_source_builder_discovers_path_scoped_version_tag() -> anyhow::Result<()> {
        let fixture = GitFixture::new()?;
        let commit = fixture.commit(&[("pkg/module.json", "{}")])?;
        fixture.tag("v9.0.0", commit)?;
        fixture.tag("pkg/v1.2.3", commit)?;
        fixture.tag("pkg/v1.4.0", commit)?;
        let source = fixture.url()?.to_string();
        let mut args = make_args(&source);
        args.path = Some("pkg".to_string());
        let config = fixture.config();
        let built = DependencySourceBuilder::new(&args, &config, &dependency_name(), &source)
            .build()
            .await?;

        let DependencySource::Git {
            selector: GitSelector::Version(requirement),
            path: Some(path),
            ..
        } = built.source
        else {
            anyhow::bail!("expected a path-scoped version selector");
        };
        assert_eq!(path.as_str(), "pkg");
        assert_eq!(requirement.to_string(), "^1.4.0");
        Ok(())
    }

    #[tokio::test]
    async fn dependency_source_builder_tracks_default_branch_without_tags() -> anyhow::Result<()> {
        let fixture = GitFixture::new()?;
        fixture.commit(&[("pkg/module.json", "{}")])?;
        let branch = fixture.default_branch()?;
        let source = fixture.url()?.to_string();
        let mut args = make_args(&source);
        args.path = Some("pkg".to_string());
        let config = fixture.config();
        let built = DependencySourceBuilder::new(&args, &config, &dependency_name(), &source)
            .build()
            .await?;

        let DependencySource::Git {
            selector: GitSelector::Branch(selected),
            ..
        } = built.source
        else {
            anyhow::bail!("expected a default-branch selector");
        };
        assert_eq!(selected, branch);
        let expected_note =
            format!("no path-scoped version tags found for `pkg`; tracking branch `{branch}`");
        assert_eq!(built.note.as_deref(), Some(expected_note.as_str()));
        Ok(())
    }

    #[tokio::test]
    async fn dependency_source_builder_reports_missing_tags_and_default_branch()
    -> anyhow::Result<()> {
        let fixture = GitFixture::new()?;
        let source = fixture.url()?.to_string();
        let args = make_args(&source);
        let config = fixture.config();
        let error = match DependencySourceBuilder::new(&args, &config, &dependency_name(), &source)
            .build()
            .await
        {
            Ok(_) => anyhow::bail!("an empty repository unexpectedly provided a selector"),
            Err(error) => error,
        };

        let message = error.to_string();
        assert!(message.contains("could not determine a default branch"));
        assert!(message.contains("specify --tag, --branch, or --commit"));
        Ok(())
    }

    #[test]
    fn scp_like_parts_detects_scp_syntax_only() {
        assert_eq!(
            scp_like_parts("git@github.com:org/repo.git"),
            Some(("git@github.com", "org/repo.git"))
        );
        assert_eq!(scp_like_parts("./local/path"), None);
        assert_eq!(scp_like_parts("C:\\repos\\module"), None);
        assert_eq!(scp_like_parts("https://example.com/repo"), None);
        assert_eq!(scp_like_parts("org/repo"), None);
    }

    #[test]
    fn infer_source_kind_preserves_scp_like_detection() {
        assert_eq!(
            infer_source_kind("git@github.com:org/repo.git"),
            SourceKind::ScpLike
        );
    }
}
