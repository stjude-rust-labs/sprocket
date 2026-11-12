//! Integration tests for the `workspace/didDeleteFiles` notification.

use async_lsp::lsp_types::CodeLensParams;
use async_lsp::lsp_types::TextDocumentIdentifier;
use async_lsp::lsp_types::request::CodeLensRequest;

use crate::common::TestContext;
use crate::common::TestContextBuilder;

async fn setup() -> TestContext {
    let mut ctx = TestContextBuilder::new("code_lens").build();
    ctx.initialize().await;

    ctx
}

#[tokio::test]
async fn should_delete_test_file() {
    let ctx = setup().await;

    let test_uri = ctx.doc_uri("example2.yaml");

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
                uri: test_uri.clone(),
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
        .unwrap();
    assert!(response.is_some(), "test should be recognized");

    // Delete file from disk
    ctx.delete_file("example2.yaml").await;

    // Code lens should now be None
    let response = ctx
        .request::<CodeLensRequest>(CodeLensParams {
            text_document: TextDocumentIdentifier {
                uri: test_uri.clone(),
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
        .unwrap();
    assert!(response.is_none(), "test should be removed");
}
