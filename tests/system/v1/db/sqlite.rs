//! SQLite database tests.

use chrono::Utc;
use sprocket::system::v1::db::Database;
use sprocket::system::v1::db::RunStatus;
use sprocket::system::v1::db::SprocketCommand;
use sprocket::system::v1::db::SqliteDatabase;
use sqlx::SqlitePool;
use uuid::Uuid;

#[sqlx::test]
async fn create_and_get_session(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let id = Uuid::new_v4();
    let session = db
        .create_session(id, SprocketCommand::Run, "test_user")
        .await
        .unwrap();

    assert_eq!(session.uuid, id);
    assert_eq!(session.subcommand, SprocketCommand::Run);
    assert_eq!(session.created_by, String::from("test_user"));
    assert!(session.created_at <= Utc::now());

    let retrieved = db.get_session(id).await.unwrap();
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.uuid, id);
    assert_eq!(retrieved.subcommand, SprocketCommand::Run);
    assert_eq!(retrieved.created_by, String::from("test_user"));
    assert_eq!(retrieved.created_at, session.created_at);
}

#[sqlx::test]
async fn create_and_get_run(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Run, "test_user")
        .await
        .unwrap();

    let run_id = Uuid::new_v4();
    let run = db
        .create_run(
            run_id,
            session_id,
            "test_workflow",
            "/path/to/run.wdl",
            "{}",
            "/tmp/execution",
        )
        .await
        .unwrap();

    assert_eq!(run.uuid, run_id);
    assert_eq!(run.session_uuid, session_id);
    assert_eq!(run.name, "test_workflow");
    assert_eq!(run.source, "/path/to/run.wdl");
    assert_eq!(run.status, RunStatus::Queued);
    assert_eq!(run.inputs, "{}");
    assert_eq!(run.outputs, None);
    assert_eq!(run.error, None);
    assert_eq!(run.directory, "/tmp/execution");
    assert!(run.created_at <= Utc::now());
    assert_eq!(run.started_at, None);
    assert_eq!(run.completed_at, None);

    let retrieved = db.get_run(run_id).await.unwrap();
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.uuid, run_id);
    assert_eq!(retrieved.session_uuid, session_id);
    assert_eq!(retrieved.name, "test_workflow");
    assert_eq!(retrieved.source, "/path/to/run.wdl");
    assert_eq!(retrieved.status, RunStatus::Queued);
    assert_eq!(retrieved.inputs, "{}");
    assert_eq!(retrieved.outputs, None);
    assert_eq!(retrieved.error, None);
    assert_eq!(retrieved.directory, "/tmp/execution");
    assert_eq!(retrieved.created_at, run.created_at);
    assert_eq!(retrieved.started_at, None);
    assert_eq!(retrieved.completed_at, None);
}

#[sqlx::test]
async fn update_run_status(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Run, "test_user")
        .await
        .unwrap();

    let run_id = Uuid::new_v4();
    db.create_run(run_id, session_id, "test", "/test.wdl", "{}", "/tmp")
        .await
        .unwrap();

    let now = Utc::now();
    db.start_run(run_id, now).await.unwrap();

    let run = db.get_run(run_id).await.unwrap().unwrap();
    assert_eq!(run.uuid, run_id);
    assert_eq!(run.session_uuid, session_id);
    assert_eq!(run.status, RunStatus::Running);
    assert_eq!(run.started_at, Some(now));
    assert_eq!(run.completed_at, None);
    assert_eq!(run.outputs, None);
    assert_eq!(run.error, None);
}

#[sqlx::test]
async fn update_run_outputs(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Run, "test_user")
        .await
        .unwrap();

    let run_id = Uuid::new_v4();
    db.create_run(run_id, session_id, "test", "/test.wdl", "{}", "/tmp")
        .await
        .unwrap();

    db.update_run_outputs(run_id, r#"{"result": "success"}"#)
        .await
        .unwrap();

    let run = db.get_run(run_id).await.unwrap().unwrap();
    assert_eq!(run.uuid, run_id);
    assert_eq!(run.session_uuid, session_id);
    assert_eq!(run.status, RunStatus::Queued);
    assert_eq!(run.outputs, Some(String::from(r#"{"result": "success"}"#)));
    assert_eq!(run.error, None);
    assert_eq!(run.started_at, None);
    assert_eq!(run.completed_at, None);
}

#[sqlx::test]
async fn update_run_error(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Run, "test_user")
        .await
        .unwrap();

    let run_id = Uuid::new_v4();
    db.create_run(run_id, session_id, "test", "/test.wdl", "{}", "/tmp")
        .await
        .unwrap();

    db.update_run_error(run_id, "Something went wrong")
        .await
        .unwrap();

    let run = db.get_run(run_id).await.unwrap().unwrap();
    assert_eq!(run.uuid, run_id);
    assert_eq!(run.session_uuid, session_id);
    assert_eq!(run.status, RunStatus::Queued);
    assert_eq!(run.error, Some(String::from("Something went wrong")));
    assert_eq!(run.outputs, None);
    assert_eq!(run.started_at, None);
    assert_eq!(run.completed_at, None);
}

#[sqlx::test]
async fn list_runs_by_session(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Run, "test_user")
        .await
        .unwrap();

    let run_id_1 = Uuid::new_v4();
    let run_id_2 = Uuid::new_v4();

    db.create_run(
        run_id_1,
        session_id,
        "workflow1",
        "/test1.wdl",
        "{}",
        "/tmp/1",
    )
    .await
    .unwrap();

    db.create_run(
        run_id_2,
        session_id,
        "workflow2",
        "/test2.wdl",
        "{}",
        "/tmp/2",
    )
    .await
    .unwrap();

    let workflows = db.list_runs_by_session(session_id).await.unwrap();
    assert_eq!(workflows.len(), 2);
    assert_eq!(workflows[0].uuid, run_id_1);
    assert_eq!(workflows[1].uuid, run_id_2);
}

#[sqlx::test]
async fn create_and_list_index_log_entries(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Run, "test_user")
        .await
        .unwrap();

    let run_id = Uuid::new_v4();
    db.create_run(run_id, session_id, "test", "/test.wdl", "{}", "/tmp")
        .await
        .unwrap();

    let entry = db
        .create_index_log_entry(run_id, "/index/output.txt", "/tmp/output.txt")
        .await
        .unwrap();

    assert_eq!(entry.run_uuid, run_id);
    assert_eq!(entry.link_path, "/index/output.txt");
    assert_eq!(entry.target_path, "/tmp/output.txt");
    assert!(entry.created_at <= Utc::now());

    let entries = db.list_index_log_entries_by_run(run_id).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, entry.id);
    assert_eq!(entries[0].run_uuid, run_id);
    assert_eq!(entries[0].link_path, "/index/output.txt");
    assert_eq!(entries[0].target_path, "/tmp/output.txt");
    assert_eq!(entries[0].created_at, entry.created_at);
}

#[sqlx::test]
async fn get_nonexistent_session(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let id = Uuid::new_v4();
    let retrieved = db.get_session(id).await.unwrap();
    assert!(retrieved.is_none());
}

#[sqlx::test]
async fn get_nonexistent_run(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let id = Uuid::new_v4();
    let retrieved = db.get_run(id).await.unwrap();
    assert!(retrieved.is_none());
}

#[sqlx::test]
async fn list_runs_for_empty_session(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Run, "test_user")
        .await
        .unwrap();

    let workflows = db.list_runs_by_session(session_id).await.unwrap();
    assert_eq!(workflows.len(), 0);
}

#[sqlx::test]
async fn list_runs_for_nonexistent_session(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let session_id = Uuid::new_v4();
    let workflows = db.list_runs_by_session(session_id).await.unwrap();
    assert_eq!(workflows.len(), 0);
}

#[sqlx::test]
async fn list_index_entries_for_run_with_none(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Run, "test_user")
        .await
        .unwrap();

    let run_id = Uuid::new_v4();
    db.create_run(run_id, session_id, "test", "/test.wdl", "{}", "/tmp")
        .await
        .unwrap();

    let entries = db.list_index_log_entries_by_run(run_id).await.unwrap();
    assert_eq!(entries.len(), 0);
}

#[sqlx::test]
async fn list_index_entries_for_nonexistent_run(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let run_id = Uuid::new_v4();
    let entries = db.list_index_log_entries_by_run(run_id).await.unwrap();
    assert_eq!(entries.len(), 0);
}

#[sqlx::test]
async fn create_run_with_invalid_session_id(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let session_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();

    let result = db
        .create_run(run_id, session_id, "test", "/test.wdl", "{}", "/tmp")
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    match err {
        sprocket::system::v1::db::DatabaseError::Sqlx(sqlx::Error::RowNotFound) => {}
        _ => panic!("expected `RowNotFound` error, got: {:?}", err),
    }
}

#[sqlx::test]
async fn create_index_log_with_invalid_run_id(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let run_id = Uuid::new_v4();

    let result = db
        .create_index_log_entry(run_id, "/index/output.txt", "/tmp/output.txt")
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    match err {
        sprocket::system::v1::db::DatabaseError::Sqlx(sqlx::Error::RowNotFound) => {}
        _ => panic!("expected `RowNotFound` error, got: {:?}", err),
    }
}

#[sqlx::test]
async fn list_runs_ordered_by_created_at(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Run, "test_user")
        .await
        .unwrap();

    let run_id_1 = Uuid::new_v4();
    let run_id_2 = Uuid::new_v4();
    let run_id_3 = Uuid::new_v4();

    db.create_run(run_id_1, session_id, "first", "/test1.wdl", "{}", "/tmp/1")
        .await
        .unwrap();

    db.create_run(run_id_2, session_id, "second", "/test2.wdl", "{}", "/tmp/2")
        .await
        .unwrap();

    db.create_run(run_id_3, session_id, "third", "/test3.wdl", "{}", "/tmp/3")
        .await
        .unwrap();

    let workflows = db.list_runs_by_session(session_id).await.unwrap();

    assert_eq!(workflows.len(), 3);
    assert_eq!(workflows[0].uuid, run_id_1);
    assert_eq!(workflows[1].uuid, run_id_2);
    assert_eq!(workflows[2].uuid, run_id_3);
    assert!(workflows[0].created_at <= workflows[1].created_at);
    assert!(workflows[1].created_at <= workflows[2].created_at);
}

#[sqlx::test]
async fn list_index_entries_ordered_by_created_at(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Run, "test_user")
        .await
        .unwrap();

    let run_id = Uuid::new_v4();
    db.create_run(run_id, session_id, "test", "/test.wdl", "{}", "/tmp")
        .await
        .unwrap();

    let entry1 = db
        .create_index_log_entry(run_id, "/index/output1.txt", "/tmp/output1.txt")
        .await
        .unwrap();

    let entry2 = db
        .create_index_log_entry(run_id, "/index/output2.txt", "/tmp/output2.txt")
        .await
        .unwrap();

    let entry3 = db
        .create_index_log_entry(run_id, "/index/output3.txt", "/tmp/output3.txt")
        .await
        .unwrap();

    let entries = db.list_index_log_entries_by_run(run_id).await.unwrap();

    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].id, entry1.id);
    assert_eq!(entries[1].id, entry2.id);
    assert_eq!(entries[2].id, entry3.id);
    assert!(entries[0].created_at <= entries[1].created_at);
    assert!(entries[1].created_at <= entries[2].created_at);
}

#[sqlx::test]
async fn session_with_http_method(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let id = Uuid::new_v4();
    let session = db
        .create_session(id, SprocketCommand::Server, "api_user")
        .await
        .unwrap();

    assert_eq!(session.uuid, id);
    assert_eq!(session.subcommand, SprocketCommand::Server);
    assert_eq!(session.created_by, String::from("api_user"));

    let retrieved = db.get_session(id).await.unwrap().unwrap();
    assert_eq!(retrieved.subcommand, SprocketCommand::Server);
}

#[sqlx::test]
async fn run_status_transitions(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Run, "test_user")
        .await
        .unwrap();

    let run_id = Uuid::new_v4();
    let run = db
        .create_run(run_id, session_id, "test", "/test.wdl", "{}", "/tmp")
        .await
        .unwrap();
    assert_eq!(run.status, RunStatus::Queued);

    db.start_run(run_id, Utc::now()).await.unwrap();
    let run = db.get_run(run_id).await.unwrap().unwrap();
    assert_eq!(run.status, RunStatus::Running);

    db.complete_run(run_id, Utc::now()).await.unwrap();
    let run = db.get_run(run_id).await.unwrap().unwrap();
    assert_eq!(run.status, RunStatus::Completed);

    db.fail_run(run_id, "test error", Utc::now()).await.unwrap();
    let run = db.get_run(run_id).await.unwrap().unwrap();
    assert_eq!(run.status, RunStatus::Failed);

    db.cancel_run(run_id, Utc::now()).await.unwrap();
    let run = db.get_run(run_id).await.unwrap().unwrap();
    assert_eq!(run.status, RunStatus::Canceled);
}

#[sqlx::test]
async fn run_with_all_nullable_fields_null(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Run, "test_user")
        .await
        .unwrap();

    let run_id = Uuid::new_v4();
    let run = db
        .create_run(run_id, session_id, "test", "/test.wdl", "{}", "/tmp")
        .await
        .unwrap();

    assert_eq!(run.outputs, None);
    assert_eq!(run.error, None);
    assert_eq!(run.started_at, None);
    assert_eq!(run.completed_at, None);
}

#[sqlx::test]
async fn multiple_index_entries_for_same_run(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Run, "test_user")
        .await
        .unwrap();

    let run_id = Uuid::new_v4();
    db.create_run(run_id, session_id, "test", "/test.wdl", "{}", "/tmp")
        .await
        .unwrap();

    db.create_index_log_entry(run_id, "/index/output1.txt", "/tmp/output1.txt")
        .await
        .unwrap();

    db.create_index_log_entry(run_id, "/index/output2.txt", "/tmp/output2.txt")
        .await
        .unwrap();

    db.create_index_log_entry(run_id, "/index/output3.txt", "/tmp/output3.txt")
        .await
        .unwrap();

    let entries = db.list_index_log_entries_by_run(run_id).await.unwrap();
    assert_eq!(entries.len(), 3);
}

#[sqlx::test]
async fn complete_run_with_all_fields(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Server, "user123")
        .await
        .unwrap();

    let run_id = Uuid::new_v4();
    let run = db
        .create_run(
            run_id,
            session_id,
            "my_workflow",
            "/workflows/analysis.wdl",
            r#"{"input_file": "data.txt", "threshold": 0.5}"#,
            "/scratch/workflows/run_001",
        )
        .await
        .unwrap();

    assert_eq!(run.status, RunStatus::Queued);
    assert_eq!(run.outputs, None);
    assert_eq!(run.error, None);
    assert_eq!(run.started_at, None);
    assert_eq!(run.completed_at, None);

    let started = Utc::now();
    db.start_run(run_id, started).await.unwrap();

    let running_run = db.get_run(run_id).await.unwrap().unwrap();
    assert_eq!(running_run.status, RunStatus::Running);
    assert_eq!(running_run.started_at, Some(started));
    assert_eq!(running_run.completed_at, None);

    db.update_run_outputs(run_id, r#"{"result_file": "output.txt", "count": 42}"#)
        .await
        .unwrap();

    let completed = Utc::now();
    db.complete_run(run_id, completed).await.unwrap();

    let final_run = db.get_run(run_id).await.unwrap().unwrap();
    assert_eq!(final_run.uuid, run_id);
    assert_eq!(final_run.session_uuid, session_id);
    assert_eq!(final_run.name, "my_workflow");
    assert_eq!(final_run.source, "/workflows/analysis.wdl");
    assert_eq!(final_run.status, RunStatus::Completed);
    assert_eq!(
        final_run.inputs,
        r#"{"input_file": "data.txt", "threshold": 0.5}"#
    );
    assert_eq!(
        final_run.outputs,
        Some(String::from(
            r#"{"result_file": "output.txt", "count": 42}"#
        ))
    );
    assert_eq!(final_run.error, None);
    assert_eq!(final_run.directory, "/scratch/workflows/run_001");
    assert!(final_run.created_at <= started);
    assert_eq!(final_run.started_at, Some(started));
    assert_eq!(final_run.completed_at, Some(completed));
    assert!(started <= completed);
}

#[sqlx::test]
async fn update_run_index_directory(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Run, "test_user")
        .await
        .unwrap();

    let run_id = Uuid::new_v4();
    let run = db
        .create_run(
            run_id,
            session_id,
            "test",
            "/test.wdl",
            "{}",
            "./runs/test/20240115_120000000000",
        )
        .await
        .unwrap();

    assert_eq!(run.index_directory, None);

    let updated = db
        .update_run_index_directory(run_id, "./index/my_index")
        .await
        .unwrap();
    assert!(updated);

    let updated_run = db.get_run(run_id).await.unwrap().unwrap();
    assert_eq!(
        updated_run.index_directory,
        Some(String::from("./index/my_index"))
    );
}

#[sqlx::test]
async fn update_nonexistent_run_index_directory(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let run_id = Uuid::new_v4();

    let updated = db
        .update_run_index_directory(run_id, "./index/my_index")
        .await
        .unwrap();

    assert!(!updated);
}

#[sqlx::test]
async fn run_with_index_directory_null_by_default(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Run, "test_user")
        .await
        .unwrap();

    let run_id = Uuid::new_v4();
    let run = db
        .create_run(
            run_id,
            session_id,
            "test",
            "/test.wdl",
            "{}",
            "./runs/test/20240115_120000000000",
        )
        .await
        .unwrap();

    assert_eq!(run.index_directory, None);

    let retrieved = db.get_run(run_id).await.unwrap().unwrap();
    assert_eq!(retrieved.index_directory, None);
}

#[sqlx::test]
async fn list_latest_index_entries(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Run, "test_user")
        .await
        .unwrap();

    let run_id_1 = Uuid::new_v4();
    db.create_run(
        run_id_1,
        session_id,
        "test1",
        "/test1.wdl",
        "{}",
        "./runs/test1/20240115_120000000000",
    )
    .await
    .unwrap();

    let run_id_2 = Uuid::new_v4();
    db.create_run(
        run_id_2,
        session_id,
        "test2",
        "/test2.wdl",
        "{}",
        "./runs/test2/20240115_130000000000",
    )
    .await
    .unwrap();

    db.create_index_log_entry(
        run_id_1,
        "./index/yak/output.txt",
        "./runs/test1/output.txt",
    )
    .await
    .unwrap();

    // Sleep to ensure second entry gets a different timestamp.
    // SQLite `current_timestamp` has second precision.
    std::thread::sleep(std::time::Duration::from_millis(1100));

    db.create_index_log_entry(
        run_id_2,
        "./index/yak/output.txt",
        "./runs/test2/output.txt",
    )
    .await
    .unwrap();

    db.create_index_log_entry(
        run_id_2,
        "./index/yak/result.json",
        "./runs/test2/result.json",
    )
    .await
    .unwrap();

    let latest_entries = db.list_latest_index_entries().await.unwrap();

    assert_eq!(latest_entries.len(), 2);

    let output_entry = latest_entries
        .iter()
        .find(|e| e.link_path == "./index/yak/output.txt")
        .unwrap();
    assert_eq!(output_entry.run_uuid, run_id_2);
    assert_eq!(output_entry.target_path, "./runs/test2/output.txt");

    let result_entry = latest_entries
        .iter()
        .find(|e| e.link_path == "./index/yak/result.json")
        .unwrap();
    assert_eq!(result_entry.run_uuid, run_id_2);
    assert_eq!(result_entry.target_path, "./runs/test2/result.json");
}

#[sqlx::test]
async fn duplicate_session_id(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let id = Uuid::new_v4();
    db.create_session(id, SprocketCommand::Run, "test_user")
        .await
        .unwrap();

    let result = db
        .create_session(id, SprocketCommand::Run, "another_user")
        .await;

    assert!(matches!(
        result,
        Err(sprocket::system::v1::db::DatabaseError::Sqlx(sqlx::Error::Database(ref db_err)))
            if db_err.message() == "UNIQUE constraint failed: sessions.uuid"
    ));
}

#[sqlx::test]
async fn duplicate_run_id(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Run, "test_user")
        .await
        .unwrap();

    let run_id = Uuid::new_v4();
    db.create_run(
        run_id,
        session_id,
        "test",
        "/test.wdl",
        "{}",
        "./runs/test/20240115_120000000000",
    )
    .await
    .unwrap();

    let result = db
        .create_run(
            run_id,
            session_id,
            "test2",
            "/test2.wdl",
            "{}",
            "./runs/test2/20240115_130000000000",
        )
        .await;

    assert!(matches!(
        result,
        Err(sprocket::system::v1::db::DatabaseError::Sqlx(sqlx::Error::Database(ref db_err)))
            if db_err.message() == "UNIQUE constraint failed: runs.uuid"
    ));
}

#[sqlx::test]
async fn very_long_field_values(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let long_username = "a".repeat(10000);
    let long_workflow_name = "b".repeat(10000);
    let long_source = "c".repeat(10000);
    let long_directory = "d".repeat(10000);

    let session_id = Uuid::new_v4();
    let session = db
        .create_session(session_id, SprocketCommand::Run, &long_username)
        .await
        .unwrap();
    assert_eq!(session.created_by, long_username);

    let run_id = Uuid::new_v4();
    let run = db
        .create_run(
            run_id,
            session_id,
            &long_workflow_name,
            &long_source,
            "{}",
            &long_directory,
        )
        .await
        .unwrap();
    assert_eq!(run.name, long_workflow_name);
    assert_eq!(run.source, long_source);
    assert_eq!(run.directory, long_directory);

    let long_outputs = "x".repeat(10000);
    db.update_run_outputs(run_id, &long_outputs).await.unwrap();

    let long_error = "y".repeat(10000);
    db.update_run_error(run_id, &long_error).await.unwrap();

    let run = db.get_run(run_id).await.unwrap().unwrap();
    assert_eq!(run.outputs, Some(long_outputs));
    assert_eq!(run.error, Some(long_error));

    let long_link_path = "i".repeat(10000);
    let long_target_path = "t".repeat(10000);
    let entry = db
        .create_index_log_entry(run_id, &long_link_path, &long_target_path)
        .await
        .unwrap();
    assert_eq!(entry.link_path, long_link_path);
    assert_eq!(entry.target_path, long_target_path);
}

#[sqlx::test]
async fn list_sessions_pagination(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let mut ids = Vec::new();
    for i in 0..10 {
        let id = Uuid::new_v4();
        db.create_session(id, SprocketCommand::Run, &format!("user_{}", i))
            .await
            .unwrap();
        ids.push(id);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let all_sessions = db.list_sessions(None, None).await.unwrap();
    assert_eq!(all_sessions.len(), 10);

    let first_page = db.list_sessions(Some(5), Some(0)).await.unwrap();
    assert_eq!(first_page.len(), 5);

    let second_page = db.list_sessions(Some(5), Some(5)).await.unwrap();
    assert_eq!(second_page.len(), 5);

    let small_page = db.list_sessions(Some(3), Some(0)).await.unwrap();
    assert_eq!(small_page.len(), 3);

    let offset_page = db.list_sessions(Some(10), Some(8)).await.unwrap();
    assert_eq!(offset_page.len(), 2);
}

#[sqlx::test]
async fn list_runs_pagination(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Run, "test_user")
        .await
        .unwrap();

    for i in 0..10 {
        let run_id = Uuid::new_v4();
        db.create_run(
            run_id,
            session_id,
            &format!("workflow_{}", i),
            "/test.wdl",
            "{}",
            &format!("./runs/workflow_{}/20240115_120000000000", i),
        )
        .await
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let all_runs = db.list_runs(None, None, None).await.unwrap();
    assert_eq!(all_runs.len(), 10);

    let first_page = db.list_runs(None, Some(5), Some(0)).await.unwrap();
    assert_eq!(first_page.len(), 5);

    let second_page = db.list_runs(None, Some(5), Some(5)).await.unwrap();
    assert_eq!(second_page.len(), 5);

    let small_page = db.list_runs(None, Some(3), Some(0)).await.unwrap();
    assert_eq!(small_page.len(), 3);

    let offset_page = db.list_runs(None, Some(10), Some(8)).await.unwrap();
    assert_eq!(offset_page.len(), 2);
}

#[sqlx::test]
async fn list_runs_filtered_by_status(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Run, "test_user")
        .await
        .unwrap();

    let queued_id = Uuid::new_v4();
    db.create_run(
        queued_id,
        session_id,
        "queued",
        "/test.wdl",
        "{}",
        "./runs/queued/20240115_120000000000",
    )
    .await
    .unwrap();

    let running_id = Uuid::new_v4();
    db.create_run(
        running_id,
        session_id,
        "running",
        "/test.wdl",
        "{}",
        "./runs/running/20240115_120000000000",
    )
    .await
    .unwrap();
    db.start_run(running_id, Utc::now()).await.unwrap();

    let completed_id = Uuid::new_v4();
    db.create_run(
        completed_id,
        session_id,
        "completed",
        "/test.wdl",
        "{}",
        "./runs/completed/20240115_120000000000",
    )
    .await
    .unwrap();
    db.complete_run(completed_id, Utc::now()).await.unwrap();

    let failed_id = Uuid::new_v4();
    db.create_run(
        failed_id,
        session_id,
        "failed",
        "/test.wdl",
        "{}",
        "./runs/failed/20240115_120000000000",
    )
    .await
    .unwrap();
    db.fail_run(failed_id, "test error", Utc::now())
        .await
        .unwrap();

    let cancelled_id = Uuid::new_v4();
    db.create_run(
        cancelled_id,
        session_id,
        "cancelled",
        "/test.wdl",
        "{}",
        "./runs/cancelled/20240115_120000000000",
    )
    .await
    .unwrap();
    db.cancel_run(cancelled_id, Utc::now()).await.unwrap();

    let all_runs = db.list_runs(None, None, None).await.unwrap();
    assert_eq!(all_runs.len(), 5);

    let queued = db
        .list_runs(Some(RunStatus::Queued), None, None)
        .await
        .unwrap();
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].uuid, queued_id);

    let running = db
        .list_runs(Some(RunStatus::Running), None, None)
        .await
        .unwrap();
    assert_eq!(running.len(), 1);
    assert_eq!(running[0].uuid, running_id);

    let completed = db
        .list_runs(Some(RunStatus::Completed), None, None)
        .await
        .unwrap();
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].uuid, completed_id);

    let failed = db
        .list_runs(Some(RunStatus::Failed), None, None)
        .await
        .unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].uuid, failed_id);

    let cancelled = db
        .list_runs(Some(RunStatus::Canceled), None, None)
        .await
        .unwrap();
    assert_eq!(cancelled.len(), 1);
    assert_eq!(cancelled[0].uuid, cancelled_id);
}

#[sqlx::test]
async fn timestamp_ordering_and_accuracy(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let session_id = Uuid::new_v4();
    let session = db
        .create_session(session_id, SprocketCommand::Run, "test_user")
        .await
        .unwrap();

    assert!(session.created_at <= Utc::now());

    let run_id = Uuid::new_v4();
    let run = db
        .create_run(
            run_id,
            session_id,
            "test",
            "/test.wdl",
            "{}",
            "./runs/test/20240115_120000000000",
        )
        .await
        .unwrap();

    assert!(run.created_at <= Utc::now());
    assert!(run.created_at >= session.created_at);
    assert_eq!(run.started_at, None);
    assert_eq!(run.completed_at, None);

    // Sleep to ensure `started_at` has a later timestamp than `created_at`.
    std::thread::sleep(std::time::Duration::from_millis(100));

    let started_time = Utc::now();
    db.start_run(run_id, started_time).await.unwrap();

    let run = db.get_run(run_id).await.unwrap().unwrap();
    assert_eq!(run.status, RunStatus::Running);
    assert_eq!(run.started_at, Some(started_time));
    assert!(run.started_at.unwrap() >= run.created_at);
    assert_eq!(run.completed_at, None);

    // Sleep to ensure `completed_at` has a later timestamp than `started_at`.
    std::thread::sleep(std::time::Duration::from_millis(100));

    let completed_time = Utc::now();
    db.complete_run(run_id, completed_time).await.unwrap();

    let run = db.get_run(run_id).await.unwrap().unwrap();
    assert_eq!(run.status, RunStatus::Completed);
    assert_eq!(run.started_at, Some(started_time));
    assert_eq!(run.completed_at, Some(completed_time));
    assert!(completed_time >= started_time);
    assert!(completed_time >= run.created_at);

    let index_entry = db
        .create_index_log_entry(run_id, "./index/output.txt", "./runs/test/output.txt")
        .await
        .unwrap();

    assert!(index_entry.created_at <= Utc::now());
}

#[sqlx::test]
async fn run_outputs_with_special_characters(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Run, "test_user")
        .await
        .unwrap();

    let run_id = Uuid::new_v4();
    db.create_run(
        run_id,
        session_id,
        "test",
        "/test.wdl",
        "{}",
        "./runs/test/20240115_120000000000",
    )
    .await
    .unwrap();

    let outputs_with_special_chars = r#"{"file": "path/with\"quotes\".txt", "message": "line1\nline2\ttab", "backslash": "C:\\Windows\\Path", "unicode": "emoji: 🎉", "json": "{\"nested\": true}"}"#;
    db.update_run_outputs(run_id, outputs_with_special_chars)
        .await
        .unwrap();

    let error_with_special_chars = "Error: failed to parse\nLine 1: unexpected token \"{\"\nPath: \
                                    C:\\workflows\\test.wdl\nStack trace:\n\tat parse() [line \
                                    42]\n\tat main() [line 10]";
    db.update_run_error(run_id, error_with_special_chars)
        .await
        .unwrap();

    let run = db.get_run(run_id).await.unwrap().unwrap();
    assert_eq!(run.outputs, Some(String::from(outputs_with_special_chars)));
    assert_eq!(run.error, Some(String::from(error_with_special_chars)));
}

#[sqlx::test]
async fn link_path_with_special_characters(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Run, "test_user")
        .await
        .unwrap();

    let run_id = Uuid::new_v4();
    db.create_run(
        run_id,
        session_id,
        "test",
        "/test.wdl",
        "{}",
        "./runs/test/20240115_120000000000",
    )
    .await
    .unwrap();

    let link_path_with_spaces = "./index/my index/file with spaces.txt";
    let target_path_with_unicode = "./runs/test/output_🎉.txt";
    let entry1 = db
        .create_index_log_entry(run_id, link_path_with_spaces, target_path_with_unicode)
        .await
        .unwrap();
    assert_eq!(entry1.link_path, link_path_with_spaces);
    assert_eq!(entry1.target_path, target_path_with_unicode);

    let link_path_with_special = r#"./index/file"with"quotes.txt"#;
    let target_path_with_backslash = r#"./runs/test/C:\Windows\style\path.txt"#;
    let entry2 = db
        .create_index_log_entry(run_id, link_path_with_special, target_path_with_backslash)
        .await
        .unwrap();
    assert_eq!(entry2.link_path, link_path_with_special);
    assert_eq!(entry2.target_path, target_path_with_backslash);

    let entries = db.list_index_log_entries_by_run(run_id).await.unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].link_path, link_path_with_spaces);
    assert_eq!(entries[0].target_path, target_path_with_unicode);
    assert_eq!(entries[1].link_path, link_path_with_special);
    assert_eq!(entries[1].target_path, target_path_with_backslash);
}

#[sqlx::test]
async fn pagination_with_zero_limit(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Run, "test_user")
        .await
        .unwrap();

    for i in 0..5 {
        let run_id = Uuid::new_v4();
        db.create_run(
            run_id,
            session_id,
            &format!("workflow_{}", i),
            "/test.wdl",
            "{}",
            &format!("./runs/workflow_{}/20240115_120000000000", i),
        )
        .await
        .unwrap();
    }

    let workflows = db.list_runs(None, Some(0), Some(0)).await.unwrap();
    assert_eq!(workflows.len(), 0);

    let sessions = db.list_sessions(Some(0), Some(0)).await.unwrap();
    assert_eq!(sessions.len(), 0);
}

#[sqlx::test]
async fn pagination_with_large_offset(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Run, "test_user")
        .await
        .unwrap();

    for i in 0..5 {
        let run_id = Uuid::new_v4();
        db.create_run(
            run_id,
            session_id,
            &format!("workflow_{}", i),
            "/test.wdl",
            "{}",
            &format!("./runs/workflow_{}/20240115_120000000000", i),
        )
        .await
        .unwrap();
    }

    let workflows = db.list_runs(None, Some(10), Some(100)).await.unwrap();
    assert_eq!(workflows.len(), 0);

    let sessions = db.list_sessions(Some(10), Some(100)).await.unwrap();
    assert_eq!(sessions.len(), 0);
}
