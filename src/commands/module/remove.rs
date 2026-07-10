//! `sprocket module remove`.

use clap::Parser;

use crate::commands::CommandResult;
use crate::commands::module::ActionColor;
use crate::commands::module::Locator;
use crate::commands::module::discover;
use crate::commands::module::print_action;
use crate::commands::module::read_manifest_value;
use crate::commands::module::remove_dependency;
use crate::commands::module::trace_project;
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
    write_manifest_value(&project.manifest_path, &value)?;
    tracing::debug!(
        dependency = args.name,
        manifest = %project.manifest_path.display(),
        "removed dependency from manifest"
    );

    if args.no_lock {
        tracing::debug!("skipped relock after removing dependency");
        print_action(
            "Removed",
            format!("`{}`", args.name),
            colorize,
            ActionColor::Green,
        );
    } else {
        let project = discover(&args.locator)?;
        trace_project("module remove relock", &project);
        let stats = crate::commands::module::relock(&config, &project, colorize).await?;
        if stats.removed.is_empty() {
            print_action(
                "Removed",
                format!("`{}`", args.name),
                colorize,
                ActionColor::Green,
            );
        }
    }

    Ok(())
}
