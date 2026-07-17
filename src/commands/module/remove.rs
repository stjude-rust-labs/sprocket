//! `sprocket dev module remove`.

use clap::Parser;

use super::relock::RelockPlanner;
use super::signer_policy::TrustModeArg;
use super::signer_policy::signer_change_mode;
use crate::commands::CommandResult;
use crate::commands::module::Locator;
use crate::commands::module::LockedProject;
use crate::commands::module::discover;
use crate::commands::module::parse_manifest_value;
use crate::commands::module::read_manifest_value;
use crate::commands::module::remove_dependency;
use crate::commands::module::trace_project;
use crate::commands::output::Action;
use crate::commands::output::CommandOutput;
use crate::commands::output::count_noun;
use crate::config::Config;

const REMOVE: Action = Action::new("Removed", "remove");

/// Arguments to `sprocket dev module remove`.
#[derive(Parser, Debug)]
pub struct Args {
    /// Dependency alias to remove from `module.json`.
    pub name: String,

    /// Skip writing `module-lock.json`.
    #[arg(long)]
    pub no_lock: bool,

    /// Override signer trust behavior for this command.
    #[arg(long, value_enum)]
    pub trust_mode: Option<TrustModeArg>,

    /// Shared module locator.
    #[command(flatten)]
    pub locator: Locator,
}

/// Runs `sprocket dev module remove`.
pub async fn remove(args: Args, config: Config, output: CommandOutput) -> CommandResult<()> {
    tracing::trace!(
        no_lock = args.no_lock,
        "starting `sprocket dev module remove`"
    );
    let locked = LockedProject::acquire(discover(&args.locator)?)?;
    let project = locked.project();
    trace_project("module remove", project);
    let mut value = read_manifest_value(&project.manifest_path)?;
    if !remove_dependency(&mut value, &args.name)? {
        tracing::debug!(dependency = args.name, "dependency was not present");
        return Err(anyhow::anyhow!("dependency `{}` not found", args.name).into());
    }

    // Relock against the pending manifest before touching any files so a
    // refused or failed relock leaves the project untouched.
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

    locked.commit(
        Some(&value),
        relock.as_ref().map(|outcome| &outcome.lockfile),
    )?;
    tracing::debug!(
        dependency = args.name,
        manifest = %project.manifest_path.display(),
        "removed dependency from manifest"
    );

    if let Some(outcome) = relock {
        tracing::debug!(lockfile = %project.lockfile_path.display(), "wrote module lockfile");
        output.completed(REMOVE, format!("`{}`", args.name));
        output.detail(
            "Lockfile",
            count_noun(
                outcome.lockfile.dependencies.len(),
                "dependency",
                "dependencies",
            ),
        );
    } else {
        tracing::debug!("skipped relock after removing dependency");
        output.completed(REMOVE, format!("`{}`", args.name));
        output.detail("Lockfile", "not written (`--no-lock`)");
    }

    Ok(())
}
