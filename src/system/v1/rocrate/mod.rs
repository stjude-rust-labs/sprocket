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
/// Process Run Crate profile, claimed by both workflow and task targets.
pub const PROCESS_PROFILE: &str = "https://w3id.org/ro/wfrun/process/0.1";
/// Workflow RO-Crate profile.
pub const WORKFLOW_RO_CRATE_PROFILE: &str = "https://w3id.org/workflowhub/workflow-ro-crate/1.0";
/// Profiles a workflow-target crate conforms to. Task targets conform only to
/// the Process Run Crate profile, since there is no `ComputationalWorkflow`.
pub const PROFILES: &[&str] = &[
    PROCESS_PROFILE,
    "https://w3id.org/ro/wfrun/workflow/0.1",
    WORKFLOW_RO_CRATE_PROFILE,
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

    /// End-to-end: analyze a tiny workflow, build a `RunCrateContext` from
    /// hand-built provenance, and assert the written crate is well-formed. Uses
    /// the real analyzer; no execution backend (and thus no Docker) is required.
    #[tokio::test]
    async fn writes_workflow_run_crate_end_to_end() {
        use std::str::FromStr as _;

        use chrono::Utc;
        use uuid::Uuid;
        use wdl::engine::Inputs;
        use wdl::engine::Outputs;
        use wdl::engine::Value;
        use wdl::engine::WorkflowInputs;

        use crate::analysis::Source;
        use crate::commands::validate::analyze_source;
        use crate::system::v1::db::models::Run;
        use crate::system::v1::db::models::RunStatus;
        use crate::system::v1::db::models::Session;
        use crate::system::v1::db::models::SprocketCommand;
        use crate::system::v1::exec::Target;
        use crate::system::v1::fs::OutputDirectory;

        let dir = tempfile::tempdir().unwrap();
        let wdl = dir.path().join("source.wdl");
        std::fs::write(
            &wdl,
            r#"version 1.3

workflow myworkflow {
    input {
        String greeting = "hi"
    }

    output {
        String message = greeting
    }
}
"#,
        )
        .unwrap();

        let source = Source::from_str(wdl.to_str().unwrap()).unwrap();
        let document = analyze_source(&source, None)
            .await
            .expect("analysis should succeed");

        let output_dir = OutputDirectory::new(dir.path().join("out"));
        let run_dir = output_dir.ensure_workflow_run("myworkflow").unwrap();

        let now = Utc::now();
        let session = Session {
            uuid: Uuid::new_v4(),
            subcommand: SprocketCommand::Run,
            created_by: "tester".to_string(),
            created_at: now,
        };
        let run = Run {
            uuid: Uuid::new_v4(),
            session_uuid: session.uuid,
            name: "tiny-run".to_string(),
            source: "source.wdl".to_string(),
            target: Some("myworkflow".to_string()),
            status: RunStatus::Completed,
            inputs: "{}".to_string(),
            outputs: Some("{}".to_string()),
            error: None,
            directory: Some(run_dir.root().display().to_string()),
            index_directory: None,
            started_at: Some(now),
            completed_at: Some(now),
            created_at: now,
        };
        let target = Target::Workflow("myworkflow".to_string());
        let mut workflow_inputs = WorkflowInputs::default();
        workflow_inputs.set("greeting", Value::from("hi".to_string()));
        let inputs = Inputs::Workflow(workflow_inputs);
        let outputs = Outputs::from_iter([("message".to_string(), Value::from("hi".to_string()))]);

        let ctx = RunCrateContext {
            run: &run,
            session: Some(&session),
            document: &document,
            target: &target,
            inputs: &inputs,
            outputs: &outputs,
            run_dir: &run_dir,
            engine: EngineInfo::from_build(),
        };

        write_run_crate(&ctx, &RoCrateOptions::from_flags(true, false, false, false))
            .expect("should write the crate");

        let meta = run_dir.root().join(METADATA_FILE);
        assert!(meta.exists(), "metadata file should exist");

        let text = std::fs::read_to_string(&meta).unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        let graph = json["@graph"].as_array().expect("@graph array");
        let run_action = graph
            .iter()
            .find(|e| e["@id"] == "#run")
            .expect("#run entity");
        assert_eq!(run_action["@type"], "CreateAction");

        for marker in [
            "https://w3id.org/ro/wfrun/process/0.1",
            "https://w3id.org/ro/wfrun/workflow/0.1",
            "https://w3id.org/workflowhub/workflow-ro-crate/1.0",
            ROCRATE_CONTEXT,
            WFRUN_CONTEXT,
        ] {
            assert!(text.contains(marker), "crate should reference `{marker}`");
        }
        let compact = text.split_whitespace().collect::<String>();
        assert!(compact.contains("\"input\":[{\"@id\":\"#param-in-greeting\"}]"));
        assert!(compact.contains("\"output\":[{\"@id\":\"#param-out-message\"}]"));
        assert!(compact.contains("\"exampleOfWork\":{\"@id\":\"#param-in-greeting\"}"));
        assert!(compact.contains("\"exampleOfWork\":{\"@id\":\"#param-out-message\"}"));
        assert!(compact.contains("\"valueRequired\":false"));

        // The WDL source is materialized into the crate.
        assert!(run_dir.root().join(WORKFLOW_ID).exists());
    }

    /// Unknown timing and an unknown submitter are omitted, not fabricated.
    #[tokio::test]
    async fn omits_unknown_timing_and_agent() {
        use std::str::FromStr as _;

        use chrono::Utc;
        use uuid::Uuid;
        use wdl::engine::Inputs;
        use wdl::engine::Outputs;
        use wdl::engine::WorkflowInputs;

        use crate::analysis::Source;
        use crate::commands::validate::analyze_source;
        use crate::system::v1::db::models::Run;
        use crate::system::v1::db::models::RunStatus;
        use crate::system::v1::exec::Target;
        use crate::system::v1::fs::OutputDirectory;

        let dir = tempfile::tempdir().unwrap();
        let wdl = dir.path().join("source.wdl");
        std::fs::write(
            &wdl,
            "version 1.3\n\nworkflow myworkflow {\n    output {\n    }\n}\n",
        )
        .unwrap();

        let source = Source::from_str(wdl.to_str().unwrap()).unwrap();
        let document = analyze_source(&source, None).await.expect("analysis");

        let output_dir = OutputDirectory::new(dir.path().join("out"));
        let run_dir = output_dir.ensure_workflow_run("myworkflow").unwrap();

        // No `started_at`/`completed_at`, and no session.
        let run = Run {
            uuid: Uuid::new_v4(),
            session_uuid: Uuid::new_v4(),
            name: "tiny-run".to_string(),
            source: "source.wdl".to_string(),
            target: Some("myworkflow".to_string()),
            status: RunStatus::Completed,
            inputs: "{}".to_string(),
            outputs: Some("{}".to_string()),
            error: None,
            directory: Some(run_dir.root().display().to_string()),
            index_directory: None,
            started_at: None,
            completed_at: None,
            created_at: Utc::now(),
        };
        let target = Target::Workflow("myworkflow".to_string());
        let inputs = Inputs::Workflow(WorkflowInputs::default());
        let outputs = Outputs::default();

        let ctx = RunCrateContext {
            run: &run,
            session: None,
            document: &document,
            target: &target,
            inputs: &inputs,
            outputs: &outputs,
            run_dir: &run_dir,
            engine: EngineInfo::from_build(),
        };

        write_run_crate(&ctx, &RoCrateOptions::from_flags(true, false, false, false))
            .expect("should write the crate");

        let text = std::fs::read_to_string(run_dir.root().join(METADATA_FILE)).unwrap();
        assert!(!text.contains("startTime"), "must not fabricate startTime");
        assert!(!text.contains("endTime"), "must not fabricate endTime");
        assert!(!text.contains("#agent"), "must not fabricate an agent");
        assert!(!text.contains("unknown"), "must not invent an agent name");
        // The required root `datePublished` is still present (a real timestamp).
        assert!(text.contains("datePublished"));
    }

    /// A direct task target emits a Process Run Crate: no `ComputationalWorkflow`
    /// type, no workflow profile, no `OrganizeAction`.
    #[tokio::test]
    async fn task_target_emits_process_crate() {
        use std::str::FromStr as _;

        use chrono::Utc;
        use uuid::Uuid;
        use wdl::engine::Inputs;
        use wdl::engine::Outputs;
        use wdl::engine::TaskInputs;

        use crate::analysis::Source;
        use crate::commands::validate::analyze_source;
        use crate::system::v1::db::models::Run;
        use crate::system::v1::db::models::RunStatus;
        use crate::system::v1::exec::Target;
        use crate::system::v1::fs::OutputDirectory;

        let dir = tempfile::tempdir().unwrap();
        let wdl = dir.path().join("source.wdl");
        std::fs::write(
            &wdl,
            "version 1.3\n\ntask mytask {\n    command <<<>>>\n    output {\n        String message = \"hi\"\n    }\n}\n",
        )
        .unwrap();

        let source = Source::from_str(wdl.to_str().unwrap()).unwrap();
        let document = analyze_source(&source, None).await.expect("analysis");

        let output_dir = OutputDirectory::new(dir.path().join("out"));
        let run_dir = output_dir.ensure_workflow_run("mytask").unwrap();

        let now = Utc::now();
        let run = Run {
            uuid: Uuid::new_v4(),
            session_uuid: Uuid::new_v4(),
            name: "tiny-run".to_string(),
            source: "source.wdl".to_string(),
            target: Some("mytask".to_string()),
            status: RunStatus::Completed,
            inputs: "{}".to_string(),
            outputs: Some("{}".to_string()),
            error: None,
            directory: Some(run_dir.root().display().to_string()),
            index_directory: None,
            started_at: Some(now),
            completed_at: Some(now),
            created_at: now,
        };
        let target = Target::Task("mytask".to_string());
        let inputs = Inputs::Task(TaskInputs::default());
        let outputs = Outputs::default();

        let ctx = RunCrateContext {
            run: &run,
            session: None,
            document: &document,
            target: &target,
            inputs: &inputs,
            outputs: &outputs,
            run_dir: &run_dir,
            engine: EngineInfo::from_build(),
        };

        write_run_crate(&ctx, &RoCrateOptions::from_flags(true, false, false, false))
            .expect("should write the crate");

        let text = std::fs::read_to_string(run_dir.root().join(METADATA_FILE)).unwrap();
        assert!(
            !text.contains("ComputationalWorkflow"),
            "a task target must not claim to be a workflow"
        );
        assert!(text.contains(PROCESS_PROFILE));
        assert!(
            !text.contains(WORKFLOW_RO_CRATE_PROFILE),
            "a task target must not claim the Workflow RO-Crate profile"
        );
        assert!(
            !text.contains("OrganizeAction"),
            "a single task has no orchestration action"
        );
    }
}
