//! `sprocket module add`.

use std::path::PathBuf;

use anyhow::Context as _;
use clap::ArgAction;
use clap::Parser;
use wdl_modules::Lockfile;
use wdl_modules::Resolver as _;
use wdl_modules::dependency::DependencyName;
use wdl_modules::dependency::DependencySource;
use wdl_modules::dependency::GitModulePath;
use wdl_modules::dependency::GitSelector;
use wdl_modules::resolver::DependencyScope;
use wdl_modules::resolver::GitPlatform;
use wdl_modules::version_requirement::VersionRequirement;

use crate::commands::CommandResult;
use crate::commands::module::ActionColor;
use crate::commands::module::Locator;
use crate::commands::module::TrustModeArg;
use crate::commands::module::build_resolver;
use crate::commands::module::discover;
use crate::commands::module::parse_manifest_value;
use crate::commands::module::print_action;
use crate::commands::module::print_relock_summary;
use crate::commands::module::read_manifest_value;
use crate::commands::module::resolve_relock_for_manifest;
use crate::commands::module::set_dependency;
use crate::commands::module::signer_change_mode;
use crate::commands::module::trace_project;
use crate::commands::module::write_lockfile;
use crate::commands::module::write_manifest_value;
use crate::config::Config;

/// Arguments to `sprocket module add`.
#[derive(Parser, Debug)]
#[command(disable_version_flag = true)]
pub struct Args {
    /// Dependency source, or a dependency alias when `SOURCE` is also provided.
    pub source_or_name: String,

    /// Dependency source when the alias is provided positionally.
    pub source: Option<String>,

    /// Dependency alias written to `module.json`.
    #[arg(long)]
    pub name: Option<String>,

    /// Semver requirement for a Git dependency.
    #[arg(long)]
    pub version: Option<String>,

    /// Git tag selector.
    #[arg(long)]
    pub tag: Option<String>,

    /// Git branch selector.
    #[arg(long)]
    pub branch: Option<String>,

    /// Git commit selector.
    #[arg(long)]
    pub commit: Option<String>,

    /// Optional path inside a Git repository.
    #[arg(long)]
    pub path: Option<String>,

    /// Hosted Git platform used to expand `owner/repo` shorthand.
    #[arg(long)]
    pub git_platform: Option<GitPlatform>,

    /// Skip writing `module-lock.json`.
    #[arg(long, action = ArgAction::SetTrue)]
    pub no_lock: bool,

    /// Override signer trust behavior for this command.
    #[arg(long, value_enum)]
    pub trust_mode: Option<TrustModeArg>,

    /// Shared module locator.
    #[command(flatten)]
    pub locator: Locator,
}

/// Runs `sprocket module add`.
pub async fn add(args: Args, config: Config, colorize: bool) -> CommandResult<()> {
    tracing::trace!(
        no_lock = args.no_lock,
        has_path = args.path.is_some(),
        selector = selector_arg_kind(&args),
        "starting `sprocket module add`"
    );
    let (name, source_arg) = dependency_name_and_source(&args)?;
    let project = discover(&args.locator)?;
    trace_project("module add", &project);
    let source = build_source(&args, &source_arg, &config, &project, &name).await?;

    let mut value = read_manifest_value(&project.manifest_path)?;
    if project.manifest.dependencies.get(&name) == Some(&source) {
        tracing::info!(
            dependency = name.manifest(),
            "dependency already exists with the same source"
        );
        print_action(
            "Skipped",
            format!(
                "`{}` already exists in the module's dependencies",
                name.manifest()
            ),
            colorize,
            ActionColor::Yellow,
        );
        return Ok(());
    }

    set_dependency(&mut value, name.manifest(), &source)?;
    let relock = if args.no_lock {
        None
    } else {
        let pending_manifest = parse_manifest_value(&value)?;
        Some(
            resolve_relock_for_manifest(
                &config,
                &project,
                std::sync::Arc::new(pending_manifest),
                signer_change_mode(&config, args.trust_mode),
                colorize,
            )
            .await?,
        )
    };

    write_manifest_value(&project.manifest_path, &value)?;
    tracing::debug!(
        dependency = name.manifest(),
        manifest = %project.manifest_path.display(),
        "wrote dependency to manifest"
    );

    if let Some(outcome) = relock {
        write_lockfile(&project, &outcome.lockfile)?;
        tracing::debug!(lockfile = %project.lockfile_path.display(), "wrote module lockfile");
        print_relock_summary(&outcome.stats, colorize);
    } else {
        tracing::debug!("skipped relock after adding dependency");
        print_action(
            "Added",
            format!("`{}`", name.manifest()),
            colorize,
            ActionColor::Green,
        );
    }

    Ok(())
}

async fn build_source(
    args: &Args,
    source_arg: &str,
    config: &Config,
    project: &crate::commands::module::Project,
    name: &DependencyName,
) -> anyhow::Result<DependencySource> {
    let Some(url) = resolve_git_url(source_arg, args.git_platform, config)? else {
        let path = local_dependency_path(source_arg, args.path.as_deref())?;
        tracing::trace!(
            source = source_arg,
            path = %path.display(),
            "building local path dependency source"
        );
        return Ok(DependencySource::LocalPath {
            path,
            extra: serde_json::Map::new(),
        });
    };

    if !matches!(url.scheme(), "https" | "http" | "ssh" | "git" | "file") {
        tracing::trace!(
            source = source_arg,
            scheme = url.scheme(),
            "treating dependency source as a local path because the URL scheme is not a Git scheme"
        );
        let path = local_dependency_path(source_arg, args.path.as_deref())?;
        return Ok(DependencySource::LocalPath {
            path,
            extra: serde_json::Map::new(),
        });
    }

    let path = args.path.as_deref().map(str::parse).transpose()?;
    let selector = if let Some(commit) = args.commit.as_deref() {
        GitSelector::Commit(commit.parse()?)
    } else if let Some(tag) = args.tag.clone() {
        GitSelector::Tag(tag)
    } else if let Some(branch) = args.branch.clone() {
        GitSelector::Branch(branch)
    } else if let Some(version) = args.version.as_deref() {
        GitSelector::Version(version.parse::<VersionRequirement>()?)
    } else {
        discover_latest_selector(config, project, name, &url, path.as_ref()).await?
    };
    tracing::trace!(
        dependency = name.manifest(),
        selector = git_selector_kind(&selector),
        has_path = path.is_some(),
        "built Git dependency source"
    );

    Ok(DependencySource::Git {
        url,
        selector,
        path,
        extra: serde_json::Map::new(),
    })
}

fn local_dependency_path(source: &str, module_path: Option<&str>) -> anyhow::Result<PathBuf> {
    let mut path = PathBuf::from(source);
    if let Some(module_path) = module_path {
        let module_path = module_path.parse::<GitModulePath>()?;
        path.push(module_path.as_path());
    }
    Ok(path)
}

fn dependency_name_and_source(args: &Args) -> anyhow::Result<(DependencyName, String)> {
    let (name, source) = if let Some(source) = &args.source {
        if args.name.is_some() {
            anyhow::bail!(
                "`--name` cannot be used when the dependency alias is provided positionally"
            );
        }
        tracing::trace!(
            dependency = args.source_or_name,
            source,
            "using positional dependency name"
        );
        (args.source_or_name.clone(), source.clone())
    } else {
        let source = args.source_or_name.clone();
        let name = args
            .name
            .clone()
            .inspect(|name| {
                tracing::trace!(
                    dependency = name,
                    source,
                    "using explicit dependency name from `--name`"
                );
            })
            .map(Ok)
            .unwrap_or_else(|| infer_dependency_name(&source, args.path.as_deref()))?;
        (name, source)
    };
    let parsed = name
        .parse()
        .with_context(|| format!("invalid dependency name `{name}`"))?;
    Ok((parsed, source))
}

fn infer_dependency_name(source: &str, module_path: Option<&str>) -> anyhow::Result<String> {
    if let Some(path) = module_path
        && let Some(name) = path.split('/').rev().find(|segment| !segment.is_empty())
    {
        let name = strip_git_suffix(name).to_string();
        tracing::trace!(
            dependency = name,
            source,
            path,
            "inferred dependency name from Git module path"
        );
        return Ok(name);
    }

    if let Some(repo) = GitPlatform::shorthand_repo_name(source) {
        tracing::trace!(
            dependency = repo,
            source,
            "inferred dependency name from hosted Git shorthand"
        );
        return Ok(repo);
    }

    if let Ok(url) = url::Url::parse(source)
        && let Some(segment) = url.path_segments().and_then(Iterator::last)
        && !segment.is_empty()
    {
        let name = strip_git_suffix(segment).to_string();
        tracing::trace!(
            dependency = name,
            source,
            "inferred dependency name from Git URL path"
        );
        return Ok(name);
    }

    let path = PathBuf::from(source);
    if let Some(name) = path.file_name().and_then(|name| name.to_str())
        && !name.is_empty()
    {
        let name = strip_git_suffix(name).to_string();
        tracing::trace!(
            dependency = name,
            source,
            "inferred dependency name from local path"
        );
        return Ok(name);
    }

    anyhow::bail!("could not infer a dependency name from `{source}`; specify `--name`")
}

fn resolve_git_url(
    source: &str,
    platform: Option<GitPlatform>,
    config: &Config,
) -> anyhow::Result<Option<url::Url>> {
    if let Ok(url) = url::Url::parse(source) {
        tracing::trace!(
            source,
            url = %url,
            scheme = url.scheme(),
            "parsed dependency source as a URL"
        );
        return Ok(Some(url));
    }

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

fn strip_git_suffix(name: &str) -> &str {
    name.strip_suffix(".git").unwrap_or(name)
}

async fn discover_latest_selector(
    config: &Config,
    project: &crate::commands::module::Project,
    name: &DependencyName,
    url: &url::Url,
    path: Option<&GitModulePath>,
) -> anyhow::Result<GitSelector> {
    let resolver = build_resolver(config, project, Lockfile::default())?;
    let temp_source = DependencySource::Git {
        url: url.clone(),
        selector: GitSelector::Version("*".parse()?),
        path: path.cloned(),
        extra: serde_json::Map::new(),
    };
    let versions = resolver
        .discover_versions(name, &temp_source, DependencyScope::TopLevel)
        .await?;
    tracing::debug!(
        dependency = name.manifest(),
        versions = versions.len(),
        has_path = path.is_some(),
        "discovered Git dependency versions"
    );
    let Some(version) = versions.first() else {
        let default_branch = resolver
            .discover_default_branch(name, url, DependencyScope::TopLevel)
            .await
            .map_err(|source| {
                anyhow::anyhow!(
                    "could not determine a default branch for {url} ({source}); specify --tag, \
                     --branch, or --commit"
                )
            })?;
        if let Some(path) = path {
            tracing::info!(
                "no path-scoped Git version tags found for `{path}`; tracking branch \
                 `{default_branch}`"
            );
        } else {
            tracing::info!("no Git version tags found; tracking branch `{default_branch}`");
        }
        return Ok(GitSelector::Branch(default_branch));
    };
    Ok(GitSelector::Version(
        format!("^{}.{}.{}", version.major, version.minor, version.patch).parse()?,
    ))
}

fn selector_arg_kind(args: &Args) -> &'static str {
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

fn git_selector_kind(selector: &GitSelector) -> &'static str {
    match selector {
        GitSelector::Version(_) => "version",
        GitSelector::Tag(_) => "tag",
        GitSelector::Branch(_) => "branch",
        GitSelector::Commit(_) => "commit",
    }
}
