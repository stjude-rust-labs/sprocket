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

use super::Args;
use crate::commands::module::resolver::ResolverEnvironment;
use crate::config::Config;

pub(super) struct BuiltSource {
    pub(super) source: DependencySource,
    pub(super) note: Option<String>,
}

pub(super) struct DependencySourceBuilder<'a> {
    args: &'a Args,
    config: &'a Config,
    name: &'a DependencyName,
    source_arg: &'a str,
}

impl<'a> DependencySourceBuilder<'a> {
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
enum SourceKind {
    Url,
    Shorthand,
    ScpLike,
    LocalPath,
}

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

fn wildcard_version_source(url: url::Url, path: Option<GitModulePath>) -> DependencySource {
    DependencySource::Git {
        url,
        selector: GitSelector::Version("*".parse().expect("wildcard is a valid requirement")),
        path,
        extra: serde_json::Map::new(),
    }
}

fn local_dependency_path(source: &str, module_path: Option<&str>) -> anyhow::Result<PathBuf> {
    let mut path = PathBuf::from(source);
    if let Some(module_path) = module_path {
        let module_path = module_path.parse::<GitModulePath>()?;
        path.push(module_path.as_path());
    }
    Ok(path)
}

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

fn git_selector_kind(selector: &GitSelector) -> &'static str {
    match selector {
        GitSelector::Version(_) => "version",
        GitSelector::Tag(_) => "tag",
        GitSelector::Branch(_) => "branch",
        GitSelector::Commit(_) => "commit",
    }
}

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
    use super::*;
    use crate::commands::module::Locator;

    #[tokio::test]
    async fn dependency_source_builder_builds_local_path() {
        let args = Args {
            source_or_name: "dep".to_string(),
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
        };
        let config = Config::default();
        let name = "dep".parse().unwrap();

        let built = DependencySourceBuilder::new(&args, &config, &name, "./dep")
            .build()
            .await
            .unwrap();

        assert!(matches!(
            built.source,
            DependencySource::LocalPath { ref path, .. } if path == std::path::Path::new("./dep")
        ));
        assert!(built.note.is_none());
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
