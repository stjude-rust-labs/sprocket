//! `sprocket module remove`.

use clap::Parser;

use crate::commands::CommandResult;
use crate::commands::module::ActionColor;
use crate::commands::module::Locator;
use crate::commands::module::TrustModeArg;
use crate::commands::module::discover;
use crate::commands::module::parse_manifest_value;
use crate::commands::module::print_action;
use crate::commands::module::print_relock_summary;
use crate::commands::module::read_manifest_value;
use crate::commands::module::remove_dependency;
use crate::commands::module::resolve_relock_for_manifest;
use crate::commands::module::signer_change_mode;
use crate::commands::module::trace_project;
use crate::commands::module::write_lockfile;
use crate::commands::module::write_manifest_value;
use crate::config::Config;

/// Arguments to `sprocket module remove`.
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

/// Runs `sprocket module remove`.
pub async fn remove(args: Args, config: Config, colorize: bool) -> CommandResult<()> {
    tracing::trace!(no_lock = args.no_lock, "starting `sprocket module remove`");
    let project = discover(&args.locator)?;
    trace_project("module remove", &project);
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
        dependency = args.name,
        manifest = %project.manifest_path.display(),
        "removed dependency from manifest"
    );

    if let Some(outcome) = relock {
        write_lockfile(&project, &outcome.lockfile)?;
        tracing::debug!(lockfile = %project.lockfile_path.display(), "wrote module lockfile");
        print_relock_summary(&outcome.stats, colorize);
        if outcome.stats.removed.is_empty() {
            print_action(
                "Removed",
                format!("`{}`", args.name),
                colorize,
                ActionColor::Green,
            );
        }
    } else {
        tracing::debug!("skipped relock after removing dependency");
        print_action(
            "Removed",
            format!("`{}`", args.name),
            colorize,
            ActionColor::Green,
        );
    }

    Ok(())
}
