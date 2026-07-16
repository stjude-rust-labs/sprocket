//! The `sprocket dev module` command group.

use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context as _;
use clap::Args as ClapArgs;
use clap::Subcommand;
use clap::ValueEnum;
use wdl_modules::Lockfile;
use wdl_modules::Manifest;
use wdl_modules::Resolver as _;
use wdl_modules::dependency::DependencySource;
use wdl_modules::module::Module;
use wdl_modules::resolver::GitResolver;
use wdl_modules::resolver::RelockOutcome;
use wdl_modules::resolver::RelockStats;
use wdl_modules::resolver::ResolverPolicy;
use wdl_modules::resolver::SignerIdentityMap;
use wdl_modules::resolver::TrustMode;
use wdl_modules::resolver::partial_relock;
use wdl_modules::resolver::signer_identity_map;

use crate::commands::printer::Printer;
use crate::config::Config;

pub mod add;
pub mod clean;
pub mod fetch;
pub mod init;
pub mod lock;
pub mod remove;
pub mod sign;
pub mod tree;
pub mod trust;
mod trust_policy;
pub mod update;
pub mod upgrade;
pub mod verify;

pub(crate) use trust_policy::SignerChangeMode;
pub(crate) use trust_policy::accept_lockfile_signers;
pub(crate) use trust_policy::enforce_signer_trust;
pub(crate) use trust_policy::load_trust_store;
pub(crate) use trust_policy::render_signer;
pub(crate) use trust_policy::save_trust_store;

/// Parsed module project context shared by porcelain subcommands.
#[derive(Debug, Clone)]
pub struct Project {
    /// Path to the discovered `module.json`.
    pub manifest_path: PathBuf,
    /// Root directory containing `module.json`.
    pub root: PathBuf,
    /// Parsed manifest discovered from disk.
    pub manifest: Arc<Manifest>,
    /// Path to the sibling `module-lock.json`.
    pub lockfile_path: PathBuf,
}

/// Locates the governing `module.json`.
#[derive(ClapArgs, Debug, Clone)]
pub struct Locator {
    /// Path to the `module.json` (or its directory). Defaults to an
    /// upward search from the current directory.
    #[arg(long, value_name = "PATH", global = true)]
    pub manifest_path: Option<PathBuf>,
}

/// Subcommands of `sprocket dev module`.
#[derive(Subcommand, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ModuleCommands {
    /// Initialize a new module in the current or given directory.
    Init(init::Args),
    /// Add a dependency to `module.json` and relock.
    Add(add::Args),
    /// Remove a dependency from `module.json` and relock.
    Remove(remove::Args),
    /// Resolve dependencies and write `module-lock.json`.
    Lock(lock::Args),
    /// Update locked dependencies within manifest constraints.
    Update(update::Args),
    /// Raise manifest constraints to the latest versions, then relock.
    Upgrade(upgrade::Args),
    /// Print the resolved dependency tree.
    Tree(tree::TreeArgs),
    /// List dependencies in a flat table.
    List(tree::ListArgs),
    /// Verify module signatures and locked dependencies.
    Verify(verify::Args),
    /// Pre-populate the cache from the lockfile.
    Fetch(fetch::Args),
    /// Manage the module cache.
    #[command(subcommand)]
    Cache(clean::CacheCommands),
    /// Sign the module, writing `module.sig`.
    Sign(sign::Args),
    /// Manage the user trust store.
    #[command(subcommand)]
    Trust(trust::TrustCommands),
}

/// Discovers the governing project manifest based on the locator.
pub fn discover(locator: &Locator) -> anyhow::Result<Project> {
    let start = match locator.manifest_path.as_deref() {
        Some(path) if path.is_file() => path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf(),
        Some(path) if path.is_dir() => path.to_path_buf(),
        Some(path) => anyhow::bail!("manifest path `{}` does not exist", path.display()),
        None => std::env::current_dir().context("reading current directory")?,
    };

    let (manifest_path, manifest) = crate::analysis::discover_manifest_upward(&start)?
        .with_context(|| "no `module.json` found; run `sprocket dev module init` first")?;

    let root = manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let lockfile_path = manifest_path.with_file_name(wdl_modules::LOCKFILE_FILENAME);

    Ok(Project {
        manifest_path,
        root,
        manifest: Arc::new(manifest),
        lockfile_path,
    })
}

/// Dispatches a `sprocket dev module` subcommand.
pub async fn run(
    command: ModuleCommands,
    config: Config,
    printer: Printer,
) -> crate::commands::CommandResult<()> {
    match command {
        ModuleCommands::Init(args) => init::init(args, printer).await,
        ModuleCommands::Add(args) => add::add(args, config, printer).await,
        ModuleCommands::Remove(args) => remove::remove(args, config, printer).await,
        ModuleCommands::Lock(args) => lock::lock(args, config, printer).await,
        ModuleCommands::Update(args) => update::update(args, config, printer).await,
        ModuleCommands::Upgrade(args) => upgrade::upgrade(args, config, printer).await,
        ModuleCommands::Tree(args) => tree::tree(args).await,
        ModuleCommands::List(args) => tree::list(args).await,
        ModuleCommands::Verify(args) => verify::verify(args, config, printer).await,
        ModuleCommands::Fetch(args) => fetch::fetch(args, config, printer).await,
        ModuleCommands::Cache(args) => clean::cache(args, config, printer).await,
        ModuleCommands::Sign(args) => sign::sign(args, printer).await,
        ModuleCommands::Trust(args) => trust::trust(args, printer).await,
    }
}

/// Traces the discovered module project for a command.
pub(crate) fn trace_project(command: &'static str, project: &Project) {
    tracing::debug!(
        command,
        module = %project.manifest.name,
        root = %project.root.display(),
        manifest = %project.manifest_path.display(),
        lockfile = %project.lockfile_path.display(),
        dependencies = project.manifest.dependencies.len(),
        "discovered module project"
    );
}

/// A resolved lockfile update plus signer metadata gathered while
/// verifying the dependency tree.
pub(crate) struct RelockPlan {
    /// The lockfile currently on disk, or an empty lockfile when absent.
    pub existing: Lockfile,
    /// The relock result that should be written after policy passes.
    pub outcome: RelockOutcome,
    /// Signer identity metadata from freshly verified `module.sig` files.
    pub identities: SignerIdentityMap,
}

/// Command-line override for module signer trust mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "lower")]
pub enum TrustModeArg {
    /// Prompt before trusting signer keys.
    Confirm,
    /// Trust first-seen signer keys automatically, then prompt on changes.
    Tofu,
    /// Trust signer keys automatically.
    Auto,
}

impl From<TrustModeArg> for TrustMode {
    fn from(value: TrustModeArg) -> Self {
        match value {
            TrustModeArg::Confirm => TrustMode::Confirm,
            TrustModeArg::Tofu => TrustMode::Tofu,
            TrustModeArg::Auto => TrustMode::Auto,
        }
    }
}

/// Resolves the signer trust mode using CLI override first, then config.
pub(crate) fn signer_change_mode(
    config: &Config,
    trust_mode: Option<TrustModeArg>,
) -> SignerChangeMode {
    SignerChangeMode::from_trust_mode(
        trust_mode
            .map(TrustMode::from)
            .unwrap_or(config.modules.trust_mode),
    )
}

/// Loads `module-lock.json` when present.
pub fn load_lockfile(project: &Project) -> anyhow::Result<Option<Lockfile>> {
    if !project.lockfile_path.exists() {
        tracing::trace!(lockfile = %project.lockfile_path.display(), "module lockfile is absent");
        return Ok(None);
    }

    tracing::trace!(lockfile = %project.lockfile_path.display(), "reading module lockfile");
    let bytes = std::fs::read(&project.lockfile_path)
        .with_context(|| format!("reading `{}`", project.lockfile_path.display()))?;
    let lock = Lockfile::parse(&bytes)
        .with_context(|| format!("parsing `{}`", project.lockfile_path.display()))?;
    tracing::debug!(
        lockfile = %project.lockfile_path.display(),
        dependencies = lock.dependencies.len(),
        "loaded module lockfile"
    );
    Ok(Some(lock))
}

/// Loads `module-lock.json`, failing when it is absent.
pub(crate) fn require_lockfile(project: &Project) -> anyhow::Result<Lockfile> {
    load_lockfile(project)?
        .ok_or_else(|| anyhow::anyhow!("no `module-lock.json`; run `sprocket dev module lock`"))
}

/// Builds a Git resolver configured for module porcelain commands.
pub fn build_resolver(config: &Config, lockfile: Lockfile) -> anyhow::Result<GitResolver> {
    let configured_cache = config.modules.cache_path.is_some();
    let cache_root = config
        .modules
        .cache_path
        .clone()
        .unwrap_or_else(crate::analysis::default_cache_root);

    let trust_path = crate::analysis::default_trust_path();
    tracing::info!(
        cache = %cache_root.display(),
        configured = configured_cache,
        "using module cache"
    );
    tracing::info!(
        trust_store = %trust_path.display(),
        "using module trust store"
    );
    let trust = load_trust_store(&trust_path)?;

    let policy = ResolverPolicy::try_from(&config.modules)?;
    tracing::debug!(
        cache = %cache_root.display(),
        trust_store = %trust_path.display(),
        trusted = trust.keys.len(),
        locked = lockfile.dependencies.len(),
        "built module resolver"
    );

    let resolver = GitResolver::builder()
        .cache_root(cache_root)
        .trust(trust)
        .lockfile(lockfile)
        .policy(policy)
        .build();
    resolver.initialize_cache()?;
    Ok(resolver)
}

/// Aligns a temporary file's permissions with its destination before an
/// atomic rename.
///
/// `NamedTempFile` creates files with owner-only permissions on Unix;
/// without this, atomic rewrites would silently narrow the destination's
/// mode. Existing destination permissions are preserved; new files get
/// the conventional read-for-all mode.
fn align_temp_permissions(temp: &tempfile::NamedTempFile, path: &Path) -> anyhow::Result<()> {
    if let Ok(metadata) = std::fs::metadata(path) {
        temp.as_file()
            .set_permissions(metadata.permissions())
            .with_context(|| format!("setting permissions on `{}`", temp.path().display()))?;
        return Ok(());
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        temp.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o644))
            .with_context(|| format!("setting permissions on `{}`", temp.path().display()))?;
    }

    Ok(())
}

/// Writes `module-lock.json` atomically.
pub fn write_lockfile(project: &Project, lock: &Lockfile) -> anyhow::Result<()> {
    let dir = project
        .lockfile_path
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(dir)
        .with_context(|| format!("creating a temporary file in `{}`", dir.display()))?;
    lock.write(&mut temp)
        .with_context(|| format!("writing `{}`", temp.path().display()))?;
    align_temp_permissions(&temp, &project.lockfile_path)?;
    temp.persist(&project.lockfile_path)
        .with_context(|| format!("replacing `{}`", project.lockfile_path.display()))?;
    Ok(())
}

/// Reads `module.json` as JSON while validating it with strict manifest
/// parsing.
pub fn read_manifest_value(path: &Path) -> anyhow::Result<serde_json::Value> {
    let bytes = std::fs::read(path).with_context(|| format!("reading `{}`", path.display()))?;
    Manifest::parse(&bytes).with_context(|| format!("parsing `{}`", path.display()))?;
    let value = serde_json::from_slice(&bytes)
        .with_context(|| format!("parsing `{}` as JSON", path.display()))?;
    Ok(value)
}

/// Writes `module.json` atomically after validating parser-accepted shape.
pub fn write_manifest_value(path: &Path, value: &serde_json::Value) -> anyhow::Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Manifest::parse(&bytes).with_context(|| format!("parsing `{}`", path.display()))?;

    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(dir)
        .with_context(|| format!("creating a temporary file in `{}`", dir.display()))?;
    std::io::Write::write_all(&mut temp, &bytes)
        .with_context(|| format!("writing `{}`", temp.path().display()))?;
    align_temp_permissions(&temp, path)?;
    temp.persist(path)
        .with_context(|| format!("replacing `{}`", path.display()))?;
    Ok(())
}

/// Parses an edited manifest JSON value with strict manifest validation.
pub(crate) fn parse_manifest_value(value: &serde_json::Value) -> anyhow::Result<Manifest> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Manifest::parse(&bytes).context("parsing edited `module.json`")
}

/// Inserts or replaces a dependency source in the manifest JSON.
pub fn set_dependency(
    value: &mut serde_json::Value,
    name: &str,
    source: &DependencySource,
) -> anyhow::Result<()> {
    let root = value
        .as_object_mut()
        .with_context(|| "`module.json` root must be an object")?;

    if !root.contains_key("dependencies") {
        root.insert(
            "dependencies".to_string(),
            serde_json::Value::Object(serde_json::Map::new()),
        );
    }

    let dependencies = root
        .get_mut("dependencies")
        .and_then(serde_json::Value::as_object_mut)
        .with_context(|| "`dependencies` in `module.json` must be an object")?;

    dependencies.insert(name.to_string(), serde_json::to_value(source)?);
    dependencies.sort_keys();

    Ok(())
}

/// Removes a dependency from the manifest JSON.
pub fn remove_dependency(value: &mut serde_json::Value, name: &str) -> anyhow::Result<bool> {
    let root = value
        .as_object_mut()
        .with_context(|| "`module.json` root must be an object")?;

    let Some(dependencies_value) = root.get_mut("dependencies") else {
        return Ok(false);
    };

    let dependencies = dependencies_value
        .as_object_mut()
        .with_context(|| "`dependencies` in `module.json` must be an object")?;
    Ok(dependencies.remove(name).is_some())
}

/// Re-resolves dependencies and merges with the previous lockfile.
pub(crate) async fn resolve_relock(
    config: &Config,
    project: &Project,
) -> anyhow::Result<RelockOutcome> {
    resolve_relock_with_signer_mode(
        config,
        project,
        SignerChangeMode::Strict,
        Printer::new(false),
    )
    .await
}

/// Re-resolves dependencies and merges with the previous lockfile.
pub(crate) async fn resolve_relock_with_signer_mode(
    config: &Config,
    project: &Project,
    signer_mode: SignerChangeMode,
    printer: Printer,
) -> anyhow::Result<RelockOutcome> {
    tracing::trace!(
        manifest = %project.manifest_path.display(),
        "resolving module dependency lockfile"
    );
    resolve_relock_for_manifest(
        config,
        project,
        project.manifest.clone(),
        signer_mode,
        printer,
    )
    .await
}

/// Re-resolves dependencies for a specific manifest and merges with the
/// previous lockfile.
pub(crate) async fn resolve_relock_for_manifest(
    config: &Config,
    project: &Project,
    manifest: Arc<Manifest>,
    signer_mode: SignerChangeMode,
    printer: Printer,
) -> anyhow::Result<RelockOutcome> {
    let plan = resolve_relock_plan(config, project, manifest).await?;
    let trust_path = crate::analysis::default_trust_path();
    enforce_signer_trust(
        &trust_path,
        &plan.existing,
        &plan.outcome.lockfile,
        &plan.identities,
        signer_mode,
        printer,
    )?;
    Ok(plan.outcome)
}

/// Re-resolves dependencies and returns the proposed lockfile without
/// applying signer-change policy.
pub(crate) async fn resolve_relock_plan(
    config: &Config,
    project: &Project,
    manifest: Arc<Manifest>,
) -> anyhow::Result<RelockPlan> {
    let module = Module::new(manifest, project.root.clone());
    let existing = load_lockfile(project)?.unwrap_or_default();
    tracing::debug!(
        existing = existing.dependencies.len(),
        declared = module.manifest.dependencies.len(),
        "loaded relock inputs"
    );
    let resolver = build_resolver(config, existing.clone())?;
    let tree = resolver.resolve_tree(&module).await?;
    tracing::debug!(
        resolved = tree.dependencies.len(),
        "resolved module dependency tree"
    );
    let outcome = partial_relock(&module.manifest, &existing, &tree)?;
    let identities = signer_identity_map(&tree);

    Ok(RelockPlan {
        existing,
        outcome,
        identities,
    })
}

/// Enforces signer-change policy for an existing and refreshed lockfile.
pub(crate) fn enforce_lockfile_signer_policy(
    existing: &Lockfile,
    new: &Lockfile,
    identities: &SignerIdentityMap,
    mode: SignerChangeMode,
    printer: Printer,
) -> anyhow::Result<()> {
    let trust_path = crate::analysis::default_trust_path();
    enforce_signer_trust(&trust_path, existing, new, identities, mode, printer)
}

/// Regenerates `module-lock.json` before execution when it is missing or
/// out of date with the governing `module.json`.
///
/// The module specification requires the engine to regenerate the
/// lockfile (or refuse to run) before executing a workflow whose module
/// is out of date. This resolves the dependency tree and rewrites the
/// lockfile so the run proceeds against a consistent, reproducible tree.
/// It is a no-op when the sources are not governed by a `module.json` or
/// when the module declares no dependencies.
pub(crate) async fn ensure_lockfile_current(config: &Config, start: &Path) -> anyhow::Result<()> {
    let Some((manifest_path, manifest)) = crate::analysis::discover_manifest_upward(start)? else {
        return Ok(());
    };
    if manifest.dependencies.is_empty() {
        return Ok(());
    }

    let root = manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let lockfile_path = manifest_path.with_file_name(wdl_modules::LOCKFILE_FILENAME);
    let project = Project {
        manifest_path,
        root,
        manifest: Arc::new(manifest),
        lockfile_path,
    };

    let existing = load_lockfile(&project)?;
    if existing
        .as_ref()
        .is_some_and(|lock| lock.satisfies_manifest(&project.manifest))
    {
        return Ok(());
    }

    tracing::info!(
        manifest = %project.manifest_path.display(),
        lockfile_present = existing.is_some(),
        "`module-lock.json` is missing or out of date; regenerating before execution"
    );
    let outcome = resolve_relock(config, &project).await?;
    write_lockfile(&project, &outcome.lockfile)?;
    Ok(())
}

/// Prints a relock change summary in cargo-style action lines.
pub fn print_relock_summary(stats: &RelockStats, printer: Printer) {
    print_relock_summary_with(stats, "Locked", printer);
}

/// Prints a relock summary, using `added_verb` for newly added dependencies.
///
/// Callers that add a dependency (such as `sprocket dev module add`) pass
/// `"Added"` so the top-level action reads naturally; the shared default is
/// `"Locked"`.
pub fn print_relock_summary_with(stats: &RelockStats, added_verb: &str, printer: Printer) {
    tracing::debug!(
        kept = stats.kept,
        added = stats.added.len(),
        removed = stats.removed.len(),
        skipped = stats.skipped.len(),
        updated = stats.updated.len(),
        "computed relock summary"
    );
    if stats.added.is_empty()
        && stats.removed.is_empty()
        && stats.skipped.is_empty()
        && stats.updated.is_empty()
    {
        printer.status("Locked", "(up to date)");
        return;
    }

    for change in &stats.added {
        let details = change_details(
            change.path.as_deref(),
            change.selector.as_deref(),
            change.commit.as_deref(),
        );
        printer.status(added_verb, format!("`{}`{details}", change.name));
    }

    for change in &stats.removed {
        let details = change_details(
            change.path.as_deref(),
            change.selector.as_deref(),
            change.commit.as_deref(),
        );
        printer.status("Removed", format!("`{}`{details}", change.name));
    }

    for change in &stats.skipped {
        let details = skipped_details(
            change.path.as_deref(),
            change.selector.as_deref(),
            change.commit.as_deref(),
        );
        printer.change("Skipped", format!("`{}`{details}", change.name));
    }

    for change in &stats.updated {
        let details = update_details(
            change.from_path.as_deref(),
            change.to_path.as_deref(),
            change.from_selector.as_deref(),
            change.to_selector.as_deref(),
            change.from_commit.as_deref(),
            change.to_commit.as_deref(),
        );

        printer.change("Updated", format!("`{}`{details}", change.name));
    }
}

/// Prints update and upgrade lockfile output as lock-centric status lines.
pub fn print_locking_summary(stats: &RelockStats, printer: Printer) {
    printer.status(
        "Locking",
        format!("{} packages based on `module.json`", stats.updated.len()),
    );

    for change in &stats.updated {
        let details = update_details(
            change.from_path.as_deref(),
            change.to_path.as_deref(),
            change.from_selector.as_deref(),
            change.to_selector.as_deref(),
            change.from_commit.as_deref(),
            change.to_commit.as_deref(),
        );
        printer.status("Updated", format!("{}{}", change.name.manifest(), details));
    }
}

/// Renders skipped source metadata for update output.
fn skipped_details(path: Option<&str>, selector: Option<&str>, commit: Option<&str>) -> String {
    let mut parts = vec!["latest".to_string()];
    parts.extend(change_detail_parts(path, selector, commit));
    format!(" ({})", parts.join(", "))
}

/// Renders added or removed source metadata for summary output.
fn change_details(path: Option<&str>, selector: Option<&str>, commit: Option<&str>) -> String {
    let parts = change_detail_parts(path, selector, commit);
    if parts.is_empty() {
        return String::new();
    }
    format!(" ({})", parts.join(", "))
}

/// Collects the source metadata fragments shared by the summary renderers.
fn change_detail_parts(
    path: Option<&str>,
    selector: Option<&str>,
    commit: Option<&str>,
) -> Vec<String> {
    let mut details = Vec::new();
    if let Some(selector) = selector {
        details.push(format!("selector: {}", selector_detail(selector)));
    }
    if let Some(path) = path {
        details.push(format!("path: `{path}`"));
    }
    if let Some(commit) = commit {
        details.push(format!("commit: `{}`", short_commit(commit)));
    }
    details
}

#[cfg(test)]
fn update_message(
    name: &impl std::fmt::Display,
    from_path: Option<&str>,
    to_path: Option<&str>,
    from_selector: Option<&str>,
    to_selector: Option<&str>,
    from_commit: Option<&str>,
    to_commit: Option<&str>,
) -> String {
    format!(
        "Updated `{name}`{}",
        update_details(
            from_path,
            to_path,
            from_selector,
            to_selector,
            from_commit,
            to_commit
        )
    )
}

fn update_details(
    from_path: Option<&str>,
    to_path: Option<&str>,
    from_selector: Option<&str>,
    to_selector: Option<&str>,
    from_commit: Option<&str>,
    to_commit: Option<&str>,
) -> String {
    let mut details = Vec::new();

    match (from_selector, to_selector) {
        (Some(from), Some(to)) if from == to => {
            details.push(format!("selector: {}", selector_detail(from)));
        }
        (Some(from), Some(to)) => {
            details.push(format!(
                "selector: {} -> {}",
                selector_detail(from),
                selector_detail(to)
            ));
        }
        _ => {}
    }

    match (from_path, to_path) {
        (None, None) => {}
        (from, to) if from == to => {
            details.push(format!("path: `{}`", from.unwrap_or("/")));
        }
        (from, to) => {
            details.push(format!(
                "path: `{}` -> `{}`",
                from.unwrap_or("/"),
                to.unwrap_or("/")
            ));
        }
    }

    if let (Some(from_commit), Some(to_commit)) = (from_commit, to_commit)
        && from_commit != to_commit
    {
        details.push(format!(
            "commit: `{}` -> `{}`",
            short_commit(from_commit),
            short_commit(to_commit)
        ));
    }

    if !details.is_empty() {
        return format!(" ({})", details.join(", "));
    }

    String::new()
}

fn selector_detail(selector: &str) -> String {
    selector.split_once(' ').map_or_else(
        || format!("`{selector}`"),
        |(kind, value)| format!("{kind} `{value}`"),
    )
}

fn short_commit(commit: &str) -> &str {
    &commit[..7.min(commit.len())]
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_MANIFEST_JSON: &str = r#"{
      "name": "example",
      "license": "MIT",
      "entrypoint": "main.wdl",
      "x-extra": { "enabled": true, "note": "preserve me" },
      "dependencies": {
        "zeta": { "path": "./zeta" },
        "alpha": { "path": "./alpha", "x-source-extra": 7 }
      }
    }"#;

    #[test]
    fn set_dependency_inserts_preserves_extra_and_sorts_dependencies() {
        let mut value: serde_json::Value = serde_json::from_str(VALID_MANIFEST_JSON).unwrap();
        let source: DependencySource = serde_json::from_str(
            r#"{
              "path": "./beta",
              "x-source-extra": "kept"
            }"#,
        )
        .unwrap();

        set_dependency(&mut value, "beta", &source).unwrap();

        assert_eq!(value["name"], "example");
        assert_eq!(value["x-extra"]["note"], "preserve me");
        assert_eq!(value["dependencies"]["alpha"]["x-source-extra"], 7);
        assert_eq!(value["dependencies"]["beta"]["x-source-extra"], "kept");

        let dependency_keys = value["dependencies"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(dependency_keys, vec!["alpha", "beta", "zeta"]);
    }

    #[test]
    fn set_dependency_errors_when_dependencies_is_non_object() {
        let mut value: serde_json::Value = serde_json::from_str(VALID_MANIFEST_JSON).unwrap();
        value["dependencies"] = serde_json::Value::String("not-an-object".to_string());
        let source: DependencySource = serde_json::from_str(r#"{ "path": "./beta" }"#).unwrap();

        let err = set_dependency(&mut value, "beta", &source).unwrap_err();
        assert!(
            err.to_string()
                .contains("`dependencies` in `module.json` must be an object")
        );
    }

    #[test]
    fn remove_dependency_returns_false_when_dependency_absent() {
        let mut value: serde_json::Value = serde_json::from_str(VALID_MANIFEST_JSON).unwrap();
        assert!(!remove_dependency(&mut value, "missing").unwrap());
    }

    #[tokio::test]
    async fn ensure_lockfile_current_regenerates_missing_lockfile() {
        let work = tempfile::tempdir().unwrap();
        // A local path dependency module.
        let dep_dir = work.path().join("dep");
        std::fs::create_dir_all(&dep_dir).unwrap();
        std::fs::write(
            dep_dir.join("module.json"),
            br#"{"name":"dep","license":"MIT"}"#,
        )
        .unwrap();
        std::fs::write(dep_dir.join("index.wdl"), b"version 1.3\n").unwrap();

        // A consumer that depends on it, with no lockfile yet.
        let consumer_dir = work.path().join("consumer");
        std::fs::create_dir_all(&consumer_dir).unwrap();
        std::fs::write(
            consumer_dir.join("module.json"),
            br#"{"name":"consumer","license":"MIT","dependencies":{"dep":{"path":"../dep"}}}"#,
        )
        .unwrap();

        let lockfile_path = consumer_dir.join(wdl_modules::LOCKFILE_FILENAME);
        assert!(!lockfile_path.exists());

        ensure_lockfile_current(&Config::default(), &consumer_dir)
            .await
            .expect("regeneration should succeed for a local path dependency");

        assert!(lockfile_path.exists(), "lockfile should be created");
        let bytes = std::fs::read(&lockfile_path).unwrap();
        let lock = Lockfile::parse(&bytes).unwrap();
        let consumer_manifest =
            Manifest::parse(&std::fs::read(consumer_dir.join("module.json")).unwrap()).unwrap();
        assert!(lock.satisfies_manifest(&consumer_manifest));
    }

    #[tokio::test]
    async fn ensure_lockfile_current_is_noop_without_dependencies() {
        let work = tempfile::tempdir().unwrap();
        std::fs::write(
            work.path().join("module.json"),
            br#"{"name":"solo","license":"MIT"}"#,
        )
        .unwrap();

        ensure_lockfile_current(&Config::default(), work.path())
            .await
            .expect("no dependencies means nothing to lock");
        assert!(
            !work.path().join(wdl_modules::LOCKFILE_FILENAME).exists(),
            "a dependency-free module needs no lockfile"
        );
    }

    #[test]
    fn write_manifest_value_round_trips_through_manifest_parse() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("module.json");
        let value: serde_json::Value = serde_json::from_str(VALID_MANIFEST_JSON).unwrap();

        write_manifest_value(&path, &value).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.ends_with(b"\n"));
        Manifest::parse(&bytes).unwrap();

        let written: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(written["x-extra"]["enabled"], true);
        assert_eq!(written["dependencies"]["alpha"]["x-source-extra"], 7);
    }

    /// Reads the permission bits of `path`.
    #[cfg(unix)]
    fn mode_of(path: &std::path::Path) -> u32 {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    #[cfg(unix)]
    fn write_manifest_value_gives_new_files_conventional_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("module.json");
        let value: serde_json::Value = serde_json::from_str(VALID_MANIFEST_JSON).unwrap();

        write_manifest_value(&path, &value).unwrap();

        assert_eq!(mode_of(&path), 0o644);
    }

    #[test]
    #[cfg(unix)]
    fn write_manifest_value_preserves_existing_permissions() {
        use std::os::unix::fs::PermissionsExt as _;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("module.json");
        let value: serde_json::Value = serde_json::from_str(VALID_MANIFEST_JSON).unwrap();
        write_manifest_value(&path, &value).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

        write_manifest_value(&path, &value).unwrap();

        assert_eq!(mode_of(&path), 0o600);
    }

    #[test]
    fn update_message_describes_path_changes_clearly() {
        assert_eq!(
            update_message(
                &"ww-bwa",
                Some("modules/ww-bwa"),
                Some("modules/ww-gatk"),
                Some("branch main"),
                Some("branch main"),
                None,
                None
            ),
            "Updated `ww-bwa` (selector: branch `main`, path: `modules/ww-bwa` -> \
             `modules/ww-gatk`)"
        );
        assert_eq!(
            update_message(
                &"ww-bwa",
                None,
                None,
                Some("version ^1"),
                Some("version ^2"),
                None,
                None
            ),
            "Updated `ww-bwa` (selector: version `^1` -> version `^2`)"
        );
        assert_eq!(
            update_message(
                &"ww-bwa",
                Some("modules/ww-bwa"),
                Some("modules/ww-bwa"),
                Some("version ^1"),
                Some("version ^2"),
                None,
                None
            ),
            "Updated `ww-bwa` (selector: version `^1` -> version `^2`, path: `modules/ww-bwa`)"
        );
        assert_eq!(
            update_message(
                &"ww-bwa",
                Some("modules/ww-bwa"),
                Some("modules/ww-bwa"),
                Some("branch main"),
                Some("branch main"),
                Some("a5805f5f2a1cbe64d28365424870d585f883bd0f"),
                Some("8797145982ba1b1b7adb5ea716c03a7e4e9dd412")
            ),
            "Updated `ww-bwa` (selector: branch `main`, path: `modules/ww-bwa`, commit: `a5805f5` \
             -> `8797145`)"
        );
    }

    #[test]
    fn change_details_describes_available_source_metadata() {
        assert_eq!(
            change_details(
                Some("modules/ww-bwa"),
                Some("branch main"),
                Some("8797145982ba1b1b7adb5ea716c03a7e4e9dd412")
            ),
            " (selector: branch `main`, path: `modules/ww-bwa`, commit: `8797145`)"
        );
        assert_eq!(
            change_details(None, Some("version ^1"), None),
            " (selector: version `^1`)"
        );
        assert_eq!(change_details(None, None, None), "");
    }

    #[test]
    fn skipped_details_marks_dependency_as_latest() {
        assert_eq!(
            skipped_details(
                Some("modules/ww-bwa"),
                Some("branch main"),
                Some("8797145982ba1b1b7adb5ea716c03a7e4e9dd412")
            ),
            " (latest, selector: branch `main`, path: `modules/ww-bwa`, commit: `8797145`)"
        );
        assert_eq!(skipped_details(None, None, None), " (latest)");
    }
}
