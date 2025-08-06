//! Integration tests for the `textDocument/rename` request.

use tower_lsp::lsp_types::*;

mod common;
use common::TestContext;
use tower_lsp::lsp_types::request::Rename;

const NEW_NAME: &str = "renamedTask";

async fn rename_request(
    ctx: &mut TestContext,
    path: &str,
    position: Position,
    new_name: &str,
) -> Option<WorkspaceEdit> {
    ctx.request::<Rename>(RenameParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: ctx.doc_uri(path),
            },
            position,
        },
        new_name: new_name.to_string(),
        work_done_progress_params: Default::default(),
    })
    .await
}

#[tokio::test]
async fn should_rename_workspace_wide() {
    let mut ctx = TestContext::new("rename");
    ctx.initialize().await;

    let edit = rename_request(&mut ctx, "source.wdl", Position::new(10, 13), NEW_NAME)
        .await
        .unwrap();

    let changes = edit.changes.expect("expected changes");
    assert!(changes.keys().any(|u| {
        u.to_file_path()
            .ok()
            .and_then(|p| p.file_name().map(|n| n == "source.wdl"))
            .unwrap_or(false)
    }));
    assert!(changes.keys().any(|u| {
        u.to_file_path()
            .ok()
            .and_then(|p| p.file_name().map(|n| n == "foo.wdl"))
            .unwrap_or(false)
    }));

    for edits in changes.values() {
        for e in edits {
            assert_eq!(e.new_text, NEW_NAME);
        }
    }
}

#[tokio::test]
async fn should_reject_invalid_identifier() {
    let mut ctx = TestContext::new("rename");
    ctx.initialize().await;

    let result = rename_request(&mut ctx, "source.wdl", Position::new(10, 13), "1notValid").await;

    assert!(result.is_none());
}

#[tokio::test]
async fn should_rename_struct_definition() {
    let mut ctx = TestContext::new("rename");
    ctx.initialize().await;

    let edit = rename_request(
        &mut ctx,
        "structs.wdl",
        Position::new(2, 9),
        "PersonRenamed",
    )
    .await
    .unwrap();

    let changes = edit.changes.expect("expected changes");
    assert!(changes.keys().any(|u| {
        u.to_file_path()
            .ok()
            .and_then(|p| p.file_name().map(|n| n == "structs.wdl"))
            .unwrap_or(false)
    }));
    assert!(changes.keys().any(|u| {
        u.to_file_path()
            .ok()
            .and_then(|p| p.file_name().map(|n| n == "foo.wdl"))
            .unwrap_or(false)
    }));
}

#[tokio::test]
async fn should_rename_import_namespace_alias() {
    let mut ctx = TestContext::new("rename");
    ctx.initialize().await;

    let edit = rename_request(&mut ctx, "source.wdl", Position::new(3, 22), "libx")
        .await
        .unwrap();

    let changes = edit.changes.expect("expected changes");
    assert!(changes.keys().any(|u| {
        u.to_file_path()
            .ok()
            .and_then(|p| p.file_name().map(|n| n == "source.wdl"))
            .unwrap_or(false)
    }));
}
