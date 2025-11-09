//! Index creation tests.

use std::fs;
use std::path::Path;

use anyhow::Result;
use indexmap::IndexMap;
use sprocket::OutputDirectory;
use sprocket::database::Database;
use sprocket::database::InvocationMethod;
use sprocket::database::SqliteDatabase;
use sprocket::provenance::create_index_entries;
use sqlx::SqlitePool;
use tempfile::TempDir;
use uuid::Uuid;
use wdl::analysis::types::ArrayType;
use wdl::analysis::types::CompoundType;
use wdl::analysis::types::PrimitiveType;
use wdl::analysis::types::Type;
use wdl::engine::Array;
use wdl::engine::CompoundValue;
use wdl::engine::HostPath;
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

    let invocation_id = Uuid::new_v4();
    db.create_invocation(invocation_id, InvocationMethod::Cli, None)
        .await?;

    let workflow_id = Uuid::new_v4();
    db.create_workflow(
        workflow_id,
        invocation_id,
        String::from("test"),
        String::from("file://test.wdl"),
        String::from("{}"),
        String::from("test-workflow"),
    )
    .await?;

    let exec_dir = output_dir.ensure_workflow_run("test-workflow")?;
    fs::write(exec_dir.join("outputs.json"), "{}")?;

    let outputs: IndexMap<String, Value> = [
        make_file_output(
            &exec_dir,
            "satisfaction_survey",
            "satisfaction_survey.tsv",
            "test content",
        )?,
        make_file_output(
            &exec_dir,
            "styling_metrics",
            "styling_metrics.json",
            "log content",
        )?,
    ]
    .into_iter()
    .collect();

    create_index_entries(
        &db,
        workflow_id,
        &output_dir,
        "test-workflow",
        "yak",
        &outputs,
    )
    .await?;

    let index_dir = output_dir.index_dir("yak");
    assert!(index_dir.join("outputs.json").exists());
    assert!(index_dir.join("outputs.json").is_symlink());
    assert!(index_dir.join("satisfaction_survey.tsv").exists());
    assert!(index_dir.join("satisfaction_survey.tsv").is_symlink());
    assert!(index_dir.join("styling_metrics.json").exists());
    assert!(index_dir.join("styling_metrics.json").is_symlink());

    let content = fs::read_to_string(index_dir.join("satisfaction_survey.tsv"))?;
    assert_eq!(content, "test content");

    let entries = db.list_index_log_entries_by_workflow(workflow_id).await?;
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].workflow_id, workflow_id);
    assert_eq!(
        entries[0].index_path.to_str().unwrap(),
        "index/yak/outputs.json"
    );
    assert_eq!(
        entries[0].target_path.to_str().unwrap(),
        "runs/test-workflow/outputs.json"
    );
    assert_eq!(entries[1].workflow_id, workflow_id);
    assert_eq!(
        entries[1].index_path.to_str().unwrap(),
        "index/yak/satisfaction_survey.tsv"
    );
    assert_eq!(
        entries[1].target_path.to_str().unwrap(),
        "runs/test-workflow/satisfaction_survey.tsv"
    );
    assert_eq!(entries[2].workflow_id, workflow_id);
    assert_eq!(
        entries[2].index_path.to_str().unwrap(),
        "index/yak/styling_metrics.json"
    );
    assert_eq!(
        entries[2].target_path.to_str().unwrap(),
        "runs/test-workflow/styling_metrics.json"
    );

    Ok(())
}

#[sqlx::test]
async fn create_index_with_directory(pool: SqlitePool) -> Result<()> {
    let temp = TempDir::new()?;
    let output_dir = OutputDirectory::new(temp.path());
    let db = SqliteDatabase::from_pool(pool).await?;

    let invocation_id = Uuid::new_v4();
    db.create_invocation(invocation_id, InvocationMethod::Cli, None)
        .await?;

    let workflow_id = Uuid::new_v4();
    db.create_workflow(
        workflow_id,
        invocation_id,
        String::from("test"),
        String::from("file://test.wdl"),
        String::from("{}"),
        String::from("test-workflow"),
    )
    .await?;

    let exec_dir = output_dir.ensure_workflow_run("test-workflow")?;
    fs::write(exec_dir.join("outputs.json"), "{}")?;

    let (name, value) = make_directory_output(&exec_dir, "styled_yaks", "styled_yaks")?;
    let styled_yaks_dir = exec_dir.join("styled_yaks");
    fs::write(styled_yaks_dir.join("file1.txt"), "content1")?;
    fs::write(styled_yaks_dir.join("file2.txt"), "content2")?;

    let outputs: IndexMap<String, Value> = [(name, value)].into_iter().collect();

    create_index_entries(
        &db,
        workflow_id,
        &output_dir,
        "test-workflow",
        "yak",
        &outputs,
    )
    .await?;

    let index_dir = output_dir.index_dir("yak");
    let yak_link = index_dir.join("styled_yaks");
    assert!(yak_link.exists());
    assert!(yak_link.is_symlink());
    assert!(yak_link.join("file1.txt").exists());
    assert!(yak_link.join("file2.txt").exists());

    let entries = db.list_index_log_entries_by_workflow(workflow_id).await?;
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].workflow_id, workflow_id);
    assert_eq!(
        entries[0].index_path.to_str().unwrap(),
        "index/yak/outputs.json"
    );
    assert_eq!(
        entries[0].target_path.to_str().unwrap(),
        "runs/test-workflow/outputs.json"
    );
    assert_eq!(entries[1].workflow_id, workflow_id);
    assert_eq!(
        entries[1].index_path.to_str().unwrap(),
        "index/yak/styled_yaks"
    );
    assert_eq!(
        entries[1].target_path.to_str().unwrap(),
        "runs/test-workflow/styled_yaks"
    );

    Ok(())
}

#[sqlx::test]
async fn create_index_with_array_of_files(pool: SqlitePool) -> Result<()> {
    let temp = TempDir::new()?;
    let output_dir = OutputDirectory::new(temp.path());
    let db = SqliteDatabase::from_pool(pool).await?;

    let invocation_id = Uuid::new_v4();
    db.create_invocation(invocation_id, InvocationMethod::Cli, None)
        .await?;

    let workflow_id = Uuid::new_v4();
    db.create_workflow(
        workflow_id,
        invocation_id,
        String::from("test"),
        String::from("file://test.wdl"),
        String::from("{}"),
        String::from("test-workflow"),
    )
    .await?;

    let exec_dir = output_dir.ensure_workflow_run("test-workflow")?;
    fs::write(exec_dir.join("outputs.json"), "{}")?;

    fs::write(exec_dir.join("result1.txt"), "content1")?;
    fs::write(exec_dir.join("result2.txt"), "content2")?;
    fs::write(exec_dir.join("result3.txt"), "content3")?;

    let outputs: IndexMap<String, Value> = [(
        "results".to_string(),
        Value::Compound(CompoundValue::Array(
            Array::new(
                None,
                Type::Compound(
                    CompoundType::Array(ArrayType::new(Type::Primitive(
                        PrimitiveType::File,
                        false,
                    ))),
                    false,
                ),
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

    create_index_entries(
        &db,
        workflow_id,
        &output_dir,
        "test-workflow",
        "yak",
        &outputs,
    )
    .await?;

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

    let entries = db.list_index_log_entries_by_workflow(workflow_id).await?;
    assert_eq!(entries.len(), 4);
    assert_eq!(
        entries[0].index_path.to_str().unwrap(),
        "index/yak/outputs.json"
    );
    assert_eq!(
        entries[1].index_path.to_str().unwrap(),
        "index/yak/result1.txt"
    );
    assert_eq!(
        entries[2].index_path.to_str().unwrap(),
        "index/yak/result2.txt"
    );
    assert_eq!(
        entries[3].index_path.to_str().unwrap(),
        "index/yak/result3.txt"
    );

    Ok(())
}

#[sqlx::test]
async fn create_index_with_missing_files(pool: SqlitePool) -> Result<()> {
    let temp = TempDir::new()?;
    let output_dir = OutputDirectory::new(temp.path());
    let db = SqliteDatabase::from_pool(pool).await?;

    let invocation_id = Uuid::new_v4();
    db.create_invocation(invocation_id, InvocationMethod::Cli, None)
        .await?;

    let workflow_id = Uuid::new_v4();
    db.create_workflow(
        workflow_id,
        invocation_id,
        String::from("test"),
        String::from("file://test.wdl"),
        String::from("{}"),
        String::from("test-workflow"),
    )
    .await?;

    let exec_dir = output_dir.ensure_workflow_run("test-workflow")?;
    fs::write(exec_dir.join("outputs.json"), "{}")?;

    // Create outputs that reference files that don't exist (both output files
    // don't exist)
    let outputs: IndexMap<String, Value> = [
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
    let result = create_index_entries(
        &db,
        workflow_id,
        &output_dir,
        "test-workflow",
        "yak",
        &outputs,
    )
    .await;

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

    let invocation_id = Uuid::new_v4();
    db.create_invocation(invocation_id, InvocationMethod::Cli, None)
        .await?;

    let workflow_id = Uuid::new_v4();
    db.create_workflow(
        workflow_id,
        invocation_id,
        String::from("test"),
        String::from("file://test.wdl"),
        String::from("{}"),
        String::from("test-workflow"),
    )
    .await?;

    let exec_dir = output_dir.ensure_workflow_run("test-workflow")?;
    fs::write(exec_dir.join("outputs.json"), "{}")?;

    // Create one file that exists and one that doesn't (`styling_metrics.json` is
    // missing)
    let outputs: IndexMap<String, Value> = [
        make_file_output(
            &exec_dir,
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
    let result = create_index_entries(
        &db,
        workflow_id,
        &output_dir,
        "test-workflow",
        "yak",
        &outputs,
    )
    .await;

    assert!(result.is_err());

    // Verify that the successful symlinks were created even though operation failed
    let index_dir = output_dir.index_dir("yak");
    assert!(index_dir.join("outputs.json").exists());
    assert!(index_dir.join("satisfaction_survey.tsv").exists());

    // And database entries were logged for the successful ones
    let entries = db.list_index_log_entries_by_workflow(workflow_id).await?;
    assert_eq!(entries.len(), 2); // `outputs.json` and `satisfaction_survey.tsv`

    Ok(())
}
