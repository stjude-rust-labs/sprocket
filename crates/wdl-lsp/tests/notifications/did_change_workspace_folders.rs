//! Integration tests for the `workspace/didChangeWorkspaceFolders`
//! notification.

use async_lsp::lsp_types::DidChangeWorkspaceFoldersParams;
use async_lsp::lsp_types::DocumentSymbolParams;
use async_lsp::lsp_types::TextDocumentIdentifier;
use async_lsp::lsp_types::WorkspaceFolder;
use async_lsp::lsp_types::WorkspaceFoldersChangeEvent;
use async_lsp::lsp_types::notification::DidChangeWorkspaceFolders;
use async_lsp::lsp_types::request::DocumentSymbolRequest;
use url::Url;

use crate::common::TestContext;
use crate::common::TestContextBuilder;
use crate::common::WORKSPACE_FOLDER_NAME;
use crate::common::get_workspace_path;

async fn assert_document_symbol_request(
    ctx: &mut TestContext,
    document: Url,
    should_have_result: bool,
) {
    let result = ctx
        .request::<DocumentSymbolRequest>(DocumentSymbolParams {
            text_document: TextDocumentIdentifier { uri: document },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
        .expect("request should succeed");

    if should_have_result {
        result.expect("result should not be empty");
    } else {
        assert!(result.is_none(), "result should be empty");
    }
}

#[tokio::test]
async fn should_change_workspace_folders() {
    let mut ctx = TestContextBuilder::new("call_hierarchy").build();
    ctx.initialize().await;

    // In current workspace
    let third_wdl = ctx.doc_uri("third.wdl");
    assert_document_symbol_request(&mut ctx, third_wdl.clone(), true).await;

    // In another workspace, unknown to the server
    let find_references_workspace = get_workspace_path("find_references");
    let enum_wdl = Url::from_file_path(find_references_workspace.join("enum.wdl")).unwrap();
    assert_document_symbol_request(&mut ctx, enum_wdl.clone(), false).await;

    ctx.notify::<DidChangeWorkspaceFolders>(DidChangeWorkspaceFoldersParams {
        event: WorkspaceFoldersChangeEvent {
            added: vec![WorkspaceFolder {
                uri: Url::from_file_path(find_references_workspace).unwrap(),
                name: WORKSPACE_FOLDER_NAME.to_string(),
            }],
            removed: vec![WorkspaceFolder {
                uri: ctx.workspace_uri(),
                name: WORKSPACE_FOLDER_NAME.to_string(),
            }],
        },
    })
    .expect("failed to send notification");

    // Now the results should be flipped
    assert_document_symbol_request(&mut ctx, enum_wdl, true).await;
    assert_document_symbol_request(&mut ctx, third_wdl, false).await;
}
