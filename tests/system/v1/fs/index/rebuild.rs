//! Rebuild index tests.

use std::fs;

use super::normalize_path;
use anyhow::Result;
use sprocket::system::v1::db::Database;
use sprocket::system::v1::db::SprocketCommand;
use sprocket::system::v1::db::SqliteDatabase;
use sprocket::system::v1::fs::OutputDirectory;
use sprocket::system::v1::fs::create_index_entries;
use sprocket::system::v1::fs::rebuild_index;
use sqlx::SqlitePool;
use tempfile::TempDir;
use uuid::Uuid;
use wdl::engine::HostPath;
use wdl::engine::Outputs;
use wdl::engine::PrimitiveValue;
use wdl::engine::Value;

#[sqlx::test]
async fn rebuild_index_full(pool: SqlitePool) -> Result<()> {
    let temp = TempDir::new()?;
    let output_dir = OutputDirectory::new(temp.path());
    let db = SqliteDatabase::from_pool(pool).await?;

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Run, "test_user")
        .await?;

    // First workflow execution
    let run_id1 = Uuid::new_v4();
    db.create_run(
        run_id1,
        session_id,
        "test",
        "file://test.wdl",
        Some("task_a"),
        "{}",
    )
    .await?;

    let run_dir1 = output_dir.ensure_workflow_run("test-workflow-run1")?;
    fs::write(run_dir1.root().join("outputs.json"), "{}")?;
    fs::write(
        run_dir1.root().join("satisfaction_survey.tsv"),
        "old survey",
    )?;
    fs::write(run_dir1.root().join("styling_metrics.json"), "old metrics")?;

    let outputs1: Outputs = [
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

    create_index_entries(&db, run_id1, &run_dir1, "yak", &outputs1).await?;

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
    let entries1 = db.list_index_log_entries_by_run(run_id1).await?;
    assert_eq!(entries1.len(), 3); // `outputs.json`, `satisfaction_survey.tsv`, `styling_metrics.json`

    // Sleep to ensure second workflow gets a different timestamp.
    // SQLite `current_timestamp` has second precision, so we need at least 1
    // second.
    std::thread::sleep(std::time::Duration::from_millis(1100));

    // Second workflow execution (rerun with newer data)
    let run_id2 = Uuid::new_v4();
    db.create_run(
        run_id2,
        session_id,
        "test",
        "file://test.wdl",
        Some("task_b"),
        "{}",
    )
    .await?;

    let run_dir2 = output_dir.ensure_workflow_run("test-workflow-run2")?;
    fs::write(run_dir2.root().join("outputs.json"), "{}")?;
    fs::write(
        run_dir2.root().join("satisfaction_survey.tsv"),
        "new survey",
    )?;
    fs::write(run_dir2.root().join("styling_metrics.json"), "new metrics")?;

    let outputs2: Outputs = [
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

    create_index_entries(&db, run_id2, &run_dir2, "yak", &outputs2).await?;

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
    let entries2 = db.list_index_log_entries_by_run(run_id2).await?;
    assert_eq!(entries2.len(), 3);

    // Verify we have only 3 latest entries (one per unique index path)
    let mut all_entries = db.list_latest_index_entries().await?;
    assert_eq!(all_entries.len(), 3);

    // Sort entries by index_path for deterministic assertions
    all_entries.sort_by(|a, b| a.link_path.cmp(&b.link_path));

    // Verify first entry: `outputs.json` from `run_id2`
    assert_eq!(all_entries[0].run_uuid, run_id2);
    assert_eq!(
        normalize_path(&all_entries[0].link_path),
        "./index/yak/outputs.json"
    );
    assert_eq!(
        normalize_path(&all_entries[0].target_path),
        "./runs/test-workflow-run2/outputs.json"
    );

    // Verify second entry: `satisfaction_survey.tsv` from `run_id2`
    assert_eq!(all_entries[1].run_uuid, run_id2);
    assert_eq!(
        normalize_path(&all_entries[1].link_path),
        "./index/yak/satisfaction_survey.tsv"
    );
    assert_eq!(
        normalize_path(&all_entries[1].target_path),
        "./runs/test-workflow-run2/satisfaction_survey.tsv"
    );

    // Verify third entry: `styling_metrics.json` from `run_id2`
    assert_eq!(all_entries[2].run_uuid, run_id2);
    assert_eq!(
        normalize_path(&all_entries[2].link_path),
        "./index/yak/styling_metrics.json"
    );
    assert_eq!(
        normalize_path(&all_entries[2].target_path),
        "./runs/test-workflow-run2/styling_metrics.json"
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

#[sqlx::test]
async fn rebuild_index_with_missing_targets(pool: SqlitePool) -> Result<()> {
    let temp = TempDir::new()?;
    let output_dir = OutputDirectory::new(temp.path());
    let db = SqliteDatabase::from_pool(pool).await?;

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Run, "test_user")
        .await?;

    let run_id = Uuid::new_v4();
    db.create_run(
        run_id,
        session_id,
        "test",
        "file://test.wdl",
        Some("test_task"),
        "{}",
    )
    .await?;

    let run_dir = output_dir.ensure_workflow_run("test-workflow")?;
    fs::write(run_dir.root().join("outputs.json"), "{}")?;
    fs::write(run_dir.root().join("file1.txt"), "content1")?;
    fs::write(run_dir.root().join("file2.txt"), "content2")?;

    let outputs: Outputs = [
        (
            "output1".to_string(),
            Value::Primitive(PrimitiveValue::File(HostPath::new("file1.txt"))),
        ),
        (
            "output2".to_string(),
            Value::Primitive(PrimitiveValue::File(HostPath::new("file2.txt"))),
        ),
    ]
    .into_iter()
    .collect();

    create_index_entries(&db, run_id, &run_dir, "yak", &outputs).await?;

    let index_dir = output_dir.index_dir("yak");
    assert!(index_dir.join("file1.txt").exists());
    assert!(index_dir.join("file2.txt").exists());

    // Delete one of the target files (but keep outputs.json)
    fs::remove_file(run_dir.root().join("file2.txt"))?;
    assert!(run_dir.root().join("outputs.json").exists());

    // Delete the index directory
    fs::remove_dir_all(output_dir.root().join("index"))?;
    assert!(!index_dir.exists());

    // Rebuild should succeed but skip the missing file
    rebuild_index(&db, &output_dir).await?;

    assert!(index_dir.exists());
    assert!(index_dir.join("outputs.json").exists());
    assert!(index_dir.join("outputs.json").is_symlink());
    assert!(index_dir.join("file1.txt").exists());
    assert!(index_dir.join("file1.txt").is_symlink());
    assert!(!index_dir.join("file2.txt").exists());

    let content = fs::read_to_string(index_dir.join("file1.txt"))?;
    assert_eq!(content, "content1");

    Ok(())
}
