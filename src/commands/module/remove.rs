//! `sprocket dev module remove`.

use clap::Parser;

use super::project::Locator;
use super::project::LockfileWrite;
use super::project::WriteIntent;
use super::project::discover;
use super::project::load_lockfile;
use super::project::trace_project;
use super::project::write_lockfile;
use super::relock::RelockPlanner;
use super::signer_policy::TrustModeArg;
use super::signer_policy::signer_change_mode;
use crate::commands::CommandResult;
use crate::commands::output::Action;
use crate::commands::output::CommandOutput;
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
    trust_mode: Option<TrustModeArg>,

    /// Shared module locator.
    #[command(flatten)]
    locator: Locator,
}

/// Runs `sprocket dev module remove`.
pub async fn remove(args: Args, config: Config, output: CommandOutput) -> CommandResult<()> {
    tracing::trace!(
        no_lock = args.no_lock,
        "starting `sprocket dev module remove`"
    );
    let project = discover(&args.locator)?;
    trace_project("module remove", &project);
    let baseline = load_lockfile(&project)?.unwrap_or_default();
    let mut document = project.document().clone();
    if !document
        .remove_dependency(&args.name)
        .map_err(anyhow::Error::from)?
    {
        tracing::debug!(dependency = args.name, "dependency was not present");
        return Err(anyhow::anyhow!("dependency `{}` not found", args.name).into());
    }

    // Relock against the pending manifest before touching any files so a
    // refused or failed relock leaves the project untouched.
    let relock = if args.no_lock {
        None
    } else {
        Some(
            RelockPlanner::new(&config, &project, &baseline)
                .plan_and_enforce(
                    std::sync::Arc::new(document.manifest().clone()),
                    signer_change_mode(&config, args.trust_mode),
                    output,
                )
                .await?,
        )
    };

    project
        .write_manifest(&document)
        .map_err(anyhow::Error::from)?;
    let written = match relock.as_ref() {
        Some(outcome) => Some(write_lockfile(
            &project,
            &outcome.lockfile,
            document.manifest(),
            WriteIntent::Satisfy,
        )?),
        None => None,
    };
    tracing::debug!(
        dependency = args.name,
        manifest = %project.manifest_path().display(),
        "removed dependency from manifest"
    );

    if let Some(outcome) = relock {
        if written == Some(LockfileWrite::Kept) {
            tracing::debug!("kept the module lockfile another process had already written");
            output.completed(REMOVE, format!("`{}`", args.name));
            output.current("module lockfile is up to date");
            return Ok(());
        }
        tracing::debug!(lockfile = %project.lockfile_path().display(), "wrote module lockfile");
        output.completed(REMOVE, format!("`{}`", args.name));
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
        tracing::debug!("skipped relock after removing dependency");
        output.completed(REMOVE, format!("`{}`", args.name));
        output.detail("Lockfile", "not written (`--no-lock`)");
    }

    Ok(())
}
