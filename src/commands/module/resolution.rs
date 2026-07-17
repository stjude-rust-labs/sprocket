//! Module dependency resolution and relocking.

use std::path::Path;
use std::sync::Arc;

use clap::ValueEnum;
use wdl_modules::Lockfile;
use wdl_modules::Manifest;
use wdl_modules::Resolver as _;
use wdl_modules::module::Module;
use wdl_modules::resolver::GitResolver;
use wdl_modules::resolver::RelockOutcome;
use wdl_modules::resolver::ResolverPolicy;
use wdl_modules::resolver::SignerIdentityMap;
use wdl_modules::resolver::TrustMode;
use wdl_modules::resolver::partial_relock;
use wdl_modules::resolver::signer_identity_map;

use super::LockedProject;
use super::Project;
use super::load_lockfile;
use super::trust_policy::SignerChangeMode;
use super::trust_policy::enforce_signer_trust;
use super::trust_policy::load_trust_store;
use crate::commands::output::CommandOutput;
use crate::config::Config;

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
#[value(rename_all = "kebab-case")]
pub enum TrustModeArg {
    /// Prompt before trusting signer keys.
    Confirm,
    /// Trust first-seen signer keys automatically, then prompt on changes.
    Tofu,
    /// Trust signer keys automatically.
    AutoAccept,
}

impl From<TrustModeArg> for TrustMode {
    fn from(value: TrustModeArg) -> Self {
        match value {
            TrustModeArg::Confirm => TrustMode::Confirm,
            TrustModeArg::Tofu => TrustMode::Tofu,
            TrustModeArg::AutoAccept => TrustMode::AutoAccept,
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

/// Re-resolves dependencies and merges with the previous lockfile.
pub(crate) async fn resolve_relock(
    config: &Config,
    project: &Project,
) -> anyhow::Result<RelockOutcome> {
    resolve_relock_with_signer_mode(
        config,
        project,
        SignerChangeMode::Strict,
        CommandOutput::new(false),
    )
    .await
}

/// Re-resolves dependencies and merges with the previous lockfile.
pub(crate) async fn resolve_relock_with_signer_mode(
    config: &Config,
    project: &Project,
    signer_mode: SignerChangeMode,
    output: CommandOutput,
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
        output,
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
    output: CommandOutput,
) -> anyhow::Result<RelockOutcome> {
    let plan = resolve_relock_plan(config, project, manifest).await?;
    let trust_path = crate::analysis::default_trust_path();
    enforce_signer_trust(
        &trust_path,
        &plan.existing,
        &plan.outcome.lockfile,
        &plan.identities,
        signer_mode,
        output,
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
    output: CommandOutput,
) -> anyhow::Result<()> {
    let trust_path = crate::analysis::default_trust_path();
    enforce_signer_trust(&trust_path, existing, new, identities, mode, output)
}

/// Regenerates `module-lock.json` before execution when it is missing or
/// out of date with the governing `module.json`.
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

    let project = LockedProject::acquire(project)?;
    let existing = load_lockfile(project.project())?;
    if existing
        .as_ref()
        .is_some_and(|lock| lock.satisfies_manifest(&project.project().manifest))
    {
        return Ok(());
    }

    tracing::info!(
        manifest = %project.project().manifest_path.display(),
        lockfile_present = existing.is_some(),
        "`module-lock.json` is missing or out of date; regenerating before execution"
    );
    let outcome = resolve_relock(config, project.project()).await?;
    project.commit(None, Some(&outcome.lockfile))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ensure_lockfile_current_regenerates_missing_lockfile() {
        let work = tempfile::tempdir().unwrap();
        let dep_dir = work.path().join("dep");
        std::fs::create_dir_all(&dep_dir).unwrap();
        std::fs::write(
            dep_dir.join("module.json"),
            br#"{"name":"dep","license":"MIT"}"#,
        )
        .unwrap();
        std::fs::write(dep_dir.join("index.wdl"), b"version 1.3\n").unwrap();

        let consumer_dir = work.path().join("consumer");
        std::fs::create_dir_all(&consumer_dir).unwrap();
        std::fs::write(
            consumer_dir.join("module.json"),
            br#"{"name":"consumer","license":"MIT","dependencies":{"dep":{"path":"../dep"}}}"#,
        )
        .unwrap();

        let lockfile_path = consumer_dir.join(wdl_modules::LOCKFILE_FILENAME);
        assert!(!lockfile_path.exists());

        let mut config = Config::default();
        config.modules.cache_path = Some(work.path().join("cache"));
        ensure_lockfile_current(&config, &consumer_dir)
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
}
