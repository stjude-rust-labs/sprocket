//! Rebuild index tests.

use std::fs;

use anyhow::Result;
use indexmap::IndexMap;
use sprocket::OutputDirectory;
use sprocket::database::Database;
use sprocket::database::InvocationMethod;
use sprocket::database::SqliteDatabase;
use sprocket::provenance::create_index_entries;
use sprocket::provenance::rebuild_index;
use sqlx::SqlitePool;
use tempfile::TempDir;
use uuid::Uuid;
use wdl::engine::HostPath;
use wdl::engine::PrimitiveValue;
use wdl::engine::Value;

use super::normalize_path;

#[sqlx::test]
async fn rebuild_index_full(pool: SqlitePool) -> Result<()> {
    let temp = TempDir::new()?;
    let output_dir = OutputDirectory::new(temp.path());
    let db = SqliteDatabase::from_pool(pool).await?;

    let invocation_id = Uuid::new_v4();
    db.create_invocation(invocation_id, InvocationMethod::Cli, None)
        .await?;

    // First workflow execution
    let workflow_id1 = Uuid::new_v4();
    db.create_workflow(
        workflow_id1,
        invocation_id,
        String::from("test"),
        String::from("file://test.wdl"),
        String::from("{}"),
        String::from("test-workflow-run1"),
    )
    .await?;

    let exec_dir1 = output_dir.ensure_workflow_run("test-workflow-run1")?;
    fs::write(exec_dir1.join("outputs.json"), "{}")?;
    fs::write(exec_dir1.join("satisfaction_survey.tsv"), "old survey")?;
    fs::write(exec_dir1.join("styling_metrics.json"), "old metrics")?;

    let outputs1: IndexMap<String, Value> = [
        (
            "satisfaction_survey".to_string(),
            Value::Primitive(PrimitiveValue::File(HostPath::new(
                "satisfaction_survey.tsv",
            ))),
        ),
        (
            "styling_metrics".to_string(),
            Value::Primitive(PrimitiveValue::File(HostPath::new("styling_metrics.json"))),
        ),
    ]
    .into_iter()
    .collect();

    create_index_entries(
        &db,
        workflow_id1,
        &output_dir,
        "test-workflow-run1",
        "yak",
        &outputs1,
    )
    .await?;

    // Verify first index was created
    let index_dir = output_dir.index_dir("yak");
    assert!(index_dir.join("outputs.json").exists());
    assert!(index_dir.join("outputs.json").is_symlink());
    assert!(index_dir.join("satisfaction_survey.tsv").exists());
    assert!(index_dir.join("satisfaction_survey.tsv").is_symlink());
    assert!(index_dir.join("styling_metrics.json").exists());
    assert!(index_dir.join("styling_metrics.json").is_symlink());

    let survey_content = fs::read_to_string(index_dir.join("satisfaction_survey.tsv"))?;
    assert_eq!(survey_content, "old survey");
    let metrics_content = fs::read_to_string(index_dir.join("styling_metrics.json"))?;
    assert_eq!(metrics_content, "old metrics");

    // Verify database entries for first workflow
    let entries1 = db.list_index_log_entries_by_workflow(workflow_id1).await?;
    assert_eq!(entries1.len(), 3); // `outputs.json`, `satisfaction_survey.tsv`, `styling_metrics.json`

    // Sleep to ensure second workflow gets a different timestamp.
    // SQLite `current_timestamp` has second precision, so we need at least 1
    // second.
    std::thread::sleep(std::time::Duration::from_millis(1100));

    // Second workflow execution (rerun with newer data)
    let workflow_id2 = Uuid::new_v4();
    db.create_workflow(
        workflow_id2,
        invocation_id,
        String::from("test"),
        String::from("file://test.wdl"),
        String::from("{}"),
        String::from("test-workflow-run2"),
    )
    .await?;

    let exec_dir2 = output_dir.ensure_workflow_run("test-workflow-run2")?;
    fs::write(exec_dir2.join("outputs.json"), "{}")?;
    fs::write(exec_dir2.join("satisfaction_survey.tsv"), "new survey")?;
    fs::write(exec_dir2.join("styling_metrics.json"), "new metrics")?;

    let outputs2: IndexMap<String, Value> = [
        (
            "satisfaction_survey".to_string(),
            Value::Primitive(PrimitiveValue::File(HostPath::new(
                "satisfaction_survey.tsv",
            ))),
        ),
        (
            "styling_metrics".to_string(),
            Value::Primitive(PrimitiveValue::File(HostPath::new("styling_metrics.json"))),
        ),
    ]
    .into_iter()
    .collect();

    create_index_entries(
        &db,
        workflow_id2,
        &output_dir,
        "test-workflow-run2",
        "yak",
        &outputs2,
    )
    .await?;

    // Verify second index replaced the first (symlinks point to newer data)
    assert!(index_dir.join("satisfaction_survey.tsv").exists());
    assert!(index_dir.join("satisfaction_survey.tsv").is_symlink());
    assert!(index_dir.join("styling_metrics.json").exists());
    assert!(index_dir.join("styling_metrics.json").is_symlink());

    let survey_content = fs::read_to_string(index_dir.join("satisfaction_survey.tsv"))?;
    assert_eq!(survey_content, "new survey");
    let metrics_content = fs::read_to_string(index_dir.join("styling_metrics.json"))?;
    assert_eq!(metrics_content, "new metrics");

    // Verify database entries for second workflow
    let entries2 = db.list_index_log_entries_by_workflow(workflow_id2).await?;
    assert_eq!(entries2.len(), 3);

    // Verify we have only 3 latest entries (one per unique index path)
    let mut all_entries = db.list_latest_index_entries().await?;
    assert_eq!(all_entries.len(), 3);

    // Sort entries by index_path for deterministic assertions
    all_entries.sort_by(|a, b| a.index_path.cmp(&b.index_path));

    // Verify first entry: `outputs.json` from `workflow_id2`
    assert_eq!(all_entries[0].workflow_id, workflow_id2);
    assert_eq!(
        normalize_path(all_entries[0].index_path.to_str().unwrap()),
        "index/yak/outputs.json"
    );
    assert_eq!(
        normalize_path(all_entries[0].target_path.to_str().unwrap()),
        "runs/test-workflow-run2/outputs.json"
    );

    // Verify second entry: `satisfaction_survey.tsv` from `workflow_id2`
    assert_eq!(all_entries[1].workflow_id, workflow_id2);
    assert_eq!(
        normalize_path(all_entries[1].index_path.to_str().unwrap()),
        "index/yak/satisfaction_survey.tsv"
    );
    assert_eq!(
        normalize_path(all_entries[1].target_path.to_str().unwrap()),
        "runs/test-workflow-run2/satisfaction_survey.tsv"
    );

    // Verify third entry: `styling_metrics.json` from `workflow_id2`
    assert_eq!(all_entries[2].workflow_id, workflow_id2);
    assert_eq!(
        normalize_path(all_entries[2].index_path.to_str().unwrap()),
        "index/yak/styling_metrics.json"
    );
    assert_eq!(
        normalize_path(all_entries[2].target_path.to_str().unwrap()),
        "runs/test-workflow-run2/styling_metrics.json"
    );

    // Delete the entire index directory
    fs::remove_dir_all(output_dir.root().join("index"))?;
    assert!(!index_dir.exists());

    // Rebuild index from database
    rebuild_index(&db, &output_dir).await?;

    // Verify all symlinks were recreated
    assert!(index_dir.exists());
    assert!(index_dir.join("outputs.json").exists());
    assert!(index_dir.join("outputs.json").is_symlink());
    assert!(index_dir.join("satisfaction_survey.tsv").exists());
    assert!(index_dir.join("satisfaction_survey.tsv").is_symlink());
    assert!(index_dir.join("styling_metrics.json").exists());
    assert!(index_dir.join("styling_metrics.json").is_symlink());

    // Verify symlinks point to the latest (second) workflow data
    let survey_content = fs::read_to_string(index_dir.join("satisfaction_survey.tsv"))?;
    assert_eq!(survey_content, "new survey");
    let metrics_content = fs::read_to_string(index_dir.join("styling_metrics.json"))?;
    assert_eq!(metrics_content, "new metrics");

    Ok(())
}
