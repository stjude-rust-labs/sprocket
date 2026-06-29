//! Integration tests for the `workspace/didChangeWatchedFiles` notification.

use async_lsp::lsp_types::DidChangeWatchedFilesParams;
use async_lsp::lsp_types::DocumentSymbolParams;
use async_lsp::lsp_types::FileChangeType;
use async_lsp::lsp_types::FileEvent;
use async_lsp::lsp_types::TextDocumentIdentifier;
use async_lsp::lsp_types::notification::DidChangeWatchedFiles;
use async_lsp::lsp_types::request::DocumentSymbolRequest;
use url::Url;

use crate::common::TestContext;
use crate::common::TestContextBuilder;
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
async fn should_change_watched_files() {
    let mut ctx = TestContextBuilder::new("call_hierarchy").build();
    ctx.initialize().await;

    // In current workspace
    let third_wdl = ctx.doc_uri("third.wdl");
    assert_document_symbol_request(&mut ctx, third_wdl.clone(), true).await;

    // In another workspace, unknown to the server
    let find_references_workspace = get_workspace_path("find_references");
    let enum_wdl = Url::from_file_path(find_references_workspace.join("enum.wdl")).unwrap();
    assert_document_symbol_request(&mut ctx, enum_wdl.clone(), false).await;

    ctx.notify::<DidChangeWatchedFiles>(DidChangeWatchedFilesParams {
        changes: vec![
            FileEvent {
                uri: enum_wdl.clone(),
                typ: FileChangeType::CREATED,
            },
            FileEvent {
                uri: third_wdl.clone(),
                typ: FileChangeType::DELETED,
            },
        ],
    })
    .expect("failed to send notification");

    // Now the results should be flipped
    assert_document_symbol_request(&mut ctx, enum_wdl, true).await;
    assert_document_symbol_request(&mut ctx, third_wdl, false).await;
}

#[tokio::test]
async fn should_ignore_non_wdl_files() {
    let mut ctx = TestContextBuilder::new("call_hierarchy").build();
    ctx.initialize().await;

    let random_file_path = ctx.doc_path("foo.txt");
    let random_file_url = Url::from_file_path(random_file_path.clone()).unwrap();
    std::fs::write(random_file_path, "Hello, world!").expect("failed to write file");

    ctx.notify::<DidChangeWatchedFiles>(DidChangeWatchedFilesParams {
        changes: vec![FileEvent {
            uri: random_file_url.clone(),
            typ: FileChangeType::CREATED,
        }],
    })
    .expect("failed to send notification");

    // Shouldn't care about a non-WDL file
    assert_document_symbol_request(&mut ctx, random_file_url, false).await;

    let new_wdl = ctx.doc_path("new.wdl");
    let new_wdl_url = Url::from_file_path(new_wdl.clone()).unwrap();
    std::fs::write(new_wdl, "version 1.3\ntask foo {}").expect("failed to write file");

    ctx.notify::<DidChangeWatchedFiles>(DidChangeWatchedFilesParams {
        changes: vec![FileEvent {
            uri: new_wdl_url.clone(),
            typ: FileChangeType::CREATED,
        }],
    })
    .expect("failed to send notification");

    // Should respect WDL file additions
    assert_document_symbol_request(&mut ctx, new_wdl_url, true).await;
}
