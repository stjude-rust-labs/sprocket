//! `sprocket module update`.

use std::collections::BTreeSet;

use anyhow::Context as _;
use clap::Parser;
use wdl_modules::Resolver as _;
use wdl_modules::dependency::DependencyName;
use wdl_modules::module::Module;
use wdl_modules::resolver::signer_identity_map;
use wdl_modules::resolver::update_relock;

use crate::commands::CommandResult;
use crate::commands::module::Locator;
use crate::commands::module::TrustModeArg;
use crate::commands::module::build_resolver;
use crate::commands::module::discover;
use crate::commands::module::enforce_lockfile_signer_policy;
use crate::commands::module::load_lockfile;
use crate::commands::module::print_locking_summary;
use crate::commands::module::print_relock_summary;
use crate::commands::module::signer_change_mode;
use crate::commands::module::trace_project;
use crate::commands::module::write_lockfile;
use crate::commands::printer::Printer;
use crate::config::Config;

/// Arguments to `sprocket module update`.
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

/// Runs `sprocket module update`.
pub async fn update(args: Args, config: Config, printer: Printer) -> CommandResult<()> {
    tracing::trace!(
        dry_run = args.dry_run,
        requested = args.names.len(),
        "starting `sprocket module update`"
    );
    let project = discover(&args.locator)?;
    trace_project("module update", &project);
    let on_disk = load_lockfile(&project)?.unwrap_or_default();

    let mut names = BTreeSet::new();
    for raw in &args.names {
        let name: DependencyName = raw
            .parse()
            .with_context(|| format!("invalid dependency name `{raw}`"))?;
        if !project.manifest.dependencies.contains_key(&name) {
            return Err(anyhow::anyhow!("dependency `{}` not found in `module.json`", raw).into());
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
    let resolver = build_resolver(&config, on_disk)?;
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

    if args.dry_run {
        tracing::debug!("dry run completed without writing lockfile");
        print_relock_summary(&outcome.stats, printer);
        return Ok(());
    }

    enforce_lockfile_signer_policy(
        resolver.lockfile(),
        &outcome.lockfile,
        &identities,
        signer_change_mode(&config, args.trust_mode),
        printer,
    )?;
    write_lockfile(&project, &outcome.lockfile)?;
    tracing::debug!(lockfile = %project.lockfile_path.display(), "wrote module lockfile");
    print_locking_summary(&outcome.stats, printer);
    printer.status(
        "Finished",
        format!("updating {} packages", outcome.stats.updated.len()),
    );
    Ok(())
}
