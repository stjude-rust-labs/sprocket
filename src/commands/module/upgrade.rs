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
use wdl_modules::resolver::lock::RelockStats;
use wdl_modules::resolver::lock::SignerIdentityMap;
use wdl_modules::resolver::lock::signer_identity_map;
use wdl_modules::resolver::lock::update_relock;

use super::display::dependency_update;
use super::display::version_constraint;
use super::manifest::parse_manifest_value;
use super::manifest::read_manifest_value;
use super::mutation::LockedProject;
use super::mutation::ProjectUpdate;
use super::project::Locator;
use super::project::Project;
use super::project::discover;
use super::project::load_lockfile;
use super::project::trace_project;
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
        let plan = plan_upgrade(&args, &config, &project).await?;
        print_upgrade_plan(output, plan);
        return Ok(());
    }

    let project = LockedProject::acquire(project)?;
    trace_project("module upgrade", project.project());
    let plan = plan_upgrade(&args, &config, project.project()).await?;
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
    project.commit(ProjectUpdate::Both {
        manifest: &changes.manifest_value,
        lockfile: &changes.outcome.lockfile,
    })?;
    tracing::debug!(
        manifest = %project.project().manifest_path.display(),
        changed = changes.changed.len(),
        "wrote upgraded version selectors"
    );
    tracing::debug!(
        lockfile = %project.project().lockfile_path.display(),
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
    print_lockfile_change_details(output, &changes.outcome.stats);
    Ok(())
}

enum UpgradePlan {
    NoEligible,
    Current,
    Changes(Box<UpgradeChanges>),
}

struct UpgradeChanges {
    existing: Lockfile,
    manifest_value: serde_json::Value,
    changed: Vec<(DependencyName, String, String)>,
    outcome: RelockOutcome,
    identities: SignerIdentityMap,
}

async fn plan_upgrade(
    args: &Args,
    config: &Config,
    project: &Project,
) -> anyhow::Result<UpgradePlan> {
    let mut selected = Vec::new();
    if args.names.is_empty() {
        selected.extend(project.manifest.dependencies.keys().cloned());
    } else {
        for raw in &args.names {
            let name: DependencyName = raw
                .parse()
                .with_context(|| format!("invalid dependency name `{raw}`"))?;
            if !project.manifest.dependencies.contains_key(&name) {
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
        let source = project.manifest.dependencies.get(&name).with_context(|| {
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

    let existing = load_lockfile(project)?.unwrap_or_default();
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

    let mut manifest_value = read_manifest_value(&project.manifest_path)?;
    for (name, _, new_req) in &changed {
        set_version_selector(&mut manifest_value, name.manifest(), new_req)?;
    }
    let pending_manifest = parse_manifest_value(&manifest_value)?;
    let module = Module::new(std::sync::Arc::new(pending_manifest), project.root.clone());
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
        manifest_value,
        changed,
        outcome,
        identities,
    })))
}

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
            print_lockfile_change_details(output, &changes.outcome.stats);
            tracing::debug!(
                changed = changes.changed.len(),
                "dry run completed without writing manifest, lockfile, or trust store"
            );
        }
    }
}

fn print_lockfile_change_details(output: CommandOutput, stats: &RelockStats) {
    for change in &stats.updated {
        output.detail(change.name.manifest(), dependency_update(change));
    }
}

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

fn set_version_selector(
    manifest: &mut serde_json::Value,
    name: &str,
    version_req: &str,
) -> anyhow::Result<()> {
    let deps = manifest
        .get_mut("dependencies")
        .and_then(serde_json::Value::as_object_mut)
        .with_context(|| "`dependencies` in `module.json` must be an object")?;
    let dep = deps
        .get_mut(name)
        .and_then(serde_json::Value::as_object_mut)
        .with_context(|| format!("dependency `{name}` in `module.json` must be an object"))?;

    if !dep.contains_key("version") {
        anyhow::bail!("dependency `{name}` does not contain a `version` selector");
    }
    dep.insert(
        "version".to_string(),
        serde_json::Value::String(version_req.to_string()),
    );
    Ok(())
}
