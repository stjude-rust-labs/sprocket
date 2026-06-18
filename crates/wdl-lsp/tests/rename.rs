//! Integration tests for the `textDocument/rename` request.

use std::collections::HashMap;

use async_lsp::lsp_types::*;
use pretty_assertions::assert_eq;
use serde_json::Value;

pub mod common;
use async_lsp::lsp_types::request::Rename;
use common::TestContext;

async fn rename_request(
    ctx: &mut TestContext,
    path: &str,
    position: Position,
    new_name: &str,
) -> async_lsp::Result<Option<WorkspaceEdit>> {
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

fn workspace_edit_to_changes(edit: &WorkspaceEdit) -> HashMap<Url, Vec<TextEdit>> {
    let edit = serde_json::to_value(edit).expect("workspace edit should serialize");
    let mut changes = changes_from_legacy_map(&edit);
    if changes.is_empty() {
        changes = changes_from_document_changes(&edit);
    }

    changes
}

fn changes_from_legacy_map(edit: &Value) -> HashMap<Url, Vec<TextEdit>> {
    let mut changes = HashMap::new();

    let Some(entries) = edit.get("changes").and_then(Value::as_object) else {
        return changes;
    };

    for (uri, edits) in entries {
        let uri = Url::parse(uri).expect("workspace edit URI should parse");
        let edits = serde_json::from_value(edits.clone()).expect("text edits should deserialize");
        changes.insert(uri, edits);
    }

    changes
}

fn changes_from_document_changes(edit: &Value) -> HashMap<Url, Vec<TextEdit>> {
    let mut changes = HashMap::new();

    let Some(document_changes) = edit.get("documentChanges").and_then(Value::as_array) else {
        return changes;
    };

    for document_change in document_changes {
        let Some(uri) = document_change
            .get("textDocument")
            .and_then(|text_document| text_document.get("uri"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let Some(edits) = document_change.get("edits").and_then(Value::as_array) else {
            continue;
        };

        let uri = Url::parse(uri).expect("workspace edit URI should parse");
        let edits = edits
            .iter()
            .filter_map(|edit| serde_json::from_value(edit.clone()).ok())
            .collect();
        changes.insert(uri, edits);
    }

    changes
}

#[tokio::test]
async fn should_rename_workspace_wide() {
    let mut ctx = TestContext::new("rename");
    ctx.initialize().await;

    const NEW_NAME: &str = "renamedTask";

    let edit = rename_request(&mut ctx, "source.wdl", Position::new(10, 13), NEW_NAME)
        .await
        .expect("request should succeed")
        .unwrap();

    let changes = workspace_edit_to_changes(&edit);
    assert!(!changes.is_empty(), "expected changes");
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

    let result = rename_request(&mut ctx, "source.wdl", Position::new(10, 13), "1notValid")
        .await
        .expect("request should succeed");

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
    .expect("request should succeed")
    .unwrap();

    let changes = workspace_edit_to_changes(&edit);
    assert!(!changes.is_empty(), "expected changes");
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
        .expect("request should succeed")
        .unwrap();

    let changes = workspace_edit_to_changes(&edit);
    assert!(!changes.is_empty(), "expected changes");
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
        .expect("request should succeed")
        .unwrap();

    let changes = workspace_edit_to_changes(&edit);
    assert!(!changes.is_empty(), "expected changes");
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

#[tokio::test]
async fn should_rename_local_variable_used_in_output_section() {
    let mut ctx = TestContext::new("rename");
    ctx.initialize().await;

    const NEW_NAME: &str = "greeting";

    let edit = rename_request(&mut ctx, "local_output.wdl", Position::new(3, 11), NEW_NAME)
        .await
        .expect("request should succeed")
        .unwrap();

    let changes = workspace_edit_to_changes(&edit);
    assert!(!changes.is_empty(), "expected changes");
    assert_eq!(changes.len(), 1);

    let edits = changes
        .get(&ctx.doc_uri("local_output.wdl"))
        .expect("should have edits for local_output.wdl");

    let expected_edits = vec![
        TextEdit {
            range: Range::new(Position::new(3, 11), Position::new(3, 12)),
            new_text: NEW_NAME.to_string(),
        },
        TextEdit {
            range: Range::new(Position::new(6, 21), Position::new(6, 22)),
            new_text: NEW_NAME.to_string(),
        },
    ];

    assert_eq!(edits.len(), expected_edits.len());
    for edit in &expected_edits {
        assert!(edits.contains(edit), "missing expected edit: {:?}", edit);
    }
}

#[tokio::test]
async fn should_rename_enum() {
    let mut ctx = TestContext::new("rename");
    ctx.initialize().await;

    // Position of `Status` in `enum Status`
    let edit = rename_request(&mut ctx, "enum.wdl", Position::new(2, 7), "State")
        .await
        .expect("request should succeed")
        .unwrap();

    let changes = workspace_edit_to_changes(&edit);
    assert!(!changes.is_empty(), "expected changes");
    let edits = changes
        .get(&ctx.doc_uri("enum.wdl"))
        .expect("should have edits for enum.wdl");

    assert_eq!(edits.len(), 5); // enum definition + two variable declarations + two member access
}

#[tokio::test]
async fn should_rename_enum_variant() {
    let mut ctx = TestContext::new("rename");
    ctx.initialize().await;

    // Position of `Active` in variant definition
    let edit = rename_request(&mut ctx, "enum.wdl", Position::new(3, 4), "Running")
        .await
        .expect("request should succeed")
        .unwrap();

    let changes = workspace_edit_to_changes(&edit);
    assert!(!changes.is_empty(), "expected changes");
    let edits = changes
        .get(&ctx.doc_uri("enum.wdl"))
        .expect("should have edits for enum.wdl");

    assert_eq!(edits.len(), 2); // variant definition + one usage
}

#[tokio::test]
async fn should_rename_imported_type_alias() {
    let mut ctx = TestContext::new("rename");
    ctx.initialize().await;

    let edit = rename_request(&mut ctx, "aliases.wdl", Position::new(2, 39), "Patient")
        .await
        .expect("request should succeed")
        .unwrap();

    let changes = workspace_edit_to_changes(&edit);
    assert!(!changes.is_empty(), "expected changes");
    assert_eq!(changes.len(), 1);

    let edits = changes
        .get(&ctx.doc_uri("aliases.wdl"))
        .expect("should have edits for aliases.wdl");

    let expected_edits = vec![
        TextEdit {
            range: Range::new(Position::new(2, 37), Position::new(2, 42)),
            new_text: "Patient".to_string(),
        },
        TextEdit {
            range: Range::new(Position::new(7, 8), Position::new(7, 13)),
            new_text: "Patient".to_string(),
        },
    ];

    assert_eq!(edits.len(), expected_edits.len());
    for edit in &expected_edits {
        assert!(edits.contains(edit), "missing expected edit: {:?}", edit);
    }
}

#[tokio::test]
async fn should_rename_call_alias() {
    let mut ctx = TestContext::new("rename");
    ctx.initialize().await;

    let edit = rename_request(&mut ctx, "aliases.wdl", Position::new(10, 26), "job")
        .await
        .expect("request should succeed")
        .unwrap();

    let changes = workspace_edit_to_changes(&edit);
    assert!(!changes.is_empty(), "expected changes");
    assert_eq!(changes.len(), 1);

    let edits = changes
        .get(&ctx.doc_uri("aliases.wdl"))
        .expect("should have edits for aliases.wdl");

    let expected_edits = vec![
        TextEdit {
            range: Range::new(Position::new(10, 24), Position::new(10, 30)),
            new_text: "job".to_string(),
        },
        TextEdit {
            range: Range::new(Position::new(13, 24), Position::new(13, 30)),
            new_text: "job".to_string(),
        },
    ];

    assert_eq!(edits.len(), expected_edits.len());
    for edit in &expected_edits {
        assert!(edits.contains(edit), "missing expected edit: {:?}", edit);
    }
}
