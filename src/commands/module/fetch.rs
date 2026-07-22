//! `sprocket dev module fetch`.

use clap::Parser;
use wdl_modules::module::Module;

use super::project::Locator;
use super::project::discover;
use super::project::require_lockfile;
use super::project::trace_project;
use super::resolver::ResolverEnvironment;
use crate::commands::CommandResult;
use crate::commands::output::Action;
use crate::commands::output::CommandOutput;
use crate::config::Config;

const FETCH: Action = Action::new("Fetched", "fetch");

/// Arguments to `sprocket dev module fetch`.
#[derive(Parser, Debug)]
pub struct Args {
    /// Shared module locator.
    #[command(flatten)]
    locator: Locator,
}

/// Runs `sprocket dev module fetch`.
pub async fn fetch(args: Args, config: Config, output: CommandOutput) -> CommandResult<()> {
    tracing::trace!("starting `sprocket dev module fetch`");
    let project = discover(&args.locator)?;
    trace_project("module fetch", &project);
    let lock = require_lockfile(&project)?;
    tracing::debug!(
        dependencies = lock.dependencies.len(),
        "loaded module lockfile for fetch"
    );
    let module = Module::new(project.manifest.clone(), project.root.clone());
    let environment = ResolverEnvironment::from_config(&config)?;
    let resolver = environment.resolver(lock)?;
    let fetched = resolver
        .ensure_locked(&module)
        .await
        .map_err(anyhow::Error::from)?;
    tracing::debug!(fetched, "ensured locked dependencies are fetched");

    if fetched == 0 {
        output.current("module cache");
    } else {
        output.completed(
            FETCH,
            format!(
                "{fetched} {}",
                if fetched == 1 {
                    "dependency"
                } else {
                    "dependencies"
                }
            ),
        );
    }
    Ok(())
}
