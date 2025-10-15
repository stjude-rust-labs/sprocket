//! Integration tests for the `textDocument/rename` request.

use pretty_assertions::assert_eq;
use tower_lsp::lsp_types::*;

mod common;
use common::TestContext;
use tower_lsp::lsp_types::request::Rename;

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

    const NEW_NAME: &str = "renamedTask";

    let edit = rename_request(&mut ctx, "source.wdl", Position::new(10, 13), NEW_NAME)
        .await
        .unwrap();

    let changes = edit.changes.expect("expected changes");
    assert!(changes.iter().any(|(uri, edits)| {
        uri.to_file_path()
            .ok()
            .and_then(|p| p.file_name().map(|n| n == "source.wdl"))
            .unwrap_or(false)
            && edits.iter().any(|e| e.new_text == NEW_NAME)
    }));
    assert!(changes.keys().any(|u| {
        u.to_file_path()
            .ok()
            .and_then(|p| p.file_name().map(|n| n == "foo.wdl"))
            .unwrap_or(false)
    }));
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

#[tokio::test]
async fn should_not_rename_shadowed_declaration() {
    let mut ctx = TestContext::new("rename");
    ctx.initialize().await;

    const NEW_NAME: &str = "renamed_out_dir";

    let edit = rename_request(&mut ctx, "shadowed.wdl", Position::new(10, 22), NEW_NAME)
        .await
        .unwrap();

    let changes = edit.changes.expect("expected changes");
    let edits = changes
        .get(&ctx.doc_uri("shadowed.wdl"))
        .expect("should have edits for shadowed.wdl");

    let expected_edits = vec![
        TextEdit {
            range: Range::new(Position::new(10, 20), Position::new(10, 32)),
            new_text: NEW_NAME.to_string(),
        },
        TextEdit {
            range: Range::new(Position::new(18, 44), Position::new(18, 56)),
            new_text: NEW_NAME.to_string(),
        },
    ];

    assert_eq!(
        edits.len(),
        expected_edits.len(),
        "should have exactly two edits (the output declaration and its usage)"
    );

    for edit in &expected_edits {
        assert!(edits.contains(edit), "missing expected edit: {:?}", edit);
    }
}
