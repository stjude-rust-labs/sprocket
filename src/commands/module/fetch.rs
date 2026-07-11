//! `sprocket dev module fetch`.

use clap::Parser;
use wdl_modules::module::Module;

use crate::commands::CommandResult;
use crate::commands::module::Locator;
use crate::commands::module::build_resolver;
use crate::commands::module::discover;
use crate::commands::module::require_lockfile;
use crate::commands::module::trace_project;
use crate::commands::printer::Printer;
use crate::config::Config;

/// Arguments to `sprocket dev module fetch`.
#[derive(Parser, Debug)]
pub struct Args {
    /// Shared module locator.
    #[command(flatten)]
    pub locator: Locator,
}

/// Runs `sprocket dev module fetch`.
pub async fn fetch(args: Args, config: Config, printer: Printer) -> CommandResult<()> {
    tracing::trace!("starting `sprocket dev module fetch`");
    let project = discover(&args.locator)?;
    trace_project("module fetch", &project);
    let lock = require_lockfile(&project)?;
    tracing::debug!(
        dependencies = lock.dependencies.len(),
        "loaded module lockfile for fetch"
    );
    let module = Module::new(project.manifest.clone(), project.root.clone());
    let resolver = build_resolver(&config, lock)?;
    let fetched = resolver
        .ensure_locked(&module)
        .await
        .map_err(anyhow::Error::from)?;
    tracing::debug!(fetched, "ensured locked dependencies are fetched");

    printer.status(
        "Fetched",
        format!(
            "{fetched} {}",
            if fetched == 1 {
                "dependency"
            } else {
                "dependencies"
            }
        ),
    );
    Ok(())
}
