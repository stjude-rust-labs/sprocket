//! `sprocket module upgrade`.

use std::collections::BTreeSet;

use anyhow::Context as _;
use clap::Parser;
use futures::future::try_join_all;
use wdl_modules::Lockfile;
use wdl_modules::Resolver as _;
use wdl_modules::dependency::DependencyName;
use wdl_modules::dependency::DependencySource;
use wdl_modules::dependency::GitSelector;
use wdl_modules::module::Module;
use wdl_modules::resolver::DependencyScope;
use wdl_modules::resolver::signer_identity_map;
use wdl_modules::resolver::update_relock;

use crate::commands::CommandResult;
use crate::commands::module::ActionColor;
use crate::commands::module::Locator;
use crate::commands::module::TrustModeArg;
use crate::commands::module::build_resolver;
use crate::commands::module::discover;
use crate::commands::module::enforce_lockfile_signer_policy;
use crate::commands::module::load_lockfile;
use crate::commands::module::parse_manifest_value;
use crate::commands::module::print_action;
use crate::commands::module::print_locking_summary;
use crate::commands::module::read_manifest_value;
use crate::commands::module::signer_change_mode;
use crate::commands::module::trace_project;
use crate::commands::module::write_lockfile;
use crate::commands::module::write_manifest_value;
use crate::config::Config;

/// Arguments to `sprocket module upgrade`.
#[derive(Parser, Debug)]
pub struct Args {
    /// Dependency aliases to upgrade. Empty upgrades all eligible dependencies.
    pub names: Vec<String>,

    /// Print manifest selector changes without writing files.
    #[arg(long)]
    pub dry_run: bool,

    /// Override signer trust behavior for this command.
    #[arg(long, value_enum)]
    pub trust_mode: Option<TrustModeArg>,

    /// Shared module locator.
    #[command(flatten)]
    pub locator: Locator,
}

/// Runs `sprocket module upgrade`.
pub async fn upgrade(args: Args, config: Config, colorize: bool) -> CommandResult<()> {
    tracing::trace!(
        dry_run = args.dry_run,
        requested = args.names.len(),
        "starting `sprocket module upgrade`"
    );
    let project = discover(&args.locator)?;
    trace_project("module upgrade", &project);

    let mut selected = Vec::new();
    if args.names.is_empty() {
        selected.extend(project.manifest.dependencies.keys().cloned());
    } else {
        for raw in &args.names {
            let name: DependencyName = raw
                .parse()
                .with_context(|| format!("invalid dependency name `{raw}`"))?;
            if !project.manifest.dependencies.contains_key(&name) {
                return Err(
                    anyhow::anyhow!("dependency `{raw}` not found in `module.json`").into(),
                );
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
        let source = project
            .manifest
            .dependencies
            .get(&name)
            .expect("selected dependency must exist");
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
        return Ok(());
    }
    tracing::debug!(
        eligible = eligible.len(),
        "checking latest dependency versions"
    );

    let resolver = build_resolver(
        &config,
        &project,
        load_lockfile(&project)?.unwrap_or_else(Lockfile::default),
    )?;

    let discovered = try_join_all(eligible.iter().map(|(name, source, old_req)| async {
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
        print_action(
            "Finished",
            "no upgrades available",
            colorize,
            ActionColor::Green,
        );
        return Ok(());
    }

    if args.dry_run {
        for (name, old_req, new_req) in &changed {
            print_action(
                "Upgrade",
                format!(
                    "{} {} -> {} (dry-run)",
                    name.manifest(),
                    version_display(old_req),
                    version_display(new_req)
                ),
                colorize,
                ActionColor::Yellow,
            );
        }
        tracing::debug!(
            changed = changed.len(),
            "dry run completed without writing manifest"
        );
        return Ok(());
    }

    let mut manifest_value = read_manifest_value(&project.manifest_path)?;
    for (name, _, new_req) in &changed {
        set_version_selector(&mut manifest_value, name.manifest(), new_req)?;
    }
    let pending_manifest = parse_manifest_value(&manifest_value)?;
    let existing = load_lockfile(&project)?.unwrap_or_default();
    let module = Module::new(std::sync::Arc::new(pending_manifest), project.root.clone());
    let resolver = build_resolver(&config, &project, existing)?;
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
    enforce_lockfile_signer_policy(
        &project,
        resolver.lockfile(),
        &outcome.lockfile,
        &identities,
        signer_change_mode(&config, args.trust_mode),
        colorize,
    )?;
    write_manifest_value(&project.manifest_path, &manifest_value)?;
    tracing::debug!(
        manifest = %project.manifest_path.display(),
        changed = changed.len(),
        "wrote upgraded version selectors"
    );
    print_action(
        "Upgrading",
        format!("{} packages to latest version", changed.len()),
        colorize,
        ActionColor::Green,
    );
    for (name, old_req, new_req) in &changed {
        print_action(
            "Upgraded",
            format!(
                "{} {} -> {}",
                name.manifest(),
                version_display(old_req),
                version_display(new_req)
            ),
            colorize,
            ActionColor::Green,
        );
    }
    print_action(
        "Finished",
        format!("upgrading {} packages", changed.len()),
        colorize,
        ActionColor::Green,
    );
    write_lockfile(&project, &outcome.lockfile)?;
    tracing::debug!(lockfile = %project.lockfile_path.display(), "wrote module lockfile");
    print_locking_summary(&outcome.stats, colorize);
    print_action(
        "Finished",
        format!("updating {} packages", outcome.stats.updated.len()),
        colorize,
        ActionColor::Green,
    );
    Ok(())
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

fn version_display(req: &str) -> String {
    let version = req
        .trim()
        .trim_start_matches(['^', '=', '~', '>', '<'])
        .trim_start_matches('=');
    format!("v{version}")
}
