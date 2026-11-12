//! Integration tests for the `workspace/didRenameFiles` notification.

use async_lsp::lsp_types::CodeLensParams;
use async_lsp::lsp_types::FileRename;
use async_lsp::lsp_types::RenameFilesParams;
use async_lsp::lsp_types::TextDocumentIdentifier;
use async_lsp::lsp_types::notification::DidRenameFiles;
use async_lsp::lsp_types::request::CodeLensRequest;

use crate::common::TestContext;
use crate::common::TestContextBuilder;

async fn setup() -> TestContext {
    let mut ctx = TestContextBuilder::new("code_lens").build();
    ctx.initialize().await;

    ctx
}

#[tokio::test]
async fn should_rename_test_file() {
    let ctx = setup().await;

    // ./example2.yaml -> ./test/example2.yaml
    let old_uri = ctx.doc_uri("example2.yaml");
    let old_path = ctx.doc_path("example2.yaml");
    let new_uri = ctx.doc_uri("test/example2.yml");
    let new_path = ctx.doc_path("test/example2.yml");

    ctx.create_file(
        "example2.yaml",
        r#"greet:
  - name: some_test
"#,
    )
    .await;

    let response = ctx
        .request::<CodeLensRequest>(CodeLensParams {
            text_document: TextDocumentIdentifier {
                uri: old_uri.clone(),
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
        .unwrap();
    assert!(response.is_some(), "test should be recognized");

    // Rename file on disk
    std::fs::rename(old_path, new_path).unwrap();
    ctx.notify::<DidRenameFiles>(RenameFilesParams {
        files: vec![FileRename {
            old_uri: old_uri.to_string(),
            new_uri: new_uri.to_string(),
        }],
    })
    .unwrap();

    // Old URI should have no code lenses
    let response_old = ctx
        .request::<CodeLensRequest>(CodeLensParams {
            text_document: TextDocumentIdentifier {
                uri: old_uri.clone(),
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
        .unwrap();
    assert!(response_old.is_none());

    // New URI should have code lenses
    let response_new = ctx
        .request::<CodeLensRequest>(CodeLensParams {
            text_document: TextDocumentIdentifier {
                uri: new_uri.clone(),
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
        .unwrap();
    assert!(response_new.is_some());
}
