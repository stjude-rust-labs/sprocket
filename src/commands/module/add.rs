//! `sprocket dev module add`.

use std::path::PathBuf;

use anyhow::Context as _;
use clap::ArgAction;
use clap::Parser;
use wdl_modules::dependency::DependencyName;
use wdl_modules::dependency::DependencySource;
use wdl_modules::resolver::GitPlatform;

use super::display::git_selector;
use super::display::short_commit;
use super::manifest::parse_manifest_value;
use super::manifest::read_manifest_value;
use super::manifest::set_dependency;
use super::mutation::LockedProject;
use super::mutation::ProjectUpdate;
use super::project::Locator;
use super::project::discover;
use super::project::load_lockfile;
use super::project::trace_project;
use super::relock::RelockPlanner;
use super::signer_policy::TrustModeArg;
use super::signer_policy::signer_change_mode;
use crate::commands::CommandResult;
use crate::commands::output::Action;
use crate::commands::output::CommandOutput;
use crate::config::Config;

mod source;

const ADD: Action = Action::new("Added", "add");
const LOCK: Action = Action::new("Locked", "lock");

/// Arguments to `sprocket dev module add`.
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
    #[arg(long, conflicts_with_all = ["tag", "branch", "commit"])]
    pub version: Option<String>,

    /// Git tag selector.
    #[arg(long, conflicts_with_all = ["branch", "commit"])]
    pub tag: Option<String>,

    /// Git branch selector.
    #[arg(long, conflicts_with = "commit")]
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
    trust_mode: Option<TrustModeArg>,

    /// Shared module locator.
    #[command(flatten)]
    locator: Locator,
}

/// Runs `sprocket dev module add`.
pub async fn add(args: Args, config: Config, output: CommandOutput) -> CommandResult<()> {
    tracing::trace!(
        no_lock = args.no_lock,
        has_path = args.path.is_some(),
        selector = source::selector_arg_kind(&args),
        "starting `sprocket dev module add`"
    );
    let (name, source_arg) = dependency_name_and_source(&args)?;
    let locked = LockedProject::acquire(discover(&args.locator)?)?;
    let project = locked.project();
    trace_project("module add", project);
    let built = source::DependencySourceBuilder::new(&args, &config, &name, &source_arg)
        .build()
        .await?;
    let source = built.source;
    let mut value = read_manifest_value(&project.manifest_path)?;
    if project.manifest.dependencies.get(&name) == Some(&source) {
        tracing::info!(
            dependency = name.manifest(),
            "dependency already exists with the same source"
        );
        let lockfile = load_lockfile(project)?;
        let lock_is_current = lockfile
            .as_ref()
            .is_some_and(|lockfile| lockfile.satisfies_manifest(&project.manifest));
        if !args.no_lock && !lock_is_current {
            let outcome = RelockPlanner::new(&config, project)
                .plan_and_enforce(
                    project.manifest.clone(),
                    signer_change_mode(&config, args.trust_mode),
                    output,
                )
                .await?;
            locked.commit(ProjectUpdate::Lockfile(&outcome.lockfile))?;
            let dependencies = outcome.lockfile.dependencies.len();
            output.completed(
                LOCK,
                format!(
                    "{dependencies} {}",
                    if dependencies == 1 {
                        "dependency"
                    } else {
                        "dependencies"
                    }
                ),
            );
        } else {
            output.current(format!(
                "`{}` already uses the requested source",
                name.manifest()
            ));
        }
        print_source_details(output, &source);
        if let Some(note) = built.note {
            output.detail("Note", note);
        }
        return Ok(());
    }

    set_dependency(&mut value, name.manifest(), &source)?;
    let relock = if args.no_lock {
        None
    } else {
        let pending_manifest = parse_manifest_value(&value)?;
        Some(
            RelockPlanner::new(&config, project)
                .plan_and_enforce(
                    std::sync::Arc::new(pending_manifest),
                    signer_change_mode(&config, args.trust_mode),
                    output,
                )
                .await?,
        )
    };

    locked.commit(match relock.as_ref() {
        Some(outcome) => ProjectUpdate::Both {
            manifest: &value,
            lockfile: &outcome.lockfile,
        },
        None => ProjectUpdate::Manifest(&value),
    })?;
    tracing::debug!(
        dependency = name.manifest(),
        manifest = %project.manifest_path.display(),
        "wrote dependency to manifest"
    );

    if let Some(outcome) = relock {
        tracing::debug!(lockfile = %project.lockfile_path.display(), "wrote module lockfile");
        output.completed(ADD, format!("`{}`", name.manifest()));
        print_source_details(output, &source);
        if let Some(change) = outcome
            .stats
            .added
            .iter()
            .find(|change| change.name == name)
            && let Some(commit) = change.commit.as_deref()
        {
            output.detail("Resolved", short_commit(commit));
        }
        let dependencies = outcome.lockfile.dependencies.len();
        output.detail(
            "Lockfile",
            format!(
                "{dependencies} {}",
                if dependencies == 1 {
                    "dependency"
                } else {
                    "dependencies"
                }
            ),
        );
    } else {
        tracing::debug!("skipped relock after adding dependency");
        output.completed(ADD, format!("`{}`", name.manifest()));
        print_source_details(output, &source);
        output.detail("Lockfile", "not written (`--no-lock`)");
    }
    if let Some(note) = built.note {
        output.detail("Note", note);
    }

    Ok(())
}

/// Prints the dependency source, selector, and optional module path.
fn print_source_details(output: CommandOutput, source: &DependencySource) {
    match source {
        DependencySource::LocalPath { path, .. } => output.detail("Source", path.display()),
        DependencySource::Git {
            url,
            selector,
            path,
            ..
        } => {
            output.detail("Source", url);
            output.detail("Selector", git_selector(selector));
            if let Some(path) = path {
                output.detail("Path", path);
            }
        }
    }
}

/// Resolves the dependency name and source from positional and named arguments.
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

/// Infers a dependency name from its module path, shorthand, URL, or local
/// path.
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

/// Removes a trailing `.git` suffix from an inferred dependency name.
fn strip_git_suffix(name: &str) -> &str {
    name.strip_suffix(".git").unwrap_or(name)
}
