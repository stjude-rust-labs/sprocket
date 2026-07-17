//! `sprocket dev module lock`.

use clap::Parser;
use wdl_modules::resolver::lock::RelockStats;

use super::relock::RelockPlanner;
use super::signer_policy::TrustModeArg;
use super::signer_policy::signer_change_mode;
use crate::commands::CommandResult;
use crate::commands::module::Locator;
use crate::commands::module::LockedProject;
use crate::commands::module::discover;
use crate::commands::module::load_lockfile;
use crate::commands::module::trace_project;
use crate::commands::output::Action;
use crate::commands::output::CommandOutput;
use crate::commands::output::count_noun;
use crate::config::Config;

const LOCK: Action = Action::new("Locked", "lock");

/// Arguments to `sprocket dev module lock`.
#[derive(Parser, Debug)]
pub struct Args {
    /// Fail if `module-lock.json` is missing or out of date.
    #[arg(long)]
    pub locked: bool,

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

/// Runs `sprocket dev module lock`.
pub async fn lock(args: Args, config: Config, output: CommandOutput) -> CommandResult<()> {
    tracing::trace!(
        locked = args.locked,
        dry_run = args.dry_run,
        "starting `sprocket dev module lock`"
    );
    let project = discover(&args.locator)?;
    trace_project("module lock", &project);
    let lock = load_lockfile(&project)?;
    let satisfied = lock
        .as_ref()
        .is_some_and(|l| l.satisfies_manifest(&project.manifest));
    tracing::debug!(
        lockfile_present = lock.is_some(),
        satisfied,
        "loaded module lockfile"
    );

    if args.locked {
        if !satisfied {
            tracing::debug!("`--locked` failed because lockfile is not current");
            return Err(
                anyhow::anyhow!("`module-lock.json` is out of date with `module.json`").into(),
            );
        }
        tracing::debug!("`--locked` succeeded");
        output.current("module lockfile is up to date");
        return Ok(());
    }

    if satisfied {
        tracing::debug!("skipped relock because lockfile is current");
        output.current("module lockfile is up to date");
        return Ok(());
    }

    if args.dry_run {
        let plan = RelockPlanner::new(&config, &project)
            .plan(project.manifest.clone())
            .await?;
        tracing::debug!("dry run completed without writing lockfile or trust store");
        let changes = relock_change_count(&plan.outcome.stats);
        output.planned(
            LOCK,
            count_noun(changes, "dependency change", "dependency changes"),
        );
        output.detail(
            "Lockfile",
            count_noun(
                plan.outcome.lockfile.dependencies.len(),
                "dependency",
                "dependencies",
            ),
        );
        return Ok(());
    }

    let project = LockedProject::acquire(project)?;
    if load_lockfile(project.project())?
        .as_ref()
        .is_some_and(|lockfile| lockfile.satisfies_manifest(&project.project().manifest))
    {
        output.current("module lockfile is up to date");
        return Ok(());
    }

    let outcome = RelockPlanner::new(&config, project.project())
        .plan_and_enforce(
            project.project().manifest.clone(),
            signer_change_mode(&config, args.trust_mode),
            output,
        )
        .await?;
    project.commit(None, Some(&outcome.lockfile))?;
    tracing::debug!(
        lockfile = %project.project().lockfile_path.display(),
        "wrote module lockfile"
    );
    output.completed(
        LOCK,
        count_noun(
            outcome.lockfile.dependencies.len(),
            "dependency",
            "dependencies",
        ),
    );
    let changes = relock_change_count(&outcome.stats);
    if changes > 0 {
        output.detail("Changed", count_noun(changes, "dependency", "dependencies"));
    }
    Ok(())
}

fn relock_change_count(stats: &RelockStats) -> usize {
    stats.added.len() + stats.removed.len() + stats.updated.len()
}
