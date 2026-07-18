//! `sprocket dev module update`.

use std::collections::BTreeSet;

use anyhow::Context as _;
use clap::Parser;
use wdl_modules::Lockfile;
use wdl_modules::Resolver as _;
use wdl_modules::dependency::DependencyName;
use wdl_modules::module::Module;
use wdl_modules::resolver::lock::RelockOutcome;
use wdl_modules::resolver::lock::RelockStats;
use wdl_modules::resolver::lock::SignerIdentityMap;
use wdl_modules::resolver::lock::signer_identity_map;
use wdl_modules::resolver::lock::update_relock;

use super::resolver::ResolverEnvironment;
use super::signer_policy::TrustModeArg;
use super::signer_policy::enforce_lockfile_signer_policy;
use super::signer_policy::signer_change_mode;
use crate::commands::CommandResult;
use crate::commands::module::Locator;
use crate::commands::module::LockedProject;
use crate::commands::module::Project;
use crate::commands::module::ProjectUpdate;
use crate::commands::module::dependency_update;
use crate::commands::module::discover;
use crate::commands::module::load_lockfile;
use crate::commands::module::trace_project;
use crate::commands::output::Action;
use crate::commands::output::CommandOutput;
use crate::commands::output::count_noun;
use crate::config::Config;

const UPDATE: Action = Action::new("Updated", "update");

/// Arguments to `sprocket dev module update`.
#[derive(Parser, Debug)]
pub struct Args {
    /// Dependency aliases to update. Empty updates all dependencies.
    pub names: Vec<String>,

    /// Print relock changes without writing `module-lock.json`.
    #[arg(long)]
    pub dry_run: bool,

    /// Override signer trust behavior for this command.
    #[arg(long, value_enum)]
    pub trust_mode: Option<TrustModeArg>,

    /// Shared module locator.
    #[command(flatten)]
    pub locator: Locator,
}

/// Runs `sprocket dev module update`.
pub async fn update(args: Args, config: Config, output: CommandOutput) -> CommandResult<()> {
    tracing::trace!(
        dry_run = args.dry_run,
        requested = args.names.len(),
        "starting `sprocket dev module update`"
    );
    let project = discover(&args.locator)?;
    if args.dry_run {
        trace_project("module update", &project);
        let plan = plan_update(&args, &config, &project).await?;
        tracing::debug!("dry run completed without writing lockfile");
        print_update_outcome(output, &plan.outcome.stats, true);
        return Ok(());
    }

    let project = LockedProject::acquire(project)?;
    trace_project("module update", project.project());
    let plan = plan_update(&args, &config, project.project()).await?;
    enforce_lockfile_signer_policy(
        &plan.existing,
        &plan.outcome.lockfile,
        &plan.identities,
        signer_change_mode(&config, args.trust_mode),
        output,
    )?;
    project.commit(ProjectUpdate::Lockfile(&plan.outcome.lockfile))?;
    tracing::debug!(
        lockfile = %project.project().lockfile_path.display(),
        "wrote module lockfile"
    );
    print_update_outcome(output, &plan.outcome.stats, false);
    Ok(())
}

struct UpdatePlan {
    existing: Lockfile,
    outcome: RelockOutcome,
    identities: SignerIdentityMap,
}

async fn plan_update(
    args: &Args,
    config: &Config,
    project: &Project,
) -> anyhow::Result<UpdatePlan> {
    let existing = load_lockfile(project)?.unwrap_or_default();
    let mut names = BTreeSet::new();
    for raw in &args.names {
        let name: DependencyName = raw
            .parse()
            .with_context(|| format!("invalid dependency name `{raw}`"))?;
        if !project.manifest.dependencies.contains_key(&name) {
            return Err(anyhow::anyhow!(
                "dependency `{}` not found in `module.json`",
                raw
            ));
        }
        names.insert(name);
    }
    tracing::debug!(
        selected = if names.is_empty() {
            project.manifest.dependencies.len()
        } else {
            names.len()
        },
        explicit = !names.is_empty(),
        "selected dependencies for update"
    );

    let module = Module::new(project.manifest.clone(), project.root.clone());
    let environment = ResolverEnvironment::from_config(config)?;
    let resolver = environment.resolver(existing.clone())?;
    let tree = resolver
        .resolve_tree(&module)
        .await
        .map_err(anyhow::Error::from)?;
    tracing::debug!(
        resolved = tree.dependencies.len(),
        "resolved dependency tree for update"
    );
    let outcome = update_relock(&module.manifest, resolver.lockfile(), &tree, &names)
        .map_err(anyhow::Error::from)?;
    let identities = signer_identity_map(&tree);

    Ok(UpdatePlan {
        existing,
        outcome,
        identities,
    })
}

fn print_update_outcome(output: CommandOutput, stats: &RelockStats, dry_run: bool) {
    if stats.updated.is_empty() {
        output.current("module lockfile is up to date");
        return;
    }

    let count = count_noun(stats.updated.len(), "dependency", "dependencies");
    if dry_run {
        output.planned(UPDATE, count);
    } else {
        output.completed(UPDATE, count);
    }
    for change in &stats.updated {
        output.detail(change.name.manifest(), dependency_update(change));
    }
}
