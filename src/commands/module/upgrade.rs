//! `sprocket dev module upgrade`.

use std::collections::BTreeSet;

use anyhow::Context as _;
use clap::Parser;
use futures::StreamExt as _;
use futures::TryStreamExt as _;
use wdl_modules::Lockfile;
use wdl_modules::Resolver as _;
use wdl_modules::dependency::DependencyName;
use wdl_modules::dependency::DependencySource;
use wdl_modules::dependency::GitSelector;
use wdl_modules::module::Module;
use wdl_modules::resolver::DependencyScope;
use wdl_modules::resolver::lock::RelockOutcome;
use wdl_modules::resolver::lock::SignerIdentityMap;
use wdl_modules::resolver::lock::signer_identity_map;
use wdl_modules::resolver::lock::update_relock;

use super::display::version_constraint;
use super::project::Locator;
use super::project::Project;
use super::project::WriteIntent;
use super::project::discover;
use super::project::load_lockfile;
use super::project::trace_project;
use super::project::write_lockfile;
use super::resolver::ResolverEnvironment;
use super::signer_policy::TrustModeArg;
use super::signer_policy::enforce_lockfile_signer_policy;
use super::signer_policy::signer_change_mode;
use crate::commands::CommandResult;
use crate::commands::output::Action;
use crate::commands::output::CommandOutput;
use crate::config::Config;

const UPGRADE: Action = Action::new("Upgraded", "upgrade");

const VERSION_DISCOVERY_CONCURRENCY: usize = 8;

/// Arguments to `sprocket dev module upgrade`.
#[derive(Parser, Debug)]
pub struct Args {
    /// Dependency aliases to upgrade. Empty upgrades all eligible dependencies.
    pub names: Vec<String>,

    /// Print manifest selector changes without writing files.
    #[arg(long)]
    pub dry_run: bool,

    /// Override signer trust behavior for this command.
    #[arg(long, value_enum)]
    trust_mode: Option<TrustModeArg>,

    /// Shared module locator.
    #[command(flatten)]
    locator: Locator,
}

/// Runs `sprocket dev module upgrade`.
pub async fn upgrade(args: Args, config: Config, output: CommandOutput) -> CommandResult<()> {
    tracing::trace!(
        dry_run = args.dry_run,
        requested = args.names.len(),
        "starting `sprocket dev module upgrade`"
    );
    let project = discover(&args.locator)?;
    if args.dry_run {
        trace_project("module upgrade", &project);
        let existing = load_lockfile(&project)?.unwrap_or_default();
        let plan = plan_upgrade(&args, &config, &project, &existing).await?;
        print_upgrade_plan(output, plan);
        return Ok(());
    }

    trace_project("module upgrade", &project);
    let existing = load_lockfile(&project)?.unwrap_or_default();
    let plan = plan_upgrade(&args, &config, &project, &existing).await?;
    let UpgradePlan::Changes(changes) = plan else {
        print_upgrade_plan(output, plan);
        return Ok(());
    };
    enforce_lockfile_signer_policy(
        &changes.existing,
        &changes.outcome.lockfile,
        &changes.identities,
        signer_change_mode(&config, args.trust_mode),
        output,
    )?;
    project
        .write_manifest(&changes.manifest)
        .map_err(anyhow::Error::from)?;
    write_lockfile(
        &project,
        &changes.outcome.lockfile,
        changes.manifest.manifest(),
        WriteIntent::Refresh,
    )?;
    tracing::debug!(
        manifest = %project.manifest_path().display(),
        changed = changes.changed.len(),
        "wrote upgraded version selectors"
    );
    tracing::debug!(
        lockfile = %project.lockfile_path().display(),
        "wrote module lockfile"
    );
    let count = changes.changed.len();
    output.completed(
        UPGRADE,
        format!(
            "{count} {}",
            if count == 1 {
                "dependency"
            } else {
                "dependencies"
            }
        ),
    );
    print_upgrade_details(output, &changes.changed);
    Ok(())
}

/// The result of checking version-based dependencies for newer constraints.
enum UpgradePlan {
    /// No selected dependency uses a version selector.
    NoEligible,
    /// Every selected version constraint is current.
    Current,
    /// One or more version constraints and lockfile entries need changes.
    Changes(Box<UpgradeChanges>),
}

/// Manifest, lockfile, and trust changes prepared by an upgrade.
struct UpgradeChanges {
    existing: Lockfile,
    manifest: wdl_modules::project::ManifestDocument,
    changed: Vec<(DependencyName, String, String)>,
    outcome: RelockOutcome,
    identities: SignerIdentityMap,
}

/// Discovers newer versions and prepares the resulting project changes.
async fn plan_upgrade(
    args: &Args,
    config: &Config,
    project: &Project,
    existing: &Lockfile,
) -> anyhow::Result<UpgradePlan> {
    let mut selected = Vec::new();
    if args.names.is_empty() {
        selected.extend(project.manifest().dependencies.keys().cloned());
    } else {
        for raw in &args.names {
            let name: DependencyName = raw
                .parse()
                .with_context(|| format!("invalid dependency name `{raw}`"))?;
            if !project.manifest().dependencies.contains_key(&name) {
                return Err(anyhow::anyhow!(
                    "dependency `{raw}` not found in `module.json`"
                ));
            }
            selected.push(name);
        }
    }
    tracing::debug!(
        selected = selected.len(),
        explicit = !args.names.is_empty(),
        "selected dependencies for upgrade"
    );

    let mut eligible = Vec::new();
    for name in selected {
        let source = project
            .manifest()
            .dependencies
            .get(&name)
            .with_context(|| {
                format!(
                    "dependency `{}` disappeared during upgrade",
                    name.manifest()
                )
            })?;
        match source {
            DependencySource::Git {
                selector: GitSelector::Version(req),
                ..
            } => eligible.push((name, source.clone(), req.to_string())),
            _ => {
                if !args.names.is_empty() {
                    tracing::info!("skipping `{}`; no version selector", name.manifest());
                }
            }
        }
    }

    if eligible.is_empty() {
        tracing::debug!("no dependencies are eligible for upgrade");
        return Ok(UpgradePlan::NoEligible);
    }
    tracing::debug!(
        eligible = eligible.len(),
        "checking latest dependency versions"
    );

    let existing = existing.clone();
    let environment = ResolverEnvironment::from_config(config)?;
    let resolver = environment.resolver(existing.clone())?;

    let discovered = futures::stream::iter(eligible.iter().map(|(name, source, old_req)| async {
        let wildcard_source = wildcard_version_source(source)?;
        let versions = resolver
            .discover_versions(name, &wildcard_source, DependencyScope::TopLevel)
            .await?;
        let highest = versions
            .into_iter()
            .max()
            .with_context(|| format!("no discoverable versions found for `{}`", name.manifest()))?;
        Ok::<_, anyhow::Error>((name.clone(), old_req.clone(), highest))
    }))
    .buffered(VERSION_DISCOVERY_CONCURRENCY)
    .try_collect::<Vec<_>>()
    .await?;
    tracing::debug!(
        discovered = discovered.len(),
        "discovered upgrade candidates"
    );

    let mut changed = Vec::new();
    for (name, old_req, version) in discovered {
        let new_req = format!("^{}.{}.{}", version.major, version.minor, version.patch);
        if old_req != new_req {
            changed.push((name, old_req, new_req));
        }
    }

    if changed.is_empty() {
        tracing::debug!(
            dry_run = args.dry_run,
            "no version selectors need upgrading"
        );
        return Ok(UpgradePlan::Current);
    }

    let mut document = project.document().clone();
    for (name, _, new_req) in &changed {
        let source = document
            .manifest()
            .dependencies
            .get(name)
            .with_context(|| format!("dependency `{}` is not declared", name.manifest()))?;
        let source = with_version_requirement(source, new_req)?;
        document.insert_dependency(name.manifest(), &source)?;
    }
    let module = Module::new(
        std::sync::Arc::new(document.manifest().clone()),
        project.root().to_path_buf(),
    );
    let tree = resolver
        .resolve_tree(&module)
        .await
        .map_err(anyhow::Error::from)?;
    let outcome = update_relock(
        &module.manifest,
        resolver.lockfile(),
        &tree,
        &BTreeSet::new(),
    )
    .map_err(anyhow::Error::from)?;
    let identities = signer_identity_map(&tree);

    Ok(UpgradePlan::Changes(Box::new(UpgradeChanges {
        existing,
        manifest: document,
        changed,
        outcome,
        identities,
    })))
}

/// Prints the result of a dry-run upgrade plan.
fn print_upgrade_plan(output: CommandOutput, plan: UpgradePlan) {
    match plan {
        UpgradePlan::NoEligible => {
            output.current("no version-based dependencies are eligible for upgrade");
        }
        UpgradePlan::Current => output.current("all version constraints"),
        UpgradePlan::Changes(changes) => {
            let count = changes.changed.len();
            output.planned(
                UPGRADE,
                format!(
                    "{count} {}",
                    if count == 1 {
                        "dependency"
                    } else {
                        "dependencies"
                    }
                ),
            );
            print_upgrade_details(output, &changes.changed);
            tracing::debug!(
                changed = changes.changed.len(),
                "dry run completed without writing manifest, lockfile, or trust store"
            );
        }
    }
}

/// Prints old and new version constraints for upgraded dependencies.
fn print_upgrade_details(output: CommandOutput, changed: &[(DependencyName, String, String)]) {
    for (name, old_req, new_req) in changed {
        output.detail(
            name.manifest(),
            format!(
                "{} -> {}",
                version_constraint(old_req),
                version_constraint(new_req)
            ),
        );
    }
}

/// Clones a Git dependency source with a wildcard version selector.
fn wildcard_version_source(source: &DependencySource) -> anyhow::Result<DependencySource> {
    let wildcard = GitSelector::Version("*".parse()?);
    match source {
        DependencySource::Git {
            url, path, extra, ..
        } => Ok(DependencySource::Git {
            url: url.clone(),
            selector: wildcard,
            path: path.clone(),
            extra: extra.clone(),
        }),
        _ => Err(anyhow::anyhow!(
            "dependency source is not a Git version selector"
        )),
    }
}

/// Clones a Git dependency source with a new version requirement selector.
fn with_version_requirement(
    source: &DependencySource,
    requirement: &str,
) -> anyhow::Result<DependencySource> {
    let selector = GitSelector::Version(requirement.parse()?);
    match source {
        DependencySource::Git {
            url, path, extra, ..
        } => Ok(DependencySource::Git {
            url: url.clone(),
            selector,
            path: path.clone(),
            extra: extra.clone(),
        }),
        _ => Err(anyhow::anyhow!(
            "dependency source is not a Git version selector"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_version_requirement_preserves_url_path_and_extra() {
        let source: DependencySource = serde_json::from_str(
            r#"{
              "git": "https://example.com/owner/repo.git",
              "version": "^1.0.0",
              "path": "sub/dir",
              "x-source-extra": "kept"
            }"#,
        )
        .unwrap();

        let upgraded = with_version_requirement(&source, "^2.3.4").unwrap();

        let DependencySource::Git {
            url,
            selector,
            path,
            extra,
        } = upgraded
        else {
            panic!("expected a Git source");
        };
        assert_eq!(url.as_str(), "https://example.com/owner/repo.git");
        assert_eq!(path.as_ref().map(|p| p.as_str()), Some("sub/dir"));
        assert_eq!(extra["x-source-extra"], "kept");
        assert_eq!(selector, GitSelector::Version("^2.3.4".parse().unwrap()));
    }

    #[test]
    fn with_version_requirement_rejects_non_git_source() {
        let source: DependencySource = serde_json::from_str(r#"{"path":"../local"}"#).unwrap();

        assert!(with_version_requirement(&source, "^1.0.0").is_err());
    }
}
