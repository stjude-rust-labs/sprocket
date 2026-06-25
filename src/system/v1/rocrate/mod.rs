//! Workflow Run RO-Crate emission for completed runs.

mod build;
mod context;
mod formal;
mod source;
mod value;

pub use build::add_workflow_parts;
pub use build::build_run_crate;
pub use context::EngineInfo;
pub use context::RunCrateContext;
pub use formal::formal_parameter;
pub use source::materialize_sources;
pub use value::value_to_entities;

/// RO-Crate 1.1 base context.
pub const ROCRATE_CONTEXT: &str = "https://w3id.org/ro/crate/1.1/context";
/// Workflow Run RO-Crate term context.
pub const WFRUN_CONTEXT: &str = "https://w3id.org/ro/terms/workflow-run/context";
/// Profiles the root dataset conforms to in M1 (workflow + workflow-ro-crate).
/// M2 adds the Provenance Run Crate profile.
pub const PROFILES: &[&str] = &[
    "https://w3id.org/ro/wfrun/workflow/0.1",
    "https://w3id.org/workflowhub/workflow-ro-crate/1.0",
];
/// The metadata descriptor filename.
pub const METADATA_FILE: &str = "ro-crate-metadata.json";
/// The main workflow entity id, materialized under `workflow/`.
pub const WORKFLOW_ID: &str = "workflow/workflow.wdl";

/// Controls whether and how a run emits an RO-Crate.
#[derive(Debug, Clone, Copy)]
pub struct RoCrateOptions {
    /// Whether to emit `ro-crate-metadata.json` at all.
    pub enabled: bool,
    /// Whether an emission failure should fail the command.
    pub strict: bool,
    /// Whether to compute SHA-256 digests for `File`/`Directory` entities.
    pub checksums: bool,
    /// Whether to localize input/output data values into crate-relative paths.
    pub localize: bool,
}

impl RoCrateOptions {
    /// Builds options from the `--ro-crate`, `--ro-crate-strict`,
    /// `--no-ro-crate-checksums`, and `--no-ro-crate-localize` flags. The `no_*`
    /// values are raw flags, so each behavior is enabled when its flag is
    /// `false`.
    pub fn from_flags(enabled: bool, strict: bool, no_checksums: bool, no_localize: bool) -> Self {
        Self {
            enabled,
            strict,
            checksums: !no_checksums,
            localize: !no_localize,
        }
    }

    /// Options that emit nothing (used by non-CLI run paths).
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            strict: false,
            checksums: false,
            localize: false,
        }
    }
}

/// Minimal structural validation before writing: the graph must contain the
/// metadata descriptor, the root dataset, the workflow entity, and the run
/// action.
fn validate(crate_: &rocraters::ro_crate::rocrate::RoCrate) -> anyhow::Result<()> {
    let ids = crate_.get_all_ids();
    for required in [METADATA_FILE, "./", WORKFLOW_ID, "#run"] {
        anyhow::ensure!(
            ids.iter().any(|i| *i == required),
            "RO-Crate missing required entity `{required}`"
        );
    }
    Ok(())
}

/// Builds, materializes sources for, validates, and writes the crate into the
/// run directory.
pub fn write_run_crate(ctx: &RunCrateContext<'_>, opts: &RoCrateOptions) -> anyhow::Result<()> {
    let mut crate_ = build_run_crate(ctx, opts)?;
    let import_ids = materialize_sources(ctx.document, ctx.run_dir.root(), &mut crate_.graph)?;
    add_workflow_parts(&mut crate_, &import_ids);
    validate(&crate_)?;
    let path = ctx.run_dir.root().join(METADATA_FILE);
    rocraters::ro_crate::write::write_crate(&crate_, path.to_string_lossy().to_string())
        .map_err(|e| anyhow::anyhow!("writing RO-Crate metadata: {e}"))?;
    Ok(())
}

/// Collects provenance and emits the crate for a completed run.
///
/// Honors `opts.strict`: on failure it returns the error when strict, else logs
/// a warning and returns `Ok`. Called *after* the run is recorded complete, so a
/// strict failure surfaces as a nonzero command exit without changing the run's
/// recorded success.
#[allow(clippy::too_many_arguments)]
pub async fn emit(
    db: &dyn crate::system::v1::db::Database,
    run_id: uuid::Uuid,
    target: &crate::system::v1::exec::Target,
    document: &wdl::analysis::Document,
    inputs: &wdl::engine::Inputs,
    outputs: &wdl::engine::Outputs,
    run_dir: &crate::system::v1::fs::RunDirectory,
    opts: &RoCrateOptions,
) -> anyhow::Result<()> {
    use anyhow::Context as _;

    if !opts.enabled {
        return Ok(());
    }

    let result = async {
        let run = db
            .get_run(run_id)
            .await?
            .with_context(|| format!("run `{run_id}` not found"))?;
        let session = db.get_session(run.session_uuid).await?;
        let ctx = RunCrateContext {
            run: &run,
            session: session.as_ref(),
            document,
            target,
            inputs,
            outputs,
            run_dir,
            engine: EngineInfo::from_build(),
        };
        write_run_crate(&ctx, opts)
    }
    .await;

    match result {
        Ok(()) => Ok(()),
        Err(e) if opts.strict => Err(e.context("RO-Crate emission failed (--ro-crate-strict)")),
        Err(e) => {
            tracing::warn!("requested RO-Crate artifact was not written: {e:#}");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_from_flags_maps_correctly() {
        let o = RoCrateOptions::from_flags(true, false, false, false);
        assert!(o.enabled && o.checksums && o.localize && !o.strict);
        let off = RoCrateOptions::disabled();
        assert!(!off.enabled);
    }

    #[test]
    fn validation_rejects_crate_without_required_entities() {
        let crate_ = rocraters::ro_crate::rocrate::RoCrate {
            context: rocraters::ro_crate::rocrate::RoCrateContext::ReferenceContext(
                ROCRATE_CONTEXT.to_string(),
            ),
            graph: Vec::new(),
        };
        assert!(validate(&crate_).is_err());
    }
}
