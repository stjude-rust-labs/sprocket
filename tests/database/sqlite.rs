//! SQLite database tests.

use std::path::Path;

use chrono::Utc;
use sprocket::database::Database;
use sprocket::database::InvocationMethod;
use sprocket::database::SqliteDatabase;
use sprocket::database::WorkflowStatus;
use sqlx::SqlitePool;
use uuid::Uuid;

#[sqlx::test]
async fn create_and_get_invocation(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let id = Uuid::new_v4();
    let invocation = db
        .create_invocation(id, InvocationMethod::Cli, Some(String::from("test_user")))
        .await
        .unwrap();

    assert_eq!(invocation.id, id);
    assert_eq!(invocation.method, InvocationMethod::Cli);
    assert_eq!(invocation.created_by, Some(String::from("test_user")));
    assert!(invocation.created_at <= Utc::now());

    let retrieved = db.get_invocation(id).await.unwrap();
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.id, id);
    assert_eq!(retrieved.method, InvocationMethod::Cli);
    assert_eq!(retrieved.created_by, Some(String::from("test_user")));
    assert_eq!(retrieved.created_at, invocation.created_at);
}

#[sqlx::test]
async fn create_and_get_workflow(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let invocation_id = Uuid::new_v4();
    db.create_invocation(invocation_id, InvocationMethod::Cli, None)
        .await
        .unwrap();

    let workflow_id = Uuid::new_v4();
    let workflow = db
        .create_workflow(
            workflow_id,
            invocation_id,
            String::from("test_workflow"),
            String::from("/path/to/workflow.wdl"),
            String::from("{}"),
            String::from("/tmp/execution"),
        )
        .await
        .unwrap();

    assert_eq!(workflow.id, workflow_id);
    assert_eq!(workflow.invocation_id, invocation_id);
    assert_eq!(workflow.name, "test_workflow");
    assert_eq!(workflow.source, "/path/to/workflow.wdl");
    assert_eq!(workflow.status, WorkflowStatus::Pending);
    assert_eq!(workflow.inputs, "{}");
    assert_eq!(workflow.outputs, None);
    assert_eq!(workflow.error, None);
    assert_eq!(workflow.execution_dir, "/tmp/execution");
    assert!(workflow.created_at <= Utc::now());
    assert_eq!(workflow.started_at, None);
    assert_eq!(workflow.completed_at, None);

    let retrieved = db.get_workflow(workflow_id).await.unwrap();
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.id, workflow_id);
    assert_eq!(retrieved.invocation_id, invocation_id);
    assert_eq!(retrieved.name, "test_workflow");
    assert_eq!(retrieved.source, "/path/to/workflow.wdl");
    assert_eq!(retrieved.status, WorkflowStatus::Pending);
    assert_eq!(retrieved.inputs, "{}");
    assert_eq!(retrieved.outputs, None);
    assert_eq!(retrieved.error, None);
    assert_eq!(retrieved.execution_dir, "/tmp/execution");
    assert_eq!(retrieved.created_at, workflow.created_at);
    assert_eq!(retrieved.started_at, None);
    assert_eq!(retrieved.completed_at, None);
}

#[sqlx::test]
async fn update_workflow_status(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let invocation_id = Uuid::new_v4();
    db.create_invocation(invocation_id, InvocationMethod::Cli, None)
        .await
        .unwrap();

    let workflow_id = Uuid::new_v4();
    db.create_workflow(
        workflow_id,
        invocation_id,
        String::from("test"),
        String::from("/test.wdl"),
        String::from("{}"),
        String::from("/tmp"),
    )
    .await
    .unwrap();

    let now = Utc::now();
    db.update_workflow_status(workflow_id, WorkflowStatus::Running, Some(now), None)
        .await
        .unwrap();

    let workflow = db.get_workflow(workflow_id).await.unwrap().unwrap();
    assert_eq!(workflow.id, workflow_id);
    assert_eq!(workflow.invocation_id, invocation_id);
    assert_eq!(workflow.status, WorkflowStatus::Running);
    assert!(workflow.started_at.is_some());
    assert_eq!(workflow.completed_at, None);
    assert_eq!(workflow.outputs, None);
    assert_eq!(workflow.error, None);
}

#[sqlx::test]
async fn update_workflow_outputs(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let invocation_id = Uuid::new_v4();
    db.create_invocation(invocation_id, InvocationMethod::Cli, None)
        .await
        .unwrap();

    let workflow_id = Uuid::new_v4();
    db.create_workflow(
        workflow_id,
        invocation_id,
        String::from("test"),
        String::from("/test.wdl"),
        String::from("{}"),
        String::from("/tmp"),
    )
    .await
    .unwrap();

    db.update_workflow_outputs(workflow_id, String::from(r#"{"result": "success"}"#))
        .await
        .unwrap();

    let workflow = db.get_workflow(workflow_id).await.unwrap().unwrap();
    assert_eq!(workflow.id, workflow_id);
    assert_eq!(workflow.invocation_id, invocation_id);
    assert_eq!(workflow.status, WorkflowStatus::Pending);
    assert_eq!(
        workflow.outputs,
        Some(String::from(r#"{"result": "success"}"#))
    );
    assert_eq!(workflow.error, None);
    assert_eq!(workflow.started_at, None);
    assert_eq!(workflow.completed_at, None);
}

#[sqlx::test]
async fn update_workflow_error(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let invocation_id = Uuid::new_v4();
    db.create_invocation(invocation_id, InvocationMethod::Cli, None)
        .await
        .unwrap();

    let workflow_id = Uuid::new_v4();
    db.create_workflow(
        workflow_id,
        invocation_id,
        String::from("test"),
        String::from("/test.wdl"),
        String::from("{}"),
        String::from("/tmp"),
    )
    .await
    .unwrap();

    db.update_workflow_error(workflow_id, String::from("Something went wrong"))
        .await
        .unwrap();

    let workflow = db.get_workflow(workflow_id).await.unwrap().unwrap();
    assert_eq!(workflow.id, workflow_id);
    assert_eq!(workflow.invocation_id, invocation_id);
    assert_eq!(workflow.status, WorkflowStatus::Pending);
    assert_eq!(workflow.error, Some(String::from("Something went wrong")));
    assert_eq!(workflow.outputs, None);
    assert_eq!(workflow.started_at, None);
    assert_eq!(workflow.completed_at, None);
}

#[sqlx::test]
async fn list_workflows_by_invocation(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let invocation_id = Uuid::new_v4();
    db.create_invocation(invocation_id, InvocationMethod::Cli, None)
        .await
        .unwrap();

    let workflow_id_1 = Uuid::new_v4();
    let workflow_id_2 = Uuid::new_v4();

    db.create_workflow(
        workflow_id_1,
        invocation_id,
        String::from("workflow1"),
        String::from("/test1.wdl"),
        String::from("{}"),
        String::from("/tmp/1"),
    )
    .await
    .unwrap();

    db.create_workflow(
        workflow_id_2,
        invocation_id,
        String::from("workflow2"),
        String::from("/test2.wdl"),
        String::from("{}"),
        String::from("/tmp/2"),
    )
    .await
    .unwrap();

    let workflows = db
        .list_workflows_by_invocation(invocation_id)
        .await
        .unwrap();
    assert_eq!(workflows.len(), 2);
    assert_eq!(workflows[0].id, workflow_id_1);
    assert_eq!(workflows[1].id, workflow_id_2);
}

#[sqlx::test]
async fn create_and_list_index_log_entries(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let invocation_id = Uuid::new_v4();
    db.create_invocation(invocation_id, InvocationMethod::Cli, None)
        .await
        .unwrap();

    let workflow_id = Uuid::new_v4();
    db.create_workflow(
        workflow_id,
        invocation_id,
        String::from("test"),
        String::from("/test.wdl"),
        String::from("{}"),
        String::from("/tmp"),
    )
    .await
    .unwrap();

    let entry_id = Uuid::new_v4();
    let entry = db
        .create_index_log_entry(
            entry_id,
            workflow_id,
            String::from("/index/output.txt"),
            String::from("/tmp/output.txt"),
        )
        .await
        .unwrap();

    assert_eq!(entry.id, entry_id);
    assert_eq!(entry.workflow_id, workflow_id);
    assert_eq!(entry.index_path, Path::new("/index/output.txt"));
    assert_eq!(entry.target_path, Path::new("/tmp/output.txt"));
    assert!(entry.created_at <= Utc::now());

    let entries = db
        .list_index_log_entries_by_workflow(workflow_id)
        .await
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, entry_id);
    assert_eq!(entries[0].workflow_id, workflow_id);
    assert_eq!(entries[0].index_path, Path::new("/index/output.txt"));
    assert_eq!(entries[0].target_path, Path::new("/tmp/output.txt"));
    assert_eq!(entries[0].created_at, entry.created_at);
}

#[sqlx::test]
async fn get_nonexistent_invocation(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let id = Uuid::new_v4();
    let retrieved = db.get_invocation(id).await.unwrap();
    assert!(retrieved.is_none());
}

#[sqlx::test]
async fn get_nonexistent_workflow(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let id = Uuid::new_v4();
    let retrieved = db.get_workflow(id).await.unwrap();
    assert!(retrieved.is_none());
}

#[sqlx::test]
async fn list_workflows_for_empty_invocation(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let invocation_id = Uuid::new_v4();
    db.create_invocation(invocation_id, InvocationMethod::Cli, None)
        .await
        .unwrap();

    let workflows = db
        .list_workflows_by_invocation(invocation_id)
        .await
        .unwrap();
    assert_eq!(workflows.len(), 0);
}

#[sqlx::test]
async fn list_workflows_for_nonexistent_invocation(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let invocation_id = Uuid::new_v4();
    let workflows = db
        .list_workflows_by_invocation(invocation_id)
        .await
        .unwrap();
    assert_eq!(workflows.len(), 0);
}

#[sqlx::test]
async fn list_index_entries_for_workflow_with_none(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let invocation_id = Uuid::new_v4();
    db.create_invocation(invocation_id, InvocationMethod::Cli, None)
        .await
        .unwrap();

    let workflow_id = Uuid::new_v4();
    db.create_workflow(
        workflow_id,
        invocation_id,
        String::from("test"),
        String::from("/test.wdl"),
        String::from("{}"),
        String::from("/tmp"),
    )
    .await
    .unwrap();

    let entries = db
        .list_index_log_entries_by_workflow(workflow_id)
        .await
        .unwrap();
    assert_eq!(entries.len(), 0);
}

#[sqlx::test]
async fn list_index_entries_for_nonexistent_workflow(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let workflow_id = Uuid::new_v4();
    let entries = db
        .list_index_log_entries_by_workflow(workflow_id)
        .await
        .unwrap();
    assert_eq!(entries.len(), 0);
}

#[sqlx::test]
async fn create_workflow_with_invalid_invocation_id(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let invocation_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    let result = db
        .create_workflow(
            workflow_id,
            invocation_id,
            String::from("test"),
            String::from("/test.wdl"),
            String::from("{}"),
            String::from("/tmp"),
        )
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    match err {
        sprocket::database::DatabaseError::Sqlx(sqlx::Error::Database(db_err)) => {
            assert!(db_err.message().contains("FOREIGN KEY constraint failed"));
        }
        _ => panic!("Expected foreign key constraint error, got: {:?}", err),
    }
}

#[sqlx::test]
async fn create_index_log_with_invalid_workflow_id(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let workflow_id = Uuid::new_v4();
    let entry_id = Uuid::new_v4();

    let result = db
        .create_index_log_entry(
            entry_id,
            workflow_id,
            String::from("/index/output.txt"),
            String::from("/tmp/output.txt"),
        )
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    match err {
        sprocket::database::DatabaseError::Sqlx(sqlx::Error::Database(db_err)) => {
            assert!(db_err.message().contains("FOREIGN KEY constraint failed"));
        }
        _ => panic!("Expected foreign key constraint error, got: {:?}", err),
    }
}

#[sqlx::test]
async fn list_workflows_ordered_by_created_at(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let invocation_id = Uuid::new_v4();
    db.create_invocation(invocation_id, InvocationMethod::Cli, None)
        .await
        .unwrap();

    let workflow_id_1 = Uuid::new_v4();
    let workflow_id_2 = Uuid::new_v4();
    let workflow_id_3 = Uuid::new_v4();

    db.create_workflow(
        workflow_id_1,
        invocation_id,
        String::from("first"),
        String::from("/test1.wdl"),
        String::from("{}"),
        String::from("/tmp/1"),
    )
    .await
    .unwrap();

    db.create_workflow(
        workflow_id_2,
        invocation_id,
        String::from("second"),
        String::from("/test2.wdl"),
        String::from("{}"),
        String::from("/tmp/2"),
    )
    .await
    .unwrap();

    db.create_workflow(
        workflow_id_3,
        invocation_id,
        String::from("third"),
        String::from("/test3.wdl"),
        String::from("{}"),
        String::from("/tmp/3"),
    )
    .await
    .unwrap();

    let workflows = db
        .list_workflows_by_invocation(invocation_id)
        .await
        .unwrap();

    assert_eq!(workflows.len(), 3);
    assert_eq!(workflows[0].id, workflow_id_1);
    assert_eq!(workflows[1].id, workflow_id_2);
    assert_eq!(workflows[2].id, workflow_id_3);
    assert!(workflows[0].created_at <= workflows[1].created_at);
    assert!(workflows[1].created_at <= workflows[2].created_at);
}

#[sqlx::test]
async fn list_index_entries_ordered_by_created_at(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let invocation_id = Uuid::new_v4();
    db.create_invocation(invocation_id, InvocationMethod::Cli, None)
        .await
        .unwrap();

    let workflow_id = Uuid::new_v4();
    db.create_workflow(
        workflow_id,
        invocation_id,
        String::from("test"),
        String::from("/test.wdl"),
        String::from("{}"),
        String::from("/tmp"),
    )
    .await
    .unwrap();

    let entry_id_1 = Uuid::new_v4();
    let entry_id_2 = Uuid::new_v4();
    let entry_id_3 = Uuid::new_v4();

    db.create_index_log_entry(
        entry_id_1,
        workflow_id,
        String::from("/index/output1.txt"),
        String::from("/tmp/output1.txt"),
    )
    .await
    .unwrap();

    db.create_index_log_entry(
        entry_id_2,
        workflow_id,
        String::from("/index/output2.txt"),
        String::from("/tmp/output2.txt"),
    )
    .await
    .unwrap();

    db.create_index_log_entry(
        entry_id_3,
        workflow_id,
        String::from("/index/output3.txt"),
        String::from("/tmp/output3.txt"),
    )
    .await
    .unwrap();

    let entries = db
        .list_index_log_entries_by_workflow(workflow_id)
        .await
        .unwrap();

    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].id, entry_id_1);
    assert_eq!(entries[1].id, entry_id_2);
    assert_eq!(entries[2].id, entry_id_3);
    assert!(entries[0].created_at <= entries[1].created_at);
    assert!(entries[1].created_at <= entries[2].created_at);
}

#[sqlx::test]
async fn invocation_with_http_method(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let id = Uuid::new_v4();
    let invocation = db
        .create_invocation(id, InvocationMethod::Http, Some(String::from("api_user")))
        .await
        .unwrap();

    assert_eq!(invocation.id, id);
    assert_eq!(invocation.method, InvocationMethod::Http);
    assert_eq!(invocation.created_by, Some(String::from("api_user")));

    let retrieved = db.get_invocation(id).await.unwrap().unwrap();
    assert_eq!(retrieved.method, InvocationMethod::Http);
}

#[sqlx::test]
async fn workflow_status_transitions(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let invocation_id = Uuid::new_v4();
    db.create_invocation(invocation_id, InvocationMethod::Cli, None)
        .await
        .unwrap();

    let workflow_id = Uuid::new_v4();
    let workflow = db
        .create_workflow(
            workflow_id,
            invocation_id,
            String::from("test"),
            String::from("/test.wdl"),
            String::from("{}"),
            String::from("/tmp"),
        )
        .await
        .unwrap();
    assert_eq!(workflow.status, WorkflowStatus::Pending);

    db.update_workflow_status(workflow_id, WorkflowStatus::Running, Some(Utc::now()), None)
        .await
        .unwrap();
    let workflow = db.get_workflow(workflow_id).await.unwrap().unwrap();
    assert_eq!(workflow.status, WorkflowStatus::Running);

    db.update_workflow_status(
        workflow_id,
        WorkflowStatus::Completed,
        None,
        Some(Utc::now()),
    )
    .await
    .unwrap();
    let workflow = db.get_workflow(workflow_id).await.unwrap().unwrap();
    assert_eq!(workflow.status, WorkflowStatus::Completed);

    db.update_workflow_status(workflow_id, WorkflowStatus::Failed, None, Some(Utc::now()))
        .await
        .unwrap();
    let workflow = db.get_workflow(workflow_id).await.unwrap().unwrap();
    assert_eq!(workflow.status, WorkflowStatus::Failed);

    db.update_workflow_status(
        workflow_id,
        WorkflowStatus::Cancelled,
        None,
        Some(Utc::now()),
    )
    .await
    .unwrap();
    let workflow = db.get_workflow(workflow_id).await.unwrap().unwrap();
    assert_eq!(workflow.status, WorkflowStatus::Cancelled);
}

#[sqlx::test]
async fn workflow_with_all_nullable_fields_null(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let invocation_id = Uuid::new_v4();
    db.create_invocation(invocation_id, InvocationMethod::Cli, None)
        .await
        .unwrap();

    let workflow_id = Uuid::new_v4();
    let workflow = db
        .create_workflow(
            workflow_id,
            invocation_id,
            String::from("test"),
            String::from("/test.wdl"),
            String::from("{}"),
            String::from("/tmp"),
        )
        .await
        .unwrap();

    assert_eq!(workflow.outputs, None);
    assert_eq!(workflow.error, None);
    assert_eq!(workflow.started_at, None);
    assert_eq!(workflow.completed_at, None);
}

#[sqlx::test]
async fn multiple_index_entries_for_same_workflow(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let invocation_id = Uuid::new_v4();
    db.create_invocation(invocation_id, InvocationMethod::Cli, None)
        .await
        .unwrap();

    let workflow_id = Uuid::new_v4();
    db.create_workflow(
        workflow_id,
        invocation_id,
        String::from("test"),
        String::from("/test.wdl"),
        String::from("{}"),
        String::from("/tmp"),
    )
    .await
    .unwrap();

    let entry_id_1 = Uuid::new_v4();
    let entry_id_2 = Uuid::new_v4();
    let entry_id_3 = Uuid::new_v4();

    db.create_index_log_entry(
        entry_id_1,
        workflow_id,
        String::from("/index/output1.txt"),
        String::from("/tmp/output1.txt"),
    )
    .await
    .unwrap();

    db.create_index_log_entry(
        entry_id_2,
        workflow_id,
        String::from("/index/output2.txt"),
        String::from("/tmp/output2.txt"),
    )
    .await
    .unwrap();

    db.create_index_log_entry(
        entry_id_3,
        workflow_id,
        String::from("/index/output3.txt"),
        String::from("/tmp/output3.txt"),
    )
    .await
    .unwrap();

    let entries = db
        .list_index_log_entries_by_workflow(workflow_id)
        .await
        .unwrap();
    assert_eq!(entries.len(), 3);
}

#[sqlx::test]
async fn workflow_completion_with_all_timestamps(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let invocation_id = Uuid::new_v4();
    db.create_invocation(invocation_id, InvocationMethod::Cli, None)
        .await
        .unwrap();

    let workflow_id = Uuid::new_v4();
    db.create_workflow(
        workflow_id,
        invocation_id,
        String::from("test"),
        String::from("/test.wdl"),
        String::from("{}"),
        String::from("/tmp"),
    )
    .await
    .unwrap();

    let started = Utc::now();
    let completed = Utc::now();

    db.update_workflow_status(
        workflow_id,
        WorkflowStatus::Completed,
        Some(started),
        Some(completed),
    )
    .await
    .unwrap();

    let workflow = db.get_workflow(workflow_id).await.unwrap().unwrap();
    assert_eq!(workflow.id, workflow_id);
    assert_eq!(workflow.invocation_id, invocation_id);
    assert_eq!(workflow.status, WorkflowStatus::Completed);
    assert!(workflow.started_at.is_some());
    assert!(workflow.completed_at.is_some());
    assert_eq!(workflow.outputs, None);
    assert_eq!(workflow.error, None);
}

#[sqlx::test]
async fn complete_workflow_with_all_fields(pool: SqlitePool) {
    let db = SqliteDatabase::from_pool(pool).await.unwrap();

    let invocation_id = Uuid::new_v4();
    db.create_invocation(
        invocation_id,
        InvocationMethod::Http,
        Some(String::from("user123")),
    )
    .await
    .unwrap();

    let workflow_id = Uuid::new_v4();
    let workflow = db
        .create_workflow(
            workflow_id,
            invocation_id,
            String::from("my_workflow"),
            String::from("/workflows/analysis.wdl"),
            String::from(r#"{"input_file": "data.txt", "threshold": 0.5}"#),
            String::from("/scratch/workflows/run_001"),
        )
        .await
        .unwrap();

    assert_eq!(workflow.status, WorkflowStatus::Pending);
    assert_eq!(workflow.outputs, None);
    assert_eq!(workflow.error, None);
    assert_eq!(workflow.started_at, None);
    assert_eq!(workflow.completed_at, None);

    let started = Utc::now();
    db.update_workflow_status(workflow_id, WorkflowStatus::Running, Some(started), None)
        .await
        .unwrap();

    let running_workflow = db.get_workflow(workflow_id).await.unwrap().unwrap();
    assert_eq!(running_workflow.status, WorkflowStatus::Running);
    assert!(running_workflow.started_at.is_some());
    assert_eq!(running_workflow.completed_at, None);

    db.update_workflow_outputs(
        workflow_id,
        String::from(r#"{"result_file": "output.txt", "count": 42}"#),
    )
    .await
    .unwrap();

    let completed = Utc::now();
    db.update_workflow_status(
        workflow_id,
        WorkflowStatus::Completed,
        Some(started),
        Some(completed),
    )
    .await
    .unwrap();

    let final_workflow = db.get_workflow(workflow_id).await.unwrap().unwrap();
    assert_eq!(final_workflow.id, workflow_id);
    assert_eq!(final_workflow.invocation_id, invocation_id);
    assert_eq!(final_workflow.name, "my_workflow");
    assert_eq!(final_workflow.source, "/workflows/analysis.wdl");
    assert_eq!(final_workflow.status, WorkflowStatus::Completed);
    assert_eq!(
        final_workflow.inputs,
        r#"{"input_file": "data.txt", "threshold": 0.5}"#
    );
    assert_eq!(
        final_workflow.outputs,
        Some(String::from(
            r#"{"result_file": "output.txt", "count": 42}"#
        ))
    );
    assert_eq!(final_workflow.error, None);
    assert_eq!(final_workflow.execution_dir, "/scratch/workflows/run_001");
    assert!(final_workflow.created_at <= started);
    assert!(final_workflow.started_at.is_some());
    assert!(final_workflow.completed_at.is_some());
    assert!(final_workflow.started_at.unwrap() <= final_workflow.completed_at.unwrap());
}
