//! Index creation tests.

use std::fs;
use std::path::Path;

use anyhow::Result;
use sprocket::system::v1::db::Database;
use sprocket::system::v1::db::SprocketCommand;
use sprocket::system::v1::db::SqliteDatabase;
use sprocket::system::v1::fs::OutputDirectory;
use sprocket::system::v1::fs::create_index_entries;
use sqlx::SqlitePool;
use tempfile::TempDir;
use uuid::Uuid;
use wdl::analysis::types::ArrayType;
use wdl::analysis::types::PrimitiveType;
use wdl::analysis::types::Type;
use wdl::engine::Array;
use wdl::engine::CompoundValue;
use wdl::engine::HostPath;
use wdl::engine::Outputs;
use wdl::engine::PrimitiveValue;
use wdl::engine::Value;

#[path = "index/rebuild.rs"]
mod rebuild;

/// Helper to create a file output value and write the file.
fn make_file_output(
    exec_dir: &Path,
    name: &str,
    path: &str,
    content: &str,
) -> Result<(String, Value)> {
    let file_path = exec_dir.join(path);
    fs::write(file_path, content)?;
    Ok((
        name.to_string(),
        Value::Primitive(PrimitiveValue::File(HostPath::new(path))),
    ))
}

/// Helper to create a directory output value and create the directory.
fn make_directory_output(exec_dir: &Path, name: &str, path: &str) -> Result<(String, Value)> {
    let dir_path = exec_dir.join(path);
    fs::create_dir_all(&dir_path)?;
    Ok((
        name.to_string(),
        Value::Primitive(PrimitiveValue::Directory(HostPath::new(path))),
    ))
}

#[sqlx::test]
async fn create_index_with_files(pool: SqlitePool) -> Result<()> {
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
        "test_task",
        "{}",
        "test-workflow",
    )
    .await?;

    let run_dir = output_dir.ensure_workflow_run("test-workflow")?;
    fs::write(run_dir.outputs_file(), "{}")?;

    let outputs: Outputs = [
        make_file_output(
            run_dir.root(),
            "satisfaction_survey",
            "satisfaction_survey.tsv",
            "test content",
        )?,
        make_file_output(
            run_dir.root(),
            "styling_metrics",
            "styling_metrics.json",
            "log content",
        )?,
    ]
    .into_iter()
    .collect();

    create_index_entries(&db, run_id, &run_dir, "yak", &outputs).await?;

    let index_dir = output_dir.index_dir("yak");
    assert!(index_dir.join("outputs.json").exists());
    assert!(index_dir.join("outputs.json").is_symlink());
    assert!(index_dir.join("satisfaction_survey.tsv").exists());
    assert!(index_dir.join("satisfaction_survey.tsv").is_symlink());
    assert!(index_dir.join("styling_metrics.json").exists());
    assert!(index_dir.join("styling_metrics.json").is_symlink());

    let content = fs::read_to_string(index_dir.join("satisfaction_survey.tsv"))?;
    assert_eq!(content, "test content");

    let entries = db.list_index_log_entries_by_run(run_id).await?;
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].run_uuid, run_id);
    assert_eq!(entries[0].link_path.as_str(), "./index/yak/outputs.json");
    assert_eq!(
        entries[0].target_path.as_str(),
        "./runs/test-workflow/outputs.json"
    );
    assert_eq!(entries[1].run_uuid, run_id);
    assert_eq!(
        entries[1].link_path.as_str(),
        "./index/yak/satisfaction_survey.tsv"
    );
    assert_eq!(
        entries[1].target_path.as_str(),
        "./runs/test-workflow/satisfaction_survey.tsv"
    );
    assert_eq!(entries[2].run_uuid, run_id);
    assert_eq!(
        entries[2].link_path.as_str(),
        "./index/yak/styling_metrics.json"
    );
    assert_eq!(
        entries[2].target_path.as_str(),
        "./runs/test-workflow/styling_metrics.json"
    );

    Ok(())
}

#[sqlx::test]
async fn create_index_with_directory(pool: SqlitePool) -> Result<()> {
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
        "test_task",
        "{}",
        "test-workflow",
    )
    .await?;

    let run_dir = output_dir.ensure_workflow_run("test-workflow")?;
    fs::write(run_dir.root().join("outputs.json"), "{}")?;

    let (name, value) = make_directory_output(run_dir.root(), "styled_yaks", "styled_yaks")?;
    let styled_yaks_dir = run_dir.root().join("styled_yaks");
    fs::write(styled_yaks_dir.join("file1.txt"), "content1")?;
    fs::write(styled_yaks_dir.join("file2.txt"), "content2")?;

    let outputs: Outputs = [(name, value)].into_iter().collect();

    create_index_entries(&db, run_id, &run_dir, "yak", &outputs).await?;

    let index_dir = output_dir.index_dir("yak");
    let yak_link = index_dir.join("styled_yaks");
    assert!(yak_link.exists());
    assert!(yak_link.is_symlink());
    assert!(yak_link.join("file1.txt").exists());
    assert!(yak_link.join("file2.txt").exists());

    let entries = db.list_index_log_entries_by_run(run_id).await?;
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].run_uuid, run_id);
    assert_eq!(entries[0].link_path.as_str(), "./index/yak/outputs.json");
    assert_eq!(
        entries[0].target_path.as_str(),
        "./runs/test-workflow/outputs.json"
    );
    assert_eq!(entries[1].run_uuid, run_id);
    assert_eq!(entries[1].link_path.as_str(), "./index/yak/styled_yaks");
    assert_eq!(
        entries[1].target_path.as_str(),
        "./runs/test-workflow/styled_yaks"
    );

    Ok(())
}

#[sqlx::test]
async fn create_index_with_array_of_files(pool: SqlitePool) -> Result<()> {
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
        "test_task",
        "{}",
        "test-workflow",
    )
    .await?;

    let run_dir = output_dir.ensure_workflow_run("test-workflow")?;
    fs::write(run_dir.root().join("outputs.json"), "{}")?;

    fs::write(run_dir.root().join("result1.txt"), "content1")?;
    fs::write(run_dir.root().join("result2.txt"), "content2")?;
    fs::write(run_dir.root().join("result3.txt"), "content3")?;

    let outputs: Outputs = [(
        "results".to_string(),
        Value::Compound(CompoundValue::Array(
            Array::new(
                ArrayType::new(Type::Primitive(PrimitiveType::File, false)),
                vec![
                    Value::Primitive(PrimitiveValue::File(HostPath::new("result1.txt"))),
                    Value::Primitive(PrimitiveValue::File(HostPath::new("result2.txt"))),
                    Value::Primitive(PrimitiveValue::File(HostPath::new("result3.txt"))),
                ],
            )
            .unwrap(),
        )),
    )]
    .into_iter()
    .collect();

    create_index_entries(&db, run_id, &run_dir, "yak", &outputs).await?;

    let index_dir = output_dir.index_dir("yak");
    assert!(index_dir.join("outputs.json").exists());
    assert!(index_dir.join("result1.txt").exists());
    assert!(index_dir.join("result1.txt").is_symlink());
    assert!(index_dir.join("result2.txt").exists());
    assert!(index_dir.join("result2.txt").is_symlink());
    assert!(index_dir.join("result3.txt").exists());
    assert!(index_dir.join("result3.txt").is_symlink());

    let content = fs::read_to_string(index_dir.join("result1.txt"))?;
    assert_eq!(content, "content1");

    let entries = db.list_index_log_entries_by_run(run_id).await?;
    assert_eq!(entries.len(), 4);
    assert_eq!(entries[0].link_path.as_str(), "./index/yak/outputs.json");
    assert_eq!(entries[1].link_path.as_str(), "./index/yak/result1.txt");
    assert_eq!(entries[2].link_path.as_str(), "./index/yak/result2.txt");
    assert_eq!(entries[3].link_path.as_str(), "./index/yak/result3.txt");

    Ok(())
}

#[sqlx::test]
async fn create_index_with_missing_files(pool: SqlitePool) -> Result<()> {
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
        "test_task",
        "{}",
        "test-workflow",
    )
    .await?;

    let run_dir = output_dir.ensure_workflow_run("test-workflow")?;
    fs::write(run_dir.root().join("outputs.json"), "{}")?;

    // Create outputs that reference files that don't exist (both output files
    // don't exist)
    let outputs: Outputs = [
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

    // This should fail because neither of the output files exist
    let result = create_index_entries(&db, run_id, &run_dir, "yak", &outputs).await;

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("failed to create one or more index entries")
    );

    Ok(())
}

#[sqlx::test]
async fn create_index_with_partial_db_failure(pool: SqlitePool) -> Result<()> {
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
        "test_task",
        "{}",
        "test-workflow",
    )
    .await?;

    let run_dir = output_dir.ensure_workflow_run("test-workflow")?;
    fs::write(run_dir.root().join("outputs.json"), "{}")?;

    // Create one file that exists and one that doesn't (`styling_metrics.json` is
    // missing)
    let outputs: Outputs = [
        make_file_output(
            run_dir.root(),
            "satisfaction_survey",
            "satisfaction_survey.tsv",
            "test content",
        )?,
        (
            "styling_metrics".to_string(),
            Value::Primitive(PrimitiveValue::File(HostPath::new("styling_metrics.json"))),
        ),
    ]
    .into_iter()
    .collect();

    // This should fail because `styling_metrics.json` is missing
    let result = create_index_entries(&db, run_id, &run_dir, "yak", &outputs).await;

    assert!(result.is_err());

    // Verify that the successful symlinks were created even though operation failed
    let index_dir = output_dir.index_dir("yak");
    assert!(index_dir.join("outputs.json").exists());
    assert!(index_dir.join("satisfaction_survey.tsv").exists());

    // And database entries were logged for the successful ones
    let entries = db.list_index_log_entries_by_run(run_id).await?;
    assert_eq!(entries.len(), 2); // `outputs.json` and `satisfaction_survey.tsv`

    Ok(())
}

#[sqlx::test]
async fn create_index_with_nested_directory_structure(pool: SqlitePool) -> Result<()> {
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
        "test_task",
        "{}",
        "test-workflow",
    )
    .await?;

    let run_dir = output_dir.ensure_workflow_run("test-workflow")?;
    fs::write(run_dir.root().join("outputs.json"), "{}")?;

    let nested_path = "data/results/final/output.txt";
    fs::create_dir_all(run_dir.root().join("data/results/final"))?;
    fs::write(run_dir.root().join(nested_path), "nested content")?;

    let outputs: Outputs = [(
        "nested_output".to_string(),
        Value::Primitive(PrimitiveValue::File(HostPath::new(nested_path))),
    )]
    .into_iter()
    .collect();

    create_index_entries(&db, run_id, &run_dir, "dataset/experiment1", &outputs).await?;

    let index_dir = output_dir.index_dir("dataset/experiment1");
    assert!(index_dir.exists());
    assert!(index_dir.join("outputs.json").exists());
    assert!(index_dir.join("outputs.json").is_symlink());
    assert!(index_dir.join("output.txt").exists());
    assert!(index_dir.join("output.txt").is_symlink());

    let content = fs::read_to_string(index_dir.join("output.txt"))?;
    assert_eq!(content, "nested content");

    let entries = db.list_index_log_entries_by_run(run_id).await?;
    assert_eq!(entries.len(), 2);
    assert!(entries[1].link_path.contains("dataset/experiment1"));
    assert!(entries[1].link_path.contains("output.txt"));

    Ok(())
}

#[sqlx::test]
async fn create_index_replaces_older_index(pool: SqlitePool) -> Result<()> {
    let temp = TempDir::new()?;
    let output_dir = OutputDirectory::new(temp.path());
    let db = SqliteDatabase::from_pool(pool).await?;

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Run, "test_user")
        .await?;

    // First workflow
    let run_id_1 = Uuid::new_v4();
    db.create_run(
        run_id_1,
        session_id,
        "test1",
        "file://test.wdl",
        "task_a",
        "{}",
        "test-workflow-1",
    )
    .await?;

    let run_dir_1 = output_dir.ensure_workflow_run("test-workflow-1")?;
    fs::write(run_dir_1.root().join("outputs.json"), "{}")?;
    fs::write(run_dir_1.root().join("result.txt"), "old result")?;

    let outputs_1: Outputs = [(
        "result".to_string(),
        Value::Primitive(PrimitiveValue::File(HostPath::new("result.txt"))),
    )]
    .into_iter()
    .collect();

    create_index_entries(&db, run_id_1, &run_dir_1, "experiment", &outputs_1).await?;

    let index_dir = output_dir.index_dir("experiment");
    let content = fs::read_to_string(index_dir.join("result.txt"))?;
    assert_eq!(content, "old result");

    // Sleep to ensure different timestamp.
    // SQLite `current_timestamp` has second precision.
    std::thread::sleep(std::time::Duration::from_millis(1100));

    // Second workflow - should replace the index
    let run_id_2 = Uuid::new_v4();
    db.create_run(
        run_id_2,
        session_id,
        "test2",
        "file://test.wdl",
        "task_b",
        "{}",
        "test-workflow-2",
    )
    .await?;

    let run_dir_2 = output_dir.ensure_workflow_run("test-workflow-2")?;
    fs::write(run_dir_2.root().join("outputs.json"), "{}")?;
    fs::write(run_dir_2.root().join("result.txt"), "new result")?;

    let outputs_2: Outputs = [(
        "result".to_string(),
        Value::Primitive(PrimitiveValue::File(HostPath::new("result.txt"))),
    )]
    .into_iter()
    .collect();

    create_index_entries(&db, run_id_2, &run_dir_2, "experiment", &outputs_2).await?;

    // Verify index now points to new workflow
    let content = fs::read_to_string(index_dir.join("result.txt"))?;
    assert_eq!(content, "new result");

    // Verify both workflows have entries in database
    let entries_1 = db.list_index_log_entries_by_run(run_id_1).await?;
    assert_eq!(entries_1.len(), 2);

    let entries_2 = db.list_index_log_entries_by_run(run_id_2).await?;
    assert_eq!(entries_2.len(), 2);

    // Verify latest entries point to second workflow
    let latest_entries = db.list_latest_index_entries().await?;
    let result_entry = latest_entries
        .iter()
        .find(|e| e.link_path.contains("result.txt"))
        .unwrap();
    assert_eq!(result_entry.run_uuid, run_id_2);

    Ok(())
}

#[sqlx::test]
async fn create_index_with_empty_outputs(pool: SqlitePool) -> Result<()> {
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
        "test_task",
        "{}",
        "test-workflow",
    )
    .await?;

    let run_dir = output_dir.ensure_workflow_run("test-workflow")?;
    fs::write(run_dir.root().join("outputs.json"), "{}")?;

    let outputs: Outputs = Outputs::default();

    create_index_entries(&db, run_id, &run_dir, "yak", &outputs).await?;

    let index_dir = output_dir.index_dir("yak");
    assert!(index_dir.exists());
    assert!(index_dir.join("outputs.json").exists());
    assert!(index_dir.join("outputs.json").is_symlink());

    let entries = db.list_index_log_entries_by_run(run_id).await?;
    assert_eq!(entries.len(), 1);
    assert!(entries[0].link_path.contains("outputs.json"));

    Ok(())
}

#[sqlx::test]
async fn create_index_with_special_characters_in_index_name(pool: SqlitePool) -> Result<()> {
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
        "test_task",
        "{}",
        "test-workflow",
    )
    .await?;

    let run_dir = output_dir.ensure_workflow_run("test-workflow")?;
    fs::write(run_dir.root().join("outputs.json"), "{}")?;
    fs::write(run_dir.root().join("data.txt"), "content")?;

    let outputs: Outputs = [(
        "data".to_string(),
        Value::Primitive(PrimitiveValue::File(HostPath::new("data.txt"))),
    )]
    .into_iter()
    .collect();

    let index_name = "my experiment_2024/batch 1_🎉";
    create_index_entries(&db, run_id, &run_dir, index_name, &outputs).await?;

    let index_dir = output_dir.index_dir(index_name);
    assert!(index_dir.exists());
    assert!(index_dir.join("outputs.json").exists());
    assert!(index_dir.join("data.txt").exists());
    assert!(index_dir.join("data.txt").is_symlink());

    let content = fs::read_to_string(index_dir.join("data.txt"))?;
    assert_eq!(content, "content");

    let entries = db.list_index_log_entries_by_run(run_id).await?;
    assert_eq!(entries.len(), 2);
    assert!(entries[1].link_path.contains(index_name));

    Ok(())
}

#[sqlx::test]
async fn create_index_with_symlink_collision(pool: SqlitePool) -> Result<()> {
    let temp = TempDir::new()?;
    let output_dir = OutputDirectory::new(temp.path());
    let db = SqliteDatabase::from_pool(pool).await?;

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Run, "test_user")
        .await?;

    // First workflow with result.txt
    let run_id_1 = Uuid::new_v4();
    db.create_run(
        run_id_1,
        session_id,
        "test1",
        "file://test.wdl",
        "task_a",
        "{}",
        "test-workflow-1",
    )
    .await?;

    let run_dir_1 = output_dir.ensure_workflow_run("test-workflow-1")?;
    fs::write(run_dir_1.root().join("outputs.json"), "{}")?;
    fs::write(run_dir_1.root().join("result.txt"), "first workflow result")?;

    let outputs_1: Outputs = [(
        "output".to_string(),
        Value::Primitive(PrimitiveValue::File(HostPath::new("result.txt"))),
    )]
    .into_iter()
    .collect();

    create_index_entries(&db, run_id_1, &run_dir_1, "experiment", &outputs_1).await?;

    let index_dir = output_dir.index_dir("experiment");
    let content = fs::read_to_string(index_dir.join("result.txt"))?;
    assert_eq!(content, "first workflow result");

    // Sleep to ensure different timestamp.
    // SQLite `current_timestamp` has second precision.
    std::thread::sleep(std::time::Duration::from_millis(1100));

    // Second workflow also with result.txt - should replace the symlink
    let run_id_2 = Uuid::new_v4();
    db.create_run(
        run_id_2,
        session_id,
        "test2",
        "file://test.wdl",
        "task_b",
        "{}",
        "test-workflow-2",
    )
    .await?;

    let run_dir_2 = output_dir.ensure_workflow_run("test-workflow-2")?;
    fs::write(run_dir_2.root().join("outputs.json"), "{}")?;
    fs::write(
        run_dir_2.root().join("result.txt"),
        "second workflow result",
    )?;

    let outputs_2: Outputs = [(
        "output".to_string(),
        Value::Primitive(PrimitiveValue::File(HostPath::new("result.txt"))),
    )]
    .into_iter()
    .collect();

    create_index_entries(&db, run_id_2, &run_dir_2, "experiment", &outputs_2).await?;

    // Verify symlink was replaced and now points to second workflow
    let content = fs::read_to_string(index_dir.join("result.txt"))?;
    assert_eq!(content, "second workflow result");

    // Verify only one result.txt symlink exists
    assert!(index_dir.join("result.txt").exists());
    assert!(index_dir.join("result.txt").is_symlink());

    Ok(())
}
