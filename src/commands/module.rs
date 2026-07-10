//! The `sprocket module` command group.

use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context as _;
use clap::Args as ClapArgs;
use clap::Subcommand;
use clap::ValueEnum;
use colored::Colorize as _;
use wdl_modules::Lockfile;
use wdl_modules::Manifest;
use wdl_modules::Resolver as _;
use wdl_modules::dependency::DependencySource;
use wdl_modules::lockfile::DependencyMap;
use wdl_modules::module::Module;
use wdl_modules::resolver::ChangedSigner;
use wdl_modules::resolver::GitResolver;
use wdl_modules::resolver::LockfileDiff;
use wdl_modules::resolver::NewSigner;
use wdl_modules::resolver::RelockOutcome;
use wdl_modules::resolver::RelockStats;
use wdl_modules::resolver::RemovedSigner;
use wdl_modules::resolver::ResolverPolicy;
use wdl_modules::resolver::SignerIdentityMap;
use wdl_modules::resolver::TrustMode;
use wdl_modules::resolver::TrustStore;
use wdl_modules::resolver::partial_relock;
use wdl_modules::resolver::signer_identity_map;
use wdl_modules::signing::SignerIdentity;
use wdl_modules::signing::VerifyingKey;

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
pub mod update;
pub mod upgrade;
pub mod verify;

/// Color used for the leading action verb in module command output.
pub(crate) enum ActionColor {
    /// Successful or constructive action.
    Green,
    /// Update or dry-run change action.
    Yellow,
    /// Informational action.
    Cyan,
    /// Failed action.
    Red,
}

impl ActionColor {
    /// Applies this color to an action verb.
    fn apply(self, verb: &str) -> String {
        match self {
            Self::Green => verb.green().bold().to_string(),
            Self::Yellow => verb.yellow().bold().to_string(),
            Self::Cyan => verb.cyan().bold().to_string(),
            Self::Red => verb.red().bold().to_string(),
        }
    }
}

/// Prints a module command action line with only the verb colored.
pub(crate) fn print_action(
    verb: &str,
    rest: impl std::fmt::Display,
    colorize: bool,
    color: ActionColor,
) {
    if colorize {
        println!("{} {rest}", color.apply(verb));
    } else {
        println!("{verb} {rest}");
    }
}

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

/// Subcommands of `sprocket module`.
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
        .with_context(|| "no `module.json` found; run `sprocket module init` first")?;

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

/// How signer changes should be handled while writing a refreshed lockfile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SignerChangeMode {
    /// Refuse signer keys unless already accepted through the trust store.
    Strict,
    /// Prompt to trust new or changed signer keys. The default answer is no.
    Confirm,
    /// Trust new signer keys without prompting but prompt on key changes.
    Tofu,
    /// Trust new or changed signer keys without prompting.
    Auto,
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

impl SignerChangeMode {
    /// Selects the interactive lock-writing mode for module commands.
    pub(crate) fn from_trust_mode(trust_mode: TrustMode) -> Self {
        match trust_mode {
            TrustMode::Confirm => Self::Confirm,
            TrustMode::Tofu => Self::Tofu,
            TrustMode::Auto => Self::Auto,
        }
    }
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

/// Builds a Git resolver configured for module porcelain commands.
pub fn build_resolver(
    config: &Config,
    _project: &Project,
    lockfile: Lockfile,
) -> anyhow::Result<GitResolver> {
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
    tracing::trace!(trust_store = %trust_path.display(), "loading module trust store");
    let trust = TrustStore::load_or_default(&trust_path)
        .with_context(|| format!("loading trust store at `{}`", trust_path.display()))?;

    let policy = ResolverPolicy::try_from(&config.modules)?;
    tracing::debug!(
        cache = %cache_root.display(),
        trust_store = %trust_path.display(),
        trusted = trust.keys.len(),
        locked = lockfile.dependencies.len(),
        "built module resolver"
    );

    Ok(GitResolver::builder()
        .cache_root(cache_root)
        .trust(trust)
        .lockfile(lockfile)
        .policy(policy)
        .build())
}

/// Writes `module-lock.json` atomically.
pub fn write_lockfile(project: &Project, lock: &Lockfile) -> anyhow::Result<()> {
    let temp_path = sibling_temp_path(&project.lockfile_path);
    let mut temp = std::fs::File::create(&temp_path)
        .with_context(|| format!("creating `{}`", temp_path.display()))?;
    lock.write(&mut temp)
        .with_context(|| format!("writing `{}`", temp_path.display()))?;
    std::fs::rename(&temp_path, &project.lockfile_path).with_context(|| {
        format!(
            "renaming `{}` to `{}`",
            temp_path.display(),
            project.lockfile_path.display()
        )
    })?;
    Ok(())
}

/// Returns true when the lockfile fully satisfies the manifest.
pub fn lockfile_satisfies(manifest: &Manifest, lock: &Lockfile) -> bool {
    lock.satisfies_manifest(manifest)
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

    let temp_path = sibling_temp_path(path);
    std::fs::write(&temp_path, &bytes)
        .with_context(|| format!("writing `{}`", temp_path.display()))?;
    std::fs::rename(&temp_path, path)
        .with_context(|| format!("renaming `{}` to `{}`", temp_path.display(), path.display()))?;
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

    let mut entries = dependencies
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    *dependencies = entries.into_iter().collect();

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

/// Re-resolves dependencies, merges with previous lock, and writes lockfile.
pub async fn relock(
    config: &Config,
    project: &Project,
    colorize: bool,
) -> anyhow::Result<RelockStats> {
    tracing::debug!("starting module relock");
    let outcome = resolve_relock_with_signer_mode(
        config,
        project,
        SignerChangeMode::from_trust_mode(config.modules.trust_mode),
        colorize,
    )
    .await?;
    write_lockfile(project, &outcome.lockfile)?;
    tracing::debug!(lockfile = %project.lockfile_path.display(), "wrote module lockfile");
    print_relock_summary(&outcome.stats, colorize);

    Ok(outcome.stats)
}

/// Re-resolves dependencies and merges with the previous lockfile.
pub(crate) async fn resolve_relock(
    config: &Config,
    project: &Project,
) -> anyhow::Result<RelockOutcome> {
    resolve_relock_with_signer_mode(config, project, SignerChangeMode::Strict, false).await
}

/// Re-resolves dependencies and merges with the previous lockfile.
pub(crate) async fn resolve_relock_with_signer_mode(
    config: &Config,
    project: &Project,
    signer_mode: SignerChangeMode,
    colorize: bool,
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
        colorize,
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
    colorize: bool,
) -> anyhow::Result<RelockOutcome> {
    let plan = resolve_relock_plan(config, project, manifest).await?;
    let trust_path = crate::analysis::default_trust_path();
    enforce_signer_trust(
        &trust_path,
        &plan.existing,
        &plan.outcome.lockfile,
        &plan.identities,
        signer_mode,
        colorize,
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
    let resolver = build_resolver(config, project, existing.clone())?;
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
    _project: &Project,
    existing: &Lockfile,
    new: &Lockfile,
    identities: &SignerIdentityMap,
    mode: SignerChangeMode,
    colorize: bool,
) -> anyhow::Result<()> {
    let trust_path = crate::analysis::default_trust_path();
    enforce_signer_trust(&trust_path, existing, new, identities, mode, colorize)
}

/// Refuses to rewrite the lockfile when regeneration would introduce,
/// change, or remove a module signer unless explicitly accepted.
///
/// New and changed signer keys require a trusted key or an interactive
/// confirmation, depending on `mode`. Removed signatures are handled by
/// mode too; strict mode refuses while interactive modes can accept them.
fn enforce_signer_trust(
    trust_path: &Path,
    existing: &Lockfile,
    new: &Lockfile,
    identities: &SignerIdentityMap,
    mode: SignerChangeMode,
    colorize: bool,
) -> anyhow::Result<()> {
    let diff = LockfileDiff::compute_with_identities(existing, new, identities);
    if !diff.has_new_signers() && !diff.has_signer_changes() {
        return Ok(());
    }

    let mut trust = TrustStore::load_or_default(trust_path)
        .with_context(|| format!("loading trust store at `{}`", trust_path.display()))?;
    let mut offenders = Vec::new();
    let mut trust_add_hint_keys = Vec::new();
    let mut trust_remove_hint_keys = Vec::new();
    let mut prompt_new_keys = Vec::new();
    let mut prompt_changed_keys = Vec::new();
    let mut prompt_removed_keys = Vec::new();
    let mut auto_new_keys = Vec::new();
    let mut auto_changed_keys = Vec::new();
    let mut auto_removed_keys = Vec::new();
    let mut trusted_keys = 0usize;
    let mut accepted_trust_changes = false;

    for added in &diff.new_signers {
        if !trust.contains_key(&added.key) {
            match mode {
                SignerChangeMode::Strict => {
                    offenders.push(added_signer_message(added, &trust));
                    push_unique_signer(&mut trust_add_hint_keys, added.key, added.identity.clone());
                }
                SignerChangeMode::Confirm => prompt_new_keys.push(added),
                SignerChangeMode::Tofu | SignerChangeMode::Auto => auto_new_keys.push(added),
            }
        }
    }

    for changed in &diff.changed_signers {
        match mode {
            SignerChangeMode::Strict => {
                if !trust.contains_key(&changed.new_key) {
                    offenders.push(changed_signer_message(changed, &trust));
                    push_unique_signer(
                        &mut trust_add_hint_keys,
                        changed.new_key,
                        changed.identity.clone(),
                    );
                }
            }
            SignerChangeMode::Confirm | SignerChangeMode::Tofu => {
                if !trust.contains_key(&changed.new_key) {
                    prompt_changed_keys.push(changed);
                }
            }
            SignerChangeMode::Auto => {
                if !trust.contains_key(&changed.new_key) {
                    auto_changed_keys.push(changed);
                }
            }
        }
    }

    for removed in &diff.removed_signers {
        if trust.contains_key(&removed.key) {
            match mode {
                SignerChangeMode::Strict => {
                    offenders.push(removed_signer_message(removed, &trust));
                    push_unique_signer(
                        &mut trust_remove_hint_keys,
                        removed.key,
                        identity_for_key(&trust, &removed.key),
                    );
                }
                SignerChangeMode::Confirm | SignerChangeMode::Tofu => {
                    prompt_removed_keys.push(removed)
                }
                SignerChangeMode::Auto => auto_removed_keys.push(removed),
            }
        }
    }

    if !prompt_new_keys.is_empty()
        || !prompt_changed_keys.is_empty()
        || !prompt_removed_keys.is_empty()
    {
        if offenders.is_empty()
            && confirm_signer_key_upgrade(
                &prompt_new_keys,
                &prompt_changed_keys,
                &prompt_removed_keys,
                &trust,
            )?
        {
            for added in &prompt_new_keys {
                trusted_keys += usize::from(trust.insert_key(added.key));
                upsert_signer_identity(&mut trust, added.key, added.identity.clone());
            }
            for changed in &prompt_changed_keys {
                trusted_keys += usize::from(trust.insert_key(changed.new_key));
                upsert_signer_identity(&mut trust, changed.new_key, changed.identity.clone());
            }
            accepted_trust_changes = true;
            if !prompt_new_keys.is_empty() || !prompt_changed_keys.is_empty() {
                trust
                    .save(trust_path)
                    .with_context(|| format!("saving trust store at `{}`", trust_path.display()))?;
            }
            prompt_new_keys.clear();
            prompt_changed_keys.clear();
            prompt_removed_keys.clear();
        }

        for added in prompt_new_keys {
            offenders.push(added_signer_message(added, &trust));
            push_unique_signer(&mut trust_add_hint_keys, added.key, added.identity.clone());
        }
        for changed in prompt_changed_keys {
            offenders.push(changed_signer_message(changed, &trust));
            push_unique_signer(
                &mut trust_add_hint_keys,
                changed.new_key,
                changed.identity.clone(),
            );
        }
        for removed in prompt_removed_keys {
            offenders.push(removed_signer_message(removed, &trust));
            push_unique_signer(
                &mut trust_remove_hint_keys,
                removed.key,
                identity_for_key(&trust, &removed.key),
            );
        }
    }

    if !auto_new_keys.is_empty() || !auto_changed_keys.is_empty() || !auto_removed_keys.is_empty() {
        if offenders.is_empty() {
            for added in &auto_new_keys {
                trusted_keys += usize::from(trust.insert_key(added.key));
                upsert_signer_identity(&mut trust, added.key, added.identity.clone());
            }
            for changed in &auto_changed_keys {
                trusted_keys += usize::from(trust.insert_key(changed.new_key));
                upsert_signer_identity(&mut trust, changed.new_key, changed.identity.clone());
            }
            accepted_trust_changes = true;
            if !auto_new_keys.is_empty() || !auto_changed_keys.is_empty() {
                trust
                    .save(trust_path)
                    .with_context(|| format!("saving trust store at `{}`", trust_path.display()))?;
            }
        } else {
            for added in auto_new_keys {
                offenders.push(added_signer_message(added, &trust));
                push_unique_signer(&mut trust_add_hint_keys, added.key, added.identity.clone());
            }
            for changed in auto_changed_keys {
                offenders.push(changed_signer_message(changed, &trust));
                push_unique_signer(
                    &mut trust_add_hint_keys,
                    changed.new_key,
                    changed.identity.clone(),
                );
            }
            for removed in auto_removed_keys {
                offenders.push(removed_signer_message(removed, &trust));
                push_unique_signer(
                    &mut trust_remove_hint_keys,
                    removed.key,
                    identity_for_key(&trust, &removed.key),
                );
            }
        }
    }

    if offenders.is_empty() {
        if accepted_trust_changes {
            print_trust_change_summary(trusted_keys, colorize);
        }
        return Ok(());
    }

    let hints = if trust_add_hint_keys.is_empty() && trust_remove_hint_keys.is_empty() {
        Vec::new()
    } else {
        vec!["accept signer trust changes with `sprocket module trust all`".to_string()]
    };

    let details = if hints.is_empty() {
        offenders.join("\n  ")
    } else {
        format!("{}\n  {}", offenders.join("\n  "), hints.join("\n  "))
    };

    anyhow::bail!(
        "refusing to update `module-lock.json`; signer trust changes require acceptance:\n  {}",
        details
    );
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SignerTrustHint {
    key: VerifyingKey,
    identity: Option<SignerIdentity>,
}

fn added_signer_message(signer: &NewSigner, trust: &TrustStore) -> String {
    format!(
        "`{}` signer key added ({})",
        signer.dep().manifest(),
        render_signer_with_trust(&signer.key, signer.identity.as_ref(), trust),
    )
}

fn changed_signer_message(changed: &ChangedSigner, trust: &TrustStore) -> String {
    match changed.old_key {
        Some(old_key) => format!(
            "`{}` signer key changed from '{}' to '{}'",
            changed.dep().manifest(),
            render_signer_with_trust(&old_key, None, trust),
            render_signer_with_trust(&changed.new_key, changed.identity.as_ref(), trust),
        ),
        None => format!(
            "`{}` signer key added to previously unsigned module ({})",
            changed.dep().manifest(),
            render_signer_with_trust(&changed.new_key, changed.identity.as_ref(), trust),
        ),
    }
}

fn removed_signer_message(removed: &RemovedSigner, trust: &TrustStore) -> String {
    format!(
        "`{}` signer key removed '{}'",
        removed.dep().manifest(),
        render_signer_with_trust(&removed.key, None, trust),
    )
}

fn render_signer_with_trust(
    key: &VerifyingKey,
    identity: Option<&SignerIdentity>,
    trust: &TrustStore,
) -> String {
    match identity {
        Some(identity) => render_signer(key, Some(identity)),
        None => match trust.identity(key) {
            Some(identity) => {
                render_identity_fields(key, identity.name.as_deref(), identity.email.as_deref())
            }
            None => render_signer(key, None),
        },
    }
}

fn render_signer(key: &VerifyingKey, identity: Option<&SignerIdentity>) -> String {
    match identity {
        Some(identity) => {
            render_identity_fields(key, identity.name.as_deref(), identity.email.as_deref())
        }
        None => key.to_openssh(),
    }
}

fn render_identity_fields(key: &VerifyingKey, name: Option<&str>, email: Option<&str>) -> String {
    let key = key.to_openssh();
    match (name, email) {
        (Some(name), Some(email)) => format!("{key} {name} <{email}>"),
        (Some(name), None) => format!("{key} {name}"),
        (None, Some(email)) => format!("{key} <{email}>"),
        (None, None) => key,
    }
}

fn push_unique_signer(
    signers: &mut Vec<SignerTrustHint>,
    key: VerifyingKey,
    identity: Option<SignerIdentity>,
) {
    if let Some(existing) = signers.iter_mut().find(|signer| signer.key == key) {
        if existing.identity.is_none() {
            existing.identity = identity;
        }
        return;
    }
    signers.push(SignerTrustHint { key, identity });
}

fn print_trust_change_summary(trusted: usize, colorize: bool) {
    if trusted == 0 {
        print_action(
            "Accepted",
            "signer trust changes",
            colorize,
            ActionColor::Green,
        );
        return;
    }

    print_action(
        "Trusted",
        format!("{trusted} signer keys"),
        colorize,
        ActionColor::Green,
    );
}

/// Adds every signer key recorded in a lockfile to the trust store.
pub(crate) fn accept_lockfile_signers(
    trust_path: &Path,
    lockfile: &Lockfile,
) -> anyhow::Result<usize> {
    let mut trust = TrustStore::load_or_default(trust_path)
        .with_context(|| format!("loading trust store at `{}`", trust_path.display()))?;
    let mut accepted = 0usize;

    for signer in lockfile_signers(lockfile, &SignerIdentityMap::new()) {
        if trust.insert_key(signer.key) {
            accepted += 1;
        }
        upsert_signer_identity(&mut trust, signer.key, signer.identity);
    }

    trust
        .save(trust_path)
        .with_context(|| format!("saving trust store at `{}`", trust_path.display()))?;
    Ok(accepted)
}

fn lockfile_signers(lockfile: &Lockfile, identities: &SignerIdentityMap) -> Vec<SignerTrustHint> {
    let mut signers = Vec::new();
    collect_lockfile_signers(
        &lockfile.dependencies,
        &mut Vec::new(),
        identities,
        &mut signers,
    );
    signers
}

fn collect_lockfile_signers(
    deps: &DependencyMap,
    chain: &mut Vec<wdl_modules::dependency::DependencyName>,
    identities: &SignerIdentityMap,
    signers: &mut Vec<SignerTrustHint>,
) {
    for (name, entry) in deps {
        chain.push(name.clone());
        if let Some(key) = entry.signer {
            push_unique_signer(signers, key, identities.get(chain).cloned());
        }
        collect_lockfile_signers(&entry.dependencies, chain, identities, signers);
        chain.pop();
    }
}

fn identity_for_key(trust: &TrustStore, key: &VerifyingKey) -> Option<SignerIdentity> {
    trust.identity(key).map(|identity| SignerIdentity {
        name: identity.name.clone(),
        email: identity.email.clone(),
    })
}

fn upsert_signer_identity(
    trust: &mut TrustStore,
    key: VerifyingKey,
    identity: Option<SignerIdentity>,
) {
    if let Some(identity) = identity {
        trust.upsert_identity(key, identity.name, identity.email);
    }
}

fn confirm_signer_key_upgrade(
    new_signers: &[&NewSigner],
    changed_signers: &[&ChangedSigner],
    removed_signers: &[&RemovedSigner],
    trust: &TrustStore,
) -> anyhow::Result<bool> {
    eprintln!("module signer key requires trust changes:");
    for signer in new_signers {
        eprintln!(
            "  `{}` signer key added: {}",
            signer.dep().manifest(),
            render_signer_with_trust(&signer.key, signer.identity.as_ref(), trust)
        );
    }
    for signer in changed_signers {
        match signer.old_key {
            Some(old_key) => eprintln!(
                "  `{}` signer key changed: {} -> {}",
                signer.dep().manifest(),
                render_signer_with_trust(&old_key, None, trust),
                render_signer_with_trust(&signer.new_key, signer.identity.as_ref(), trust)
            ),
            None => eprintln!(
                "  `{}` signer key added to previously unsigned module: {}",
                signer.dep().manifest(),
                render_signer_with_trust(&signer.new_key, signer.identity.as_ref(), trust)
            ),
        }
    }
    for signer in removed_signers {
        eprintln!(
            "  `{}` signer key removed: {}",
            signer.dep().manifest(),
            render_signer_with_trust(&signer.key, None, trust)
        );
    }
    eprint!("Accept these signer trust changes and update the lockfile? [y/N] ");
    std::io::Write::flush(&mut std::io::stderr()).context("flushing prompt")?;

    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .context("reading prompt response")?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "YES" | "Yes"))
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
        .is_some_and(|lock| lockfile_satisfies(&project.manifest, lock))
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
pub fn print_relock_summary(stats: &RelockStats, colorize: bool) {
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
        print_action("Locked", "(up to date)", colorize, ActionColor::Green);
        return;
    }

    for change in &stats.added {
        let details = change_details(
            change.path.as_deref(),
            change.selector.as_deref(),
            change.commit.as_deref(),
        );
        print_action(
            "Locked",
            format!("`{}`{details}", change.name),
            colorize,
            ActionColor::Green,
        );
    }

    for change in &stats.removed {
        let details = change_details(
            change.path.as_deref(),
            change.selector.as_deref(),
            change.commit.as_deref(),
        );
        print_action(
            "Removed",
            format!("`{}`{details}", change.name),
            colorize,
            ActionColor::Green,
        );
    }

    for change in &stats.skipped {
        let details = skipped_details(
            change.path.as_deref(),
            change.selector.as_deref(),
            change.commit.as_deref(),
        );
        print_action(
            "Skipped",
            format!("`{}`{details}", change.name),
            colorize,
            ActionColor::Yellow,
        );
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

        print_action(
            "Updated",
            format!("`{}`{details}", change.name),
            colorize,
            ActionColor::Yellow,
        );
    }
}

/// Prints update and upgrade lockfile output as lock-centric status lines.
pub fn print_locking_summary(stats: &RelockStats, colorize: bool) {
    print_action(
        "Locking",
        format!("{} packages based on `module.json`", stats.updated.len()),
        colorize,
        ActionColor::Green,
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
        print_action(
            "Updated",
            format!("{}{}", change.name.manifest(), details),
            colorize,
            ActionColor::Green,
        );
    }
}

/// Renders skipped source metadata for update output.
fn skipped_details(path: Option<&str>, selector: Option<&str>, commit: Option<&str>) -> String {
    let details = change_details(path, selector, commit);
    if details.is_empty() {
        return " (latest)".to_string();
    }

    format!(" (latest, {}", &details[2..])
}

/// Renders added or removed source metadata for summary output.
fn change_details(path: Option<&str>, selector: Option<&str>, commit: Option<&str>) -> String {
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

    if details.is_empty() {
        return String::new();
    }

    format!(" ({})", details.join(", "))
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

/// Returns a sibling temporary path used for atomic rewrite + rename.
fn sibling_temp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("module.json");
    path.with_file_name(format!("{file_name}.tmp"))
}

#[cfg(test)]
mod tests {
    use wdl_modules::dependency::DependencyName;

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
        std::fs::write(dep_dir.join("index.wdl"), b"version 1.2\n").unwrap();

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
        assert!(lockfile_satisfies(&consumer_manifest, &lock));
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

    /// Builds a one-entry lockfile whose Git dependency `dep` from `url`
    /// carries the given optional signer.
    fn signed_lockfile(
        dep: &str,
        url: &str,
        signer: Option<wdl_modules::signing::VerifyingKey>,
    ) -> Lockfile {
        use wdl_modules::lockfile::DependencyEntry;
        use wdl_modules::lockfile::ResolvedSource;

        let mut dependencies = std::collections::BTreeMap::new();
        dependencies.insert(
            dep.parse().unwrap(),
            DependencyEntry {
                source: ResolvedSource::Git {
                    git: url.parse().unwrap(),
                    sha: "0000000000000000000000000000000000000000".parse().unwrap(),
                    selector: wdl_modules::dependency::GitSelector::Version("^1".parse().unwrap()),
                    path: None,
                },
                checksum: Some(
                    "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                        .parse()
                        .unwrap(),
                ),
                signer,
                dependencies: std::collections::BTreeMap::new(),
            },
        );
        Lockfile {
            version: wdl_modules::lockfile::LOCKFILE_VERSION,
            dependencies,
        }
    }

    fn vkey(seed: u64) -> wdl_modules::signing::VerifyingKey {
        wdl_modules::signing::test_utils::signing_key_from_seed(seed).verifying_key()
    }

    fn trust_for(key: wdl_modules::signing::VerifyingKey) -> TrustStore {
        let mut store = TrustStore::default();
        store.insert_key(key);
        store
    }

    fn trust_path(store: &TrustStore) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trust.toml");
        store.save(&path).unwrap();
        (dir, path)
    }

    fn empty_trust_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trust.toml");
        (dir, path)
    }

    #[test]
    fn enforce_signer_trust_refuses_untrusted_key_change() {
        let url = "https://example.com/repo";
        let existing = signed_lockfile("dep", url, Some(vkey(1)));
        let new = signed_lockfile("dep", url, Some(vkey(2)));

        // With an empty trust store the changed key is not accepted.
        let (_dir, path) = empty_trust_path();
        let err = enforce_signer_trust(
            &path,
            &existing,
            &new,
            &SignerIdentityMap::new(),
            SignerChangeMode::Strict,
            false,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("signer key changed"),
            "unexpected error: {err}"
        );

        // Once the new key is trusted, the change is allowed.
        let (_dir, path) = trust_path(&trust_for(vkey(2)));
        enforce_signer_trust(
            &path,
            &existing,
            &new,
            &SignerIdentityMap::new(),
            SignerChangeMode::Strict,
            false,
        )
        .expect("a trusted new key should be accepted");
    }

    #[test]
    fn enforce_signer_trust_confirm_allows_trusted_key_change() {
        let url = "https://example.com/repo";
        let existing = signed_lockfile("dep", url, Some(vkey(1)));
        let new = signed_lockfile("dep", url, Some(vkey(2)));

        let (_dir, path) = trust_path(&trust_for(vkey(2)));
        enforce_signer_trust(
            &path,
            &existing,
            &new,
            &SignerIdentityMap::new(),
            SignerChangeMode::Confirm,
            false,
        )
        .expect("a globally trusted replacement key should not prompt or fail");
    }

    #[test]
    fn enforce_signer_trust_refuses_removal_while_pinned() {
        let url = "https://example.com/repo";
        let existing = signed_lockfile("dep", url, Some(vkey(1)));
        let new = signed_lockfile("dep", url, None);

        // While the key is still pinned, the downgrade to unsigned is refused.
        let (_dir, path) = trust_path(&trust_for(vkey(1)));
        let err = enforce_signer_trust(
            &path,
            &existing,
            &new,
            &SignerIdentityMap::new(),
            SignerChangeMode::Strict,
            false,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("signer key removed"),
            "unexpected error: {err}"
        );

        // With no pin, the downgrade is accepted.
        let (_dir, path) = empty_trust_path();
        enforce_signer_trust(
            &path,
            &existing,
            &new,
            &SignerIdentityMap::new(),
            SignerChangeMode::Strict,
            false,
        )
        .expect("an unpinned downgrade should be accepted");
    }

    #[test]
    fn enforce_signer_trust_auto_keeps_key_when_added_and_removed_together() {
        let url = "https://example.com/repo";
        let key = vkey(7);
        let dep_added: DependencyName = "added".parse().unwrap();
        let dep_removed: DependencyName = "removed".parse().unwrap();

        let mut existing = Lockfile::default();
        let mut existing_added = signed_lockfile("added", url, None);
        existing.dependencies.insert(
            dep_added.clone(),
            existing_added.dependencies.remove(&dep_added).unwrap(),
        );
        let mut existing_removed = signed_lockfile("removed", url, Some(key));
        existing.dependencies.insert(
            dep_removed.clone(),
            existing_removed.dependencies.remove(&dep_removed).unwrap(),
        );

        let mut new = Lockfile::default();
        let mut new_added = signed_lockfile("added", url, Some(key));
        new.dependencies.insert(
            dep_added.clone(),
            new_added.dependencies.remove(&dep_added).unwrap(),
        );
        let mut new_removed = signed_lockfile("removed", url, None);
        new.dependencies.insert(
            dep_removed.clone(),
            new_removed.dependencies.remove(&dep_removed).unwrap(),
        );

        let (_dir, path) = trust_path(&trust_for(key));
        enforce_signer_trust(
            &path,
            &existing,
            &new,
            &SignerIdentityMap::new(),
            SignerChangeMode::Auto,
            false,
        )
        .expect("auto mode should accept the batch");
        let trust = TrustStore::load_or_default(&path).unwrap();
        assert!(
            trust.contains_key(&key),
            "key should remain trusted when another dependency still uses it"
        );
    }

    #[test]
    fn enforce_signer_trust_auto_keeps_removed_signer_trusted() {
        let url = "https://example.com/repo";
        let key = vkey(7);
        let existing = signed_lockfile("dep", url, Some(key));
        let new = signed_lockfile("dep", url, None);

        let (_dir, path) = trust_path(&trust_for(key));
        enforce_signer_trust(
            &path,
            &existing,
            &new,
            &SignerIdentityMap::new(),
            SignerChangeMode::Auto,
            false,
        )
        .expect("auto mode should accept the removed signature");
        let trust = TrustStore::load_or_default(&path).unwrap();
        assert!(
            trust.contains_key(&key),
            "accepting a removed module signature should not remove global trust for the signer \
             key"
        );
    }

    #[test]
    fn enforce_signer_trust_allows_unchanged_and_refuses_new_untrusted_signer() {
        let url = "https://example.com/repo";
        let signed = signed_lockfile("dep", url, Some(vkey(1)));
        // Unchanged signer: no-op.
        let (_dir, path) = empty_trust_path();
        enforce_signer_trust(
            &path,
            &signed,
            &signed,
            &SignerIdentityMap::new(),
            SignerChangeMode::Strict,
            false,
        )
        .unwrap();

        // A new signer requires explicit trust.
        let empty = Lockfile::default();
        let err = enforce_signer_trust(
            &path,
            &empty,
            &signed,
            &SignerIdentityMap::new(),
            SignerChangeMode::Strict,
            false,
        )
        .expect_err("a newly introduced signer should require trust");
        assert!(
            err.to_string().contains("signer key added"),
            "unexpected error: {err}"
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
