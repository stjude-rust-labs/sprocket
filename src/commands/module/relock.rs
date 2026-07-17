//! Partial relock planning for module porcelain commands.

use std::sync::Arc;

use wdl_modules::Lockfile;
use wdl_modules::Manifest;
use wdl_modules::Resolver as _;
use wdl_modules::module::Module;
use wdl_modules::resolver::lock::RelockOutcome;
use wdl_modules::resolver::lock::SignerIdentityMap;
use wdl_modules::resolver::lock::partial_relock;
use wdl_modules::resolver::lock::signer_identity_map;

use super::Project;
use super::load_lockfile;
use super::resolver::ResolverEnvironment;
use super::signer_policy::enforce_lockfile_signer_policy;
use super::trust_policy::SignerChangeMode;
use crate::commands::output::CommandOutput;
use crate::config::Config;

/// A resolved lockfile update plus signer metadata gathered while
/// verifying the dependency tree.
pub(crate) struct RelockPlan {
    /// The lockfile currently on disk, or an empty lockfile when absent.
    pub(crate) existing: Lockfile,
    /// The relock result that should be written after policy passes.
    pub(crate) outcome: RelockOutcome,
    /// Signer identity metadata from freshly verified `module.sig` files.
    pub(crate) identities: SignerIdentityMap,
}

/// Plans partial relocks for add, remove, lock, and automatic locking.
pub(crate) struct RelockPlanner<'a> {
    /// Module configuration governing cache, trust, and policy.
    config: &'a Config,
    /// The project whose lockfile is being refreshed.
    project: &'a Project,
}

impl<'a> RelockPlanner<'a> {
    /// Creates a partial relock planner for a project.
    pub(crate) fn new(config: &'a Config, project: &'a Project) -> Self {
        Self { config, project }
    }

    /// Re-resolves dependencies for a manifest and merges the result with the
    /// previous lockfile without applying signer-change policy.
    pub(crate) async fn plan(&self, manifest: Arc<Manifest>) -> anyhow::Result<RelockPlan> {
        let module = Module::new(manifest, self.project.root.clone());
        let existing = load_lockfile(self.project)?.unwrap_or_default();
        tracing::debug!(
            existing = existing.dependencies.len(),
            declared = module.manifest.dependencies.len(),
            "loaded relock inputs"
        );
        let environment = ResolverEnvironment::from_config(self.config)?;
        let resolver = environment.resolver(existing.clone())?;
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

    /// Re-resolves dependencies for a manifest, enforces signer-change policy,
    /// and returns the relock outcome ready to be written.
    pub(crate) async fn plan_and_enforce(
        &self,
        manifest: Arc<Manifest>,
        mode: SignerChangeMode,
        output: CommandOutput,
    ) -> anyhow::Result<RelockOutcome> {
        let plan = self.plan(manifest).await?;
        enforce_lockfile_signer_policy(
            &plan.existing,
            &plan.outcome.lockfile,
            &plan.identities,
            mode,
            output,
        )?;
        Ok(plan.outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn plan_partially_relocks_local_path_dependency() {
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
        let manifest_path = consumer_dir.join("module.json");
        std::fs::write(
            &manifest_path,
            br#"{"name":"consumer","license":"MIT","dependencies":{"dep":{"path":"../dep"}}}"#,
        )
        .unwrap();

        let manifest = Arc::new(Manifest::parse(&std::fs::read(&manifest_path).unwrap()).unwrap());
        let lockfile_path = manifest_path.with_file_name(wdl_modules::LOCKFILE_FILENAME);
        let project = Project {
            manifest_path,
            root: consumer_dir,
            manifest: manifest.clone(),
            lockfile_path,
        };

        let mut config = Config::default();
        config.modules.cache_path = Some(work.path().join("cache"));

        let plan = RelockPlanner::new(&config, &project)
            .plan(project.manifest.clone())
            .await
            .unwrap();
        assert_eq!(plan.outcome.lockfile.dependencies.len(), 1);
    }
}
